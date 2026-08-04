import * as vscode from 'vscode';
import { BoardPanel } from './panel';

export function activate(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.commands.registerCommand('cliban.openBoard', () => {
      BoardPanel.createOrShow(context);
    }),
  );
}

export function deactivate(): void {}
