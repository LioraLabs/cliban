import * as vscode from 'vscode';
import * as crypto from 'node:crypto';
import type { HostMsg, WebviewMsg } from '../shared/protocol';
import type { Status } from '../shared/model';
import { BoardStore, type MilestoneFilter } from './store';
import {
  ClibanClient,
  ClibanError,
  CliMissingError,
  ConflictError,
  NotFoundError,
  ValidationError,
} from './client/client';
import type { Issue, IssueDraft } from '../shared/model';
import type { EditSectionMsg, ErrorKind } from '../shared/protocol';
import { readSettings } from './settings';
import { DbWatcher, resolveDbPath } from './watcher';
import { ClibanDocsProvider, openIssueDocument } from './docs';

const PROJECT_STATE_KEY = 'cliban.project';

export class BoardPanel {
  private static current: BoardPanel | undefined;

  static createOrShow(context: vscode.ExtensionContext, docs: ClibanDocsProvider): BoardPanel {
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
    BoardPanel.current = new BoardPanel(panel, context, docs);
    return BoardPanel.current;
  }

  /** Command entry points act on the open panel, opening it if needed. */
  static get(context: vscode.ExtensionContext, docs: ClibanDocsProvider): BoardPanel {
    return BoardPanel.createOrShow(context, docs);
  }

  static refreshIfOpen(): void {
    void BoardPanel.current?.refresh();
  }

  private client: ClibanClient;
  private readonly store = new BoardStore();

  private constructor(
    private readonly panel: vscode.WebviewPanel,
    private readonly context: vscode.ExtensionContext,
    private readonly docs: ClibanDocsProvider,
  ) {
    this.client = this.buildClient();
    this.panel.webview.html = this.render(context);
    this.panel.webview.onDidReceiveMessage((msg: WebviewMsg) => void this.onMessage(msg));
    this.panel.onDidDispose(() => {
      this.watcher?.dispose();
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
          events: snap.events,
          milestoneFilter: snap.milestoneFilter,
        });
      }
    });
    this.armWatcher();
    this.panel.onDidChangeViewState((e) => {
      if (e.webviewPanel.visible) void this.refresh();
    });
    this.context.subscriptions.push(
      vscode.workspace.onDidChangeConfiguration((e) => {
        if (e.affectsConfiguration('cliban')) {
          this.client = this.buildClient();
          this.armWatcher();
          void this.refresh();
        }
      }),
    );
  }

  private watcher: DbWatcher | undefined;

  private armWatcher(): void {
    this.watcher?.dispose();
    const s = readSettings();
    this.watcher = new DbWatcher({
      dbPath: resolveDbPath(s.dbPath, process.env),
      mode: s.watchMode,
      pollIntervalMs: s.pollIntervalSeconds * 1000,
      onFire: () => {
        this.docs.externalChange();
        void this.refresh();
      },
    });
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
      case 'pickMilestone':
        await this.switchMilestone();
        break;
      case 'openIssue':
        await this.openIssue(msg.key);
        break;
      case 'openIssueDoc':
        await openIssueDocument(msg.key);
        break;
      case 'moveIssue':
        await this.moveIssue(msg.requestId, msg.key, msg.toStatus);
        break;
      case 'tickStep':
        await this.mutateAndReload(msg.key, () => this.client.tick(msg.key, msg.task, msg.step));
        break;
      case 'addLog':
        await this.mutateAndReload(msg.key, async () => {
          await this.client.log(msg.key, msg.message);
          return undefined;
        });
        break;
      case 'editMeta':
        await this.mutateAndReload(msg.key, () =>
          this.client.editMeta(msg.key, msg.patch, msg.ifUpdatedAt),
        );
        break;
      case 'createIssue':
        await this.createIssue(msg.requestId, msg.draft);
        break;
      case 'editSection':
        await this.editSection(msg);
        break;
      case 'archiveDone':
        await this.archiveDone();
        break;
      case 'openSettings':
        void vscode.commands.executeCommand('workbench.action.openSettings', 'cliban');
        break;
    }
  }

  async archiveDone(): Promise<void> {
    const project = this.store.snapshot().project;
    if (!project) return;
    const doneCount = this.store.snapshot().issues.filter((i) => i.status === 'done').length;
    if (doneCount === 0) {
      this.post({ type: 'toast', level: 'info', message: 'nothing in Done to archive' });
      return;
    }
    const pick = await vscode.window.showWarningMessage(
      `Archive all ${doneCount} done issue(s) in ${project}? (reversible: cliban issue unarchive)`,
      { modal: true },
      'Archive',
    );
    if (pick !== 'Archive') return;
    try {
      await this.client.archiveDone(project);
      await this.refresh();
    } catch (err) {
      this.surface(err);
    }
  }

  private async createIssue(requestId: string, draft: IssueDraft): Promise<void> {
    try {
      const echo = await this.client.addIssue(draft);
      this.store.applyEcho(echo);
      this.post({ type: 'toast', level: 'info', message: `created ${echo.key}` });
    } catch (err) {
      this.postMutationFailed(requestId, err);
    }
  }

  private async editSection(msg: EditSectionMsg): Promise<void> {
    try {
      const echo = await this.client.editSection(
        msg.key,
        msg.section,
        msg.content,
        msg.ifUpdatedAt,
      );
      this.store.applyEcho(echo);
      await this.openIssue(msg.key);
    } catch (err) {
      // push the failure first so the webview can stash the draft, then
      // reload the detail so the drawer shows current reality
      this.postMutationFailed(msg.requestId, err);
      if (err instanceof ConflictError) await this.openIssue(msg.key);
    }
  }

  private postMutationFailed(requestId: string, err: unknown): void {
    const message = err instanceof Error ? err.message : String(err);
    let kind: ErrorKind = 'internal';
    if (err instanceof ConflictError) kind = 'conflict';
    else if (err instanceof ValidationError) kind = 'validation';
    else if (err instanceof NotFoundError) kind = 'not-found';
    else if (err instanceof CliMissingError) kind = 'cli-missing';
    this.post({ type: 'mutationFailed', requestId, kind, message });
  }

  /**
   * Run a drawer mutation, fold any echo into the board, and push a fresh
   * issueDetail so the drawer reflects reality (including after a conflict,
   * where the reload IS the recovery).
   */
  private async mutateAndReload(key: string, op: () => Promise<Issue | undefined>): Promise<void> {
    try {
      const echo = await op();
      if (echo) this.store.applyEcho(echo);
    } catch (err) {
      if (err instanceof ConflictError) {
        this.post({
          type: 'toast',
          level: 'error',
          message: `${key} changed since you opened it — reloaded`,
        });
      } else {
        const message = err instanceof Error ? err.message : String(err);
        this.post({ type: 'toast', level: 'error', message });
      }
    }
    await this.openIssue(key);
  }

  private async openIssue(key: string): Promise<void> {
    this.post({ type: 'busy', on: true });
    try {
      const [issue, spec, plan, notes, activity] = await Promise.all([
        this.client.showIssue(key),
        this.client.showSection(key, 'spec'),
        this.client.showSection(key, 'plan'),
        this.client.showSection(key, 'notes'),
        this.client.showSection(key, 'activity'),
      ]);
      this.post({ type: 'issueDetail', issue, sections: { spec, plan, notes, activity } });
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      this.post({ type: 'toast', level: 'error', message });
    } finally {
      this.post({ type: 'busy', on: false });
    }
  }

  private async moveIssue(requestId: string, key: string, toStatus: Status): Promise<void> {
    this.store.applyOptimistic(requestId, key, { status: toStatus });
    try {
      const echo = await this.client.moveIssue(key, toStatus);
      this.store.commit(requestId, echo);
    } catch (err) {
      this.store.rollback(requestId);
      const message = err instanceof Error ? err.message : String(err);
      this.post({ type: 'toast', level: 'error', message });
    }
  }

  async refresh(): Promise<void> {
    const project = await this.resolveProject();
    if (!project) return;
    this.post({ type: 'busy', on: true });
    try {
      const since = this.store.lastActivityTs ?? '1d';
      const [issues, milestones, labels, events] = await Promise.all([
        this.client.listIssues(project),
        this.client.listMilestones(project),
        this.client.listLabels(project),
        this.client.activity(since, project),
      ]);
      this.store.setMilestoneFilter(this.persistedMilestoneFilter(project));
      this.store.setBoard(project, issues, milestones, labels);
      this.store.mergeActivity(events);
    } catch (err) {
      this.surface(err);
    } finally {
      this.post({ type: 'busy', on: false });
    }
  }

  newIssue(): void {
    this.post({ type: 'openCreateForm' });
  }

  async switchMilestone(): Promise<void> {
    const snap = this.store.snapshot();
    if (!snap.project) return;
    type Item = vscode.QuickPickItem & { filter: MilestoneFilter };
    const items: Item[] = [
      { label: 'All milestones', filter: undefined },
      { label: 'No milestone', description: 'issues not assigned to any milestone', filter: null },
      ...snap.milestones.map(
        (m): Item => ({
          label: `◇ ${m.name}`,
          description: m.status && m.status !== 'open' ? m.status : '',
          filter: m.name,
        }),
      ),
    ];
    const picked = await vscode.window.showQuickPick(items, {
      placeHolder: 'Filter the board by milestone',
    });
    if (!picked) return;
    await this.context.workspaceState.update(this.milestoneStateKey(snap.project), picked.filter);
    this.store.setMilestoneFilter(picked.filter);
  }

  private milestoneStateKey(project: string): string {
    return `cliban.milestone:${project}`;
  }

  private persistedMilestoneFilter(project: string): MilestoneFilter {
    return this.context.workspaceState.get<string | null>(this.milestoneStateKey(project));
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
