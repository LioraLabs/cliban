import * as vscode from 'vscode';
import * as crypto from 'node:crypto';
import type { HostMsg, WebviewMsg } from '../shared/protocol';
import { BoardStore } from './store';
import { ClibanClient, ClibanError, CliMissingError } from './client/client';
import { readSettings } from './settings';

const PROJECT_STATE_KEY = 'cliban.project';

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

  /** Command entry points act on the open panel, opening it if needed. */
  static get(context: vscode.ExtensionContext): BoardPanel {
    return BoardPanel.createOrShow(context);
  }

  private client: ClibanClient;
  private readonly store = new BoardStore();

  private constructor(
    private readonly panel: vscode.WebviewPanel,
    private readonly context: vscode.ExtensionContext,
  ) {
    this.client = this.buildClient();
    this.panel.webview.html = this.render(context);
    this.panel.webview.onDidReceiveMessage((msg: WebviewMsg) => void this.onMessage(msg));
    this.panel.onDidDispose(() => {
      BoardPanel.current = undefined;
    });
    this.store.onChange((snap) => {
      if (snap.project) {
        this.post({
          type: 'board',
          project: snap.project,
          issues: snap.issues,
          milestones: snap.milestones,
          labels: snap.labels,
        });
      }
    });
    this.context.subscriptions.push(
      vscode.workspace.onDidChangeConfiguration((e) => {
        if (e.affectsConfiguration('cliban')) {
          this.client = this.buildClient();
          void this.refresh();
        }
      }),
    );
  }

  private buildClient(): ClibanClient {
    const s = readSettings();
    return new ClibanClient({ exePath: s.executablePath, dbPath: s.dbPath });
  }

  private post(msg: HostMsg): void {
    void this.panel.webview.postMessage(msg);
  }

  private async onMessage(msg: WebviewMsg): Promise<void> {
    switch (msg.type) {
      case 'ready':
        await this.refresh();
        break;
      case 'refresh':
        await this.refresh();
        break;
      case 'pickProject':
        await this.switchProject();
        break;
      case 'openIssue':
        // detail drawer lands in a later task
        break;
    }
  }

  async refresh(): Promise<void> {
    const project = await this.resolveProject();
    if (!project) return;
    this.post({ type: 'busy', on: true });
    try {
      const [issues, milestones, labels] = await Promise.all([
        this.client.listIssues(project),
        this.client.listMilestones(project),
        this.client.listLabels(project),
      ]);
      this.store.setBoard(project, issues, milestones, labels);
    } catch (err) {
      this.surface(err);
    } finally {
      this.post({ type: 'busy', on: false });
    }
  }

  async switchProject(): Promise<void> {
    try {
      const projects = await this.client.listProjects();
      const picked = await vscode.window.showQuickPick(
        projects.map((p) => ({ label: p.key, description: p.name ?? '' })),
        { placeHolder: 'Cliban project' },
      );
      if (!picked) return;
      await this.context.workspaceState.update(PROJECT_STATE_KEY, picked.label);
      await this.refresh();
    } catch (err) {
      this.surface(err);
    }
  }

  private async resolveProject(): Promise<string | undefined> {
    const remembered = this.context.workspaceState.get<string>(PROJECT_STATE_KEY);
    if (remembered) return remembered;
    const fallback = readSettings().defaultProject;
    if (fallback) {
      await this.context.workspaceState.update(PROJECT_STATE_KEY, fallback);
      return fallback;
    }
    await this.switchProject();
    return this.context.workspaceState.get<string>(PROJECT_STATE_KEY);
  }

  private surface(err: unknown): void {
    if (err instanceof CliMissingError) {
      this.post({ type: 'errorState', kind: 'cli-missing', message: err.message });
      return;
    }
    const message = err instanceof Error ? err.message : String(err);
    const kind = err instanceof ClibanError ? 'internal' : 'internal';
    this.post({ type: 'errorState', kind, message });
    void vscode.window.showErrorMessage(`cliban: ${message}`);
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
