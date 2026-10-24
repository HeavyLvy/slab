import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';

import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const serverPath = findServer(context);
  const command = serverPath ?? 'slab-lsp';
  const start = () => {
    client = new LanguageClient(
      'slab',
      'slab',
      { run: { command }, debug: { command, args: ['-v'] } } as ServerOptions,
      {
        documentSelector: [{ language: 'slab', scheme: 'file' }],
        synchronize: { fileEvents: vscode.workspace.createFileSystemWatcher('**/main.slab') },
      } as LanguageClientOptions,
    );
    client.start();
  };

  if (serverPath === undefined) {
    // The binary is not where we expect it: tell the user, but try PATH anyway.
    vscode.window
      .showErrorMessage(
        'slab-lsp not found: build it (`cargo build --release -p slab-lsp`) and set "slab.server.path", or put slab-lsp on your PATH.',
        'Build it now',
      )
      .then((pick) => {
        if (pick === 'Build it now') {
          const root = vscode.workspace.workspaceFolders?.[0];
          if (root) {
            const term = vscode.window.createTerminal({ name: 'slab-lsp' });
            term.sendText('cargo build --release -p slab-lsp', true);
            term.show();
          }
        }
      });
  }
  start();

  context.subscriptions.push(
    vscode.commands.registerCommand('slab.restartServer', () => {
      const restart = async () => {
        await client?.stop();
        start();
      };
      void restart();
    }),
  );
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}

/// Where the server executable lives: the "slab.server.path" setting (absolute, or a bare
/// name looked up on PATH), otherwise a release build in any workspace folder, otherwise
/// undefined (PATH lookup only).
function findServer(context: vscode.ExtensionContext): string | undefined {
  const setting = vscode.workspace.getConfiguration('slab').get<string>('server.path')?.trim();
  const workspaceFolders = vscode.workspace.workspaceFolders?.map((f) => f.uri.fsPath) ?? [];
  const candidates: string[] = [];
  if (setting) {
    candidates.push(setting);
    for (const folder of workspaceFolders) {
      candidates.push(path.join(folder, setting));
    }
  }
  for (const folder of workspaceFolders) {
    for (const profile of ['release', 'debug']) {
      candidates.push(path.join(folder, 'target', profile, 'slab-lsp'));
    }
  }
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return undefined;
}