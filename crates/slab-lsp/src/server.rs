//! The Language Server Protocol loop over stdio: `lsp-server` provides the transport, and
//! `serde_json` values carry the payloads (the crate gets no `lsp_types`). Text sync is
//! full-document; every open/change rebuilds the project synchronously and republishes all
//! diagnostics.

use std::process::ExitCode;

use lsp_server::{Connection, Message, Notification, Request, Response};
use serde_json::{Value, json};

use crate::completion;
use crate::lines::{Position, Range};
use crate::state::{Workspace, path_to_uri};
use crate::symbols;
use crate::tokens;

pub fn run() -> ExitCode {
    let (connection, io_threads) = Connection::stdio();
    let result = serve(&connection);
    // The io threads unblock only when every channel end is gone.
    drop(connection);
    let _ = io_threads.join();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("slab-lsp: {e}");
            ExitCode::FAILURE
        }
    }
}

fn serve(connection: &Connection) -> Result<(), String> {
    let capabilities = json!({
        "positionEncoding": "utf-8",
        "textDocumentSync": { "openClose": true, "change": 1 },
        "hoverProvider": true,
        "definitionProvider": true,
        "documentSymbolProvider": true,
        "completionProvider": { "triggerCharacters": [".", ":"], "resolveProvider": false },
        "semanticTokensProvider": {
            "legend": {
                "tokenTypes": tokens::TOKEN_TYPES,
                "tokenModifiers": tokens::TOKEN_MODIFIERS,
            },
            "full": true,
        },
    });
    connection.initialize(capabilities).map_err(|e| e.to_string())?;

    let mut ws = Workspace::default();
    for msg in &connection.receiver {
        match msg {
            Message::Request(ref request) => {
                if connection.handle_shutdown(request).map_err(|e| e.to_string())? {
                    break;
                }
                let Request { id, method, params } = request;
                let result: Value = match method.as_str() {
                    "textDocument/hover" => hover(&ws, params),
                    "textDocument/definition" => definition(&ws, params),
                    "textDocument/completion" => completion_response(&ws, params),
                    "textDocument/documentSymbol" => document_symbols(&ws, params),
                    "textDocument/semanticTokens/full" => semantic_tokens(&ws, params),
                    other => {
                        eprintln!("slab-lsp: ignoring request `{other}`");
                        Value::Null
                    }
                };
                connection.sender.send(Message::Response(Response::new_ok(id.clone(), result))).map_err(|e| e.to_string())?;
            }
            Message::Notification(ref notification) => {
                let Notification { method, params } = notification;
                if method == "exit" {
                    break;
                }
                let publishes = match method.as_str() {
                    "textDocument/didOpen" => did_open(&mut ws, params),
                    "textDocument/didChange" => did_change(&mut ws, params),
                    "textDocument/didClose" => {
                        let uri = param_uri(params).unwrap_or_default().to_owned();
                        ws.close(&uri)
                    }
                    "textDocument/didSave" => Vec::new(),
                    "$/setTrace" | "workspace/didChangeConfiguration" | "workspace/didChangeWatchedFiles" => Vec::new(),
                    other => {
                        eprintln!("slab-lsp: ignoring notification `{other}`");
                        Vec::new()
                    }
                };
                publish(connection, publishes);
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ notifications

fn did_open(ws: &mut Workspace, params: &Value) -> Vec<crate::state::Publish> {
    let td = params.get("textDocument");
    let uri = td.and_then(|t| t.get("uri")).and_then(Value::as_str);
    let text = td.and_then(|t| t.get("text")).and_then(Value::as_str);
    match (uri, text) {
        (Some(uri), Some(text)) => ws.open(uri, text, version_of(td.unwrap())),
        _ => Vec::new(),
    }
}

fn did_change(ws: &mut Workspace, params: &Value) -> Vec<crate::state::Publish> {
    let td = params.get("textDocument");
    let uri = td.and_then(|t| t.get("uri")).and_then(Value::as_str);
    // Full sync: exactly one (usually) content change per notice; take the last.
    let changes = params.get("contentChanges").and_then(Value::as_array);
    let text = changes
        .and_then(|c| c.last())
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str);
    match (uri, text) {
        (Some(uri), Some(text)) => ws.change(uri, text, version_of(td.unwrap())),
        _ => Vec::new(),
    }
}

fn version_of(td: &Value) -> i32 {
    td.get("version").and_then(Value::as_i64).unwrap_or(0) as i32
}

fn publish(connection: &Connection, publishes: Vec<crate::state::Publish>) {
    for p in publishes {
        let diagnostics: Vec<Value> = p
            .diagnostics
            .iter()
            .map(|d| json!({ "range": range_value(d.range), "severity": d.severity as i64, "source": "slab", "message": d.message }))
            .collect();
        let mut params = json!({ "uri": p.uri, "diagnostics": diagnostics });
        if let Some(v) = p.version {
            params["version"] = json!(v);
        }
        let _ = connection
            .sender
            .send(Message::Notification(Notification::new("textDocument/publishDiagnostics".to_owned(), params)));
    }
}

// ------------------------------------------------------------------ requests

/// The (project, document, module) a request points at, when the project and module resolve.
fn ctx<'a, 'b>(ws: &'a Workspace, params: &'b Value) -> Option<(&'a crate::state::ProjectState, &'a crate::state::Doc, usize)> {
    let uri = param_uri(params)?;
    let (state, doc, module) = ws.locate(uri)?;
    Some((state, doc, module?))
}

fn hover(ws: &Workspace, params: &Value) -> Value {
    let Some((state, doc, module)) = ctx(ws, params) else { return Value::Null };
    let Some(offset) = param_position(params).and_then(|p| doc.lines.offset(&doc.text, p)) else { return Value::Null };
    let Some(md) = state.hover(module, offset) else { return Value::Null };
    json!({ "contents": { "kind": "markdown", "value": md } })
}

fn definition(ws: &Workspace, params: &Value) -> Value {
    let Some((state, doc, module)) = ctx(ws, params) else { return Value::Null };
    let Some(offset) = param_position(params).and_then(|p| doc.lines.offset(&doc.text, p)) else { return Value::Null };
    let Some(def) = state.goto_definition(module, offset) else { return Value::Null };
    let text = &state.sources.files[def.file_idx].text;
    let range = state.lines[def.file_idx].range(text, def.span);
    let uri = path_to_uri(std::path::Path::new(state.sources.files[def.file_idx].name.as_str()));
    json!([{ "uri": uri, "range": range_value(range) }])
}

fn completion_response(ws: &Workspace, params: &Value) -> Value {
    let Some(uri) = param_uri(params) else { return Value::Null };
    let Some(pos) = param_position(params) else { return Value::Null };
    // The offset is built from the document the client has (== our overlay).
    let Some((_state, doc, _module)) = ws.locate(uri) else { return Value::Null };
    let Some(offset) = doc.lines.offset(&doc.text, pos) else { return Value::Null };
    let Some(items) = completion::completions(ws, uri, offset) else { return Value::Null };
    let json_items: Vec<Value> = items
        .into_iter()
        .map(|i| {
            let mut o = json!({ "label": i.label, "kind": i.kind as i64 });
            if let Some(d) = &i.detail {
                o["detail"] = json!(d);
            }
            if let Some(d) = &i.docs {
                o["documentation"] = json!({ "kind": "markdown", "value": d });
            }
            if let Some(insert) = &i.insert {
                o["insertText"] = json!(insert);
                o["insertTextFormat"] = json!(2);
            }
            o
        })
        .collect();
    json!({ "isIncomplete": false, "items": json_items })
}

fn document_symbols(ws: &Workspace, params: &Value) -> Value {
    let Some((state, _doc, module)) = ctx(ws, params) else { return Value::Null };
    fn build(s: &symbols::Symbol) -> Value {
        let mut o = json!({
            "name": s.name,
            "kind": s.kind as i64,
            "range": range_value(s.range),
            "selectionRange": range_value(s.selection),
        });
        if let Some(d) = &s.detail {
            o["detail"] = json!(d);
        }
        if !s.children.is_empty() {
            o["children"] = json!(s.children.iter().map(build).collect::<Vec<_>>());
        }
        o
    }
    json!(symbols::document_symbols(state, module).iter().map(build).collect::<Vec<_>>())
}

fn semantic_tokens(ws: &Workspace, params: &Value) -> Value {
    let Some((state, doc, module)) = ctx(ws, params) else { return Value::Null };
    let program = &state.modules[module].program;
    let found = tokens::document_tokens(&doc.text, program);
    let mut data: Vec<u64> = Vec::with_capacity(found.len() * 5);
    let mut prev = Position { line: 0, character: 0 };
    for t in found {
        let start = doc.lines.position(&doc.text, t.start);
        let delta_line = start.line - prev.line;
        let delta_char = if delta_line == 0 { start.character - prev.character } else { start.character };
        // Token length in characters (the negotiated encoding).
        let text = &doc.text[t.start..(t.start + t.len).min(doc.text.len())];
        data.extend([delta_line as u64, delta_char as u64, text.chars().count() as u64, t.ty.index() as u64, t.mods as u64]);
        prev = start;
    }
    json!({ "data": data })
}

// ------------------------------------------------------------------ parameter plumbing

fn param_uri(params: &Value) -> Option<&str> {
    params.pointer("/textDocument/uri").and_then(Value::as_str)
}

fn param_position(params: &Value) -> Option<Position> {
    let p = params.pointer("/position")?;
    Some(Position { line: p.get("line")?.as_u64()? as u32, character: p.get("character")?.as_u64()? as u32 })
}

fn range_value(r: Range) -> Value {
    json!({
        "start": { "line": r.start.line, "character": r.start.character },
        "end": { "line": r.end.line, "character": r.end.character },
    })
}