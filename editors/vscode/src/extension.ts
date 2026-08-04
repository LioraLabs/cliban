import * as vscode from 'vscode';
import { BoardPanel } from './panel';

export function activate(context: vscode.ExtensionContext): void {
  context.subscriptions.push(
    vscode.commands.registerCommand('cliban.openBoard', () => {
      BoardPanel.createOrShow(context);
    }),
    vscode.commands.registerCommand('cliban.switchProject', () => {
      void BoardPanel.get(context).switchProject();
    }),
    vscode.commands.registerCommand('cliban.refreshBoard', () => {
      void BoardPanel.get(context).refresh();
    }),
  );
}

export function deactivate(): void {}
