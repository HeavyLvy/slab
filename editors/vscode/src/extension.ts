import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';

import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from 'vscode-languageclient/node';

const INSTALL_URL = 'https://heavylvy.github.io/slab/guide/getting-started.html#install';
const EXE = process.platform === 'win32' ? 'slab-lsp.exe' : 'slab-lsp';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const command = findServer(context);
  const start = () => {
    if (command === undefined) {
      return;
    }
    client = new LanguageClient(
      'slab',
      'slab',
      { run: { command }, debug: { command, args: ['-v'] } } as ServerOptions,
      {
        documentSelector: [{ language: 'slab', scheme: 'file' }],
        synchronize: { fileEvents: vscode.workspace.createFileSystemWatcher('**/main.slab') },
      } as LanguageClientOptions,
    );
    void client.start();
  };

  if (command === undefined) {
    // Highlighting and snippets still work without the server; say how to get the rest.
    void vscode.window
      .showWarningMessage(
        'slab: the language server (slab-lsp) was not found, so there are no diagnostics, hover or completion. Install it, or set "slab.server.path".',
        'How to install',
      )
      .then((pick) => {
        if (pick === 'How to install') {
          void vscode.env.openExternal(vscode.Uri.parse(INSTALL_URL));
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

function isFile(candidate: string): boolean {
  try {
    return fs.statSync(candidate).isFile();
  } catch {
    return false;
  }
}

/// The first directory of PATH (plus the places `cargo install` and the install script put
/// binaries, which a VS Code started from a launcher often does not have on PATH) with the server.
function onPath(): string | undefined {
  const dirs = (process.env.PATH ?? '').split(path.delimiter).filter((d) => d.length > 0);
  dirs.push(path.join(os.homedir(), '.cargo', 'bin'), path.join(os.homedir(), '.local', 'bin'));
  for (const dir of dirs) {
    const candidate = path.join(dir, EXE);
    if (isFile(candidate)) {
      return candidate;
    }
  }
  return undefined;
}

/// Where the server executable lives, in this order:
///  1. the "slab.server.path" setting (absolute, or relative to a workspace folder; only in a
///     trusted workspace, since a workspace can set it),
///  2. the server bundled in this extension (the platform-specific packages),
///  3. a build in a workspace folder's target/ (for working on slab itself; trusted workspaces
///     only, because an untrusted folder could hold any program),
///  4. PATH, ~/.cargo/bin and ~/.local/bin.
function findServer(context: vscode.ExtensionContext): string | undefined {
  const trusted = vscode.workspace.isTrusted;
  const folders = vscode.workspace.workspaceFolders?.map((f) => f.uri.fsPath) ?? [];
  const setting = vscode.workspace.getConfiguration('slab').get<string>('server.path')?.trim();
  if (setting && trusted) {
    const candidates = path.isAbsolute(setting) ? [setting] : folders.map((f) => path.join(f, setting));
    const found = candidates.find(isFile);
    if (found) {
      return found;
    }
    // a bare name such as "slab-lsp" is looked up on PATH by the client
    if (!setting.includes('/') && !setting.includes('\\')) {
      return setting;
    }
  }
  const bundled = path.join(context.extensionPath, 'server', EXE);
  if (isFile(bundled)) {
    if (process.platform !== 'win32') {
      try {
        // a .vsix is a zip, which does not reliably keep the executable bit
        fs.chmodSync(bundled, 0o755);
      } catch {
        // read-only install: it either already works or the message below says it does not
      }
    }
    return bundled;
  }
  if (trusted) {
    for (const folder of folders) {
      for (const profile of ['release', 'debug']) {
        const candidate = path.join(folder, 'target', profile, EXE);
        if (isFile(candidate)) {
          return candidate;
        }
      }
    }
  }
  return onPath();
}
