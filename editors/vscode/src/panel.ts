import * as vscode from 'vscode';
import * as crypto from 'node:crypto';
import type { HostMsg, WebviewMsg } from '../shared/protocol';

export class BoardPanel {
  private static current: BoardPanel | undefined;

  static createOrShow(context: vscode.ExtensionContext): BoardPanel {
    if (BoardPanel.current) {
      BoardPanel.current.panel.reveal();
      return BoardPanel.current;
    }
    const panel = vscode.window.createWebviewPanel(
      'clibanBoard',
      'Cliban Board',
      vscode.ViewColumn.Active,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: [vscode.Uri.joinPath(context.extensionUri, 'dist')],
      },
    );
    BoardPanel.current = new BoardPanel(panel, context);
    return BoardPanel.current;
  }

  private constructor(
    private readonly panel: vscode.WebviewPanel,
    context: vscode.ExtensionContext,
  ) {
    this.panel.webview.html = this.render(context);
    this.panel.webview.onDidReceiveMessage((msg: WebviewMsg) => this.onMessage(msg));
    this.panel.onDidDispose(() => {
      BoardPanel.current = undefined;
    });
  }

  private post(msg: HostMsg): void {
    void this.panel.webview.postMessage(msg);
  }

  private onMessage(msg: WebviewMsg): void {
    switch (msg.type) {
      case 'ready':
        break;
      case 'ping':
        this.post({ type: 'pong', nonce: msg.nonce });
        break;
    }
  }

  private render(context: vscode.ExtensionContext): string {
    const webview = this.panel.webview;
    const nonce = crypto.randomBytes(16).toString('base64');
    const scriptUri = webview.asWebviewUri(
      vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview.js'),
    );
    const styleUri = webview.asWebviewUri(
      vscode.Uri.joinPath(context.extensionUri, 'dist', 'webview.css'),
    );
    const csp = [
      `default-src 'none'`,
      `script-src 'nonce-${nonce}'`,
      `style-src ${webview.cspSource}`,
      `font-src ${webview.cspSource}`,
    ].join('; ');
    return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="${csp}">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<link rel="stylesheet" href="${styleUri}">
<title>Cliban Board</title>
</head>
<body>
<div id="app"></div>
<script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
  }
}
