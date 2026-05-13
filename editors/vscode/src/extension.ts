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
 * Configuration:
 *   "brief.serverPath" — path to the `brief` binary (default: "brief" on $PATH)
 *   "brief.trace.server" — LSP trace level ("off" | "messages" | "verbose")
 */

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

export function activate(context: vscode.ExtensionContext): void {
  const config     = vscode.workspace.getConfiguration('brief');
  const serverPath = config.get<string>('serverPath') ?? 'brief';
  const traceLevel = config.get<string>('trace.server') ?? 'off';

  // ── Server options ────────────────────────────────────────────────────────
  // The Brief LSP server is a standalone process that communicates over stdio.
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

  // Apply trace level from configuration.
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
  );

  // ── Status bar ────────────────────────────────────────────────────────────
  const statusItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    100,
  );
  statusItem.text    = '$(sparkle) Brief';
  statusItem.tooltip = 'Brief language server is running. Click to restart.';
  statusItem.command = 'brief.restartServer';
  statusItem.show();
  context.subscriptions.push(statusItem);

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

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
  }
}
