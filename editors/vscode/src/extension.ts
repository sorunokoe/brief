/**
 * Brief VS Code Extension
 *
 * Activates the Brief LSP server (`brief lsp`) when a `.brief` file is opened,
 * and connects VS Code to it via the Language Client protocol.
 *
 * Features provided by the LSP server:
 *   - Diagnostics: all E/W codes on open/change/save
 *   - Hover: effect function signatures on `perform` calls
 *
 * Additional features:
 *   - Status bar: shows .brief.lock freshness (🔒 sealed / ⚠ stale / ✗ missing)
 *   - Commands: brief.verify, brief.serve, brief.check
 *
 * Configuration:
 *   "brief.serverPath" — path to the `brief` binary (default: "brief" on $PATH)
 *   "brief.trace.server" — LSP trace level ("off" | "messages" | "verbose")
 */

import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
  Trace,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let lockStatusItem: vscode.StatusBarItem | undefined;
let lockWatcher: vscode.FileSystemWatcher | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const config     = vscode.workspace.getConfiguration('brief');
  const serverPath = config.get<string>('serverPath') ?? 'brief';
  const traceLevel = config.get<string>('trace.server') ?? 'off';

  // ── Server options ────────────────────────────────────────────────────────
  const serverOptions: ServerOptions = {
    command:   serverPath,
    args:      ['lsp'],
    transport: TransportKind.stdio,
  };

  // ── Client options ────────────────────────────────────────────────────────
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: 'file', language: 'brief' }],
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher('**/*.brief'),
    },
    outputChannelName: 'Brief Language Server',
    traceOutputChannel: vscode.window.createOutputChannel('Brief LSP Trace'),
  };

  // ── Create and start client ───────────────────────────────────────────────
  client = new LanguageClient(
    'brief-lsp',
    'Brief Language Server',
    serverOptions,
    clientOptions,
  );

  if (traceLevel === 'messages') {
    client.setTrace(Trace.Messages);
  } else if (traceLevel === 'verbose') {
    client.setTrace(Trace.Verbose);
  }

  client.start();

  // ── Commands ──────────────────────────────────────────────────────────────
  context.subscriptions.push(
    vscode.commands.registerCommand('brief.restartServer', async () => {
      await client?.stop();
      client?.start();
      vscode.window.showInformationMessage('Brief language server restarted.');
    }),

    vscode.commands.registerCommand('brief.check', async () => {
      const file = vscode.window.activeTextEditor?.document.uri.fsPath;
      if (!file?.endsWith('.brief')) {
        vscode.window.showWarningMessage('Open a .brief file first.');
        return;
      }
      const terminal = vscode.window.createTerminal('brief check');
      terminal.sendText(`${serverPath} check "${file}"`);
      terminal.show();
    }),

    vscode.commands.registerCommand('brief.verify', async () => {
      const file = vscode.window.activeTextEditor?.document.uri.fsPath;
      if (!file?.endsWith('.brief')) {
        vscode.window.showWarningMessage('Open a .brief file first.');
        return;
      }
      const terminal = vscode.window.createTerminal('brief verify');
      terminal.sendText(`${serverPath} verify "${file}"`);
      terminal.show();
    }),

    vscode.commands.registerCommand('brief.serve', async () => {
      const file = vscode.window.activeTextEditor?.document.uri.fsPath;
      if (!file?.endsWith('.brief')) {
        vscode.window.showWarningMessage('Open a .brief file first.');
        return;
      }
      const terminal = vscode.window.createTerminal('brief serve');
      terminal.sendText(`${serverPath} serve "${file}"`);
      terminal.show();
    }),
  );

  // ── Status bar — LSP server ───────────────────────────────────────────────
  const statusItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100,
  );
  statusItem.text    = '$(sparkle) Brief';
  statusItem.tooltip = 'Brief language server is running. Click to restart.';
  statusItem.command = 'brief.restartServer';
  statusItem.show();
  context.subscriptions.push(statusItem);

  // ── Status bar — lock freshness ───────────────────────────────────────────
  lockStatusItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    99,
  );
  lockStatusItem.command = 'brief.verify';
  context.subscriptions.push(lockStatusItem);

  // Watch .brief.lock files for changes.
  lockWatcher = vscode.workspace.createFileSystemWatcher('**/*.brief.lock');
  lockWatcher.onDidCreate(uri => updateLockStatus(uri.fsPath.replace(/\.lock$/, '')));
  lockWatcher.onDidChange(uri => updateLockStatus(uri.fsPath.replace(/\.lock$/, '')));
  lockWatcher.onDidDelete(uri => updateLockStatus(uri.fsPath.replace(/\.lock$/, '')));
  context.subscriptions.push(lockWatcher);

  // Update lock status when active editor changes.
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor(editor => {
      if (editor?.document.languageId === 'brief') {
        updateLockStatus(editor.document.uri.fsPath);
      } else {
        lockStatusItem?.hide();
      }
    }),
  );

  // Update for currently open .brief file.
  const active = vscode.window.activeTextEditor;
  if (active?.document.languageId === 'brief') {
    updateLockStatus(active.document.uri.fsPath);
  }

  // ── Config change reload ──────────────────────────────────────────────────
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (e) => {
      if (e.affectsConfiguration('brief.serverPath')) {
        const choice = await vscode.window.showInformationMessage(
          'Brief: server path changed. Restart the language server?',
          'Restart',
          'Later',
        );
        if (choice === 'Restart') {
          await client?.stop();
          client?.start();
        }
      }
    }),
  );
}

/**
 * Update the lock status bar item based on the .brief.lock file state.
 */
function updateLockStatus(briefPath: string): void {
  if (!lockStatusItem) return;

  const lockPath = briefPath + '.lock';

  if (!fs.existsSync(lockPath)) {
    lockStatusItem.text    = '$(unlock) Brief: unsealed';
    lockStatusItem.tooltip = 'No .brief.lock file. Click to run `brief verify`.';
    lockStatusItem.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
    lockStatusItem.show();
    return;
  }

  try {
    const content = fs.readFileSync(lockPath, 'utf8');
    const verifiedAt = extractVerifiedAt(content);
    const briefHash  = extractBriefHash(content);

    if (!verifiedAt) {
      lockStatusItem.text    = '$(warning) Brief: invalid lock';
      lockStatusItem.tooltip = 'Lock file is malformed. Run `brief verify` to regenerate.';
      lockStatusItem.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
      lockStatusItem.show();
      return;
    }

    const ageMs   = Date.now() - new Date(verifiedAt).getTime();
    const ageHrs  = ageMs / (1000 * 60 * 60);
    const ageStr  = ageHrs < 1
      ? `${Math.round(ageMs / 60000)}m ago`
      : `${Math.round(ageHrs)}h ago`;

    if (ageHrs > 24) {
      lockStatusItem.text    = `$(warning) Brief: lock stale (${ageStr})`;
      lockStatusItem.tooltip = 'Lock is older than 24h. Click to run `brief verify`.';
      lockStatusItem.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
    } else {
      lockStatusItem.text    = `$(lock) Brief: sealed (${ageStr})`;
      lockStatusItem.tooltip = `Contract sealed ${ageStr}. Click to re-verify.`;
      lockStatusItem.backgroundColor = undefined;
    }
    lockStatusItem.show();
  } catch {
    lockStatusItem.text    = '$(error) Brief: lock error';
    lockStatusItem.tooltip = 'Cannot read lock file.';
    lockStatusItem.backgroundColor = new vscode.ThemeColor('statusBarItem.errorBackground');
    lockStatusItem.show();
  }
}

function extractVerifiedAt(toml: string): string | undefined {
  const m = toml.match(/verified_at\s*=\s*"([^"]+)"/);
  return m?.[1];
}

function extractBriefHash(toml: string): string | undefined {
  const m = toml.match(/brief_hash\s*=\s*"([^"]+)"/);
  return m?.[1];
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
  }
  lockWatcher?.dispose();
}
