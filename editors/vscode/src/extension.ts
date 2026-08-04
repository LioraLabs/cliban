import * as vscode from 'vscode';
import { BoardPanel } from './panel';
import { ClibanDocsProvider, openIssueDocument } from './docs';
import { ClibanClient } from './client/client';
import { readSettings } from './settings';

export function activate(context: vscode.ExtensionContext): void {
  let docsClient: ClibanClient | undefined;
  const provider = new ClibanDocsProvider(() => {
    if (!docsClient) {
      const s = readSettings();
      docsClient = new ClibanClient({ exePath: s.executablePath, dbPath: s.dbPath });
    }
    return docsClient;
  });
  provider.setOnSaved(() => BoardPanel.refreshIfOpen());

  context.subscriptions.push(
    vscode.workspace.registerFileSystemProvider(ClibanDocsProvider.scheme, provider, {
      isCaseSensitive: true,
    }),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('cliban')) docsClient = undefined;
    }),
    vscode.commands.registerCommand('cliban.openBoard', () => {
      BoardPanel.createOrShow(context, provider);
    }),
    vscode.commands.registerCommand('cliban.switchProject', () => {
      void BoardPanel.get(context, provider).switchProject();
    }),
    vscode.commands.registerCommand('cliban.refreshBoard', () => {
      void BoardPanel.get(context, provider).refresh();
    }),
    vscode.commands.registerCommand('cliban.newIssue', () => {
      BoardPanel.get(context, provider).newIssue();
    }),
    vscode.commands.registerCommand('cliban.archiveDone', () => {
      void BoardPanel.get(context, provider).archiveDone();
    }),
    vscode.commands.registerCommand('cliban.openIssueDocument', async (key?: string) => {
      const target = key ?? (await promptForKey());
      if (!target) return;
      await openIssueDocument(target);
    }),
  );
}

async function promptForKey(): Promise<string | undefined> {
  const value = await vscode.window.showInputBox({
    placeHolder: 'Issue key, e.g. CLI-42',
    validateInput: (v) => (/^[A-Za-z][A-Za-z0-9]{1,9}-\d+$/.test(v.trim()) ? null : 'PROJ-N'),
  });
  return value?.trim().toUpperCase();
}

export function deactivate(): void {}
