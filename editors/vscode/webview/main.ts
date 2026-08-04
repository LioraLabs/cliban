import './styles.css';
import type { BoardMsg, HostMsg, IssueDetailMsg, WebviewMsg } from '../shared/protocol';
import type { IssueDraft, MetaPatch, Status } from '../shared/model';
import { renderBoard, renderErrorState } from './board';
import { closeDrawer, renderDrawer, type EditableSection } from './drawer';
import { closeCreateForm, createFormFailed, createFormSucceeded, openCreateForm } from './forms';
import { renderFeed } from './feed';
import { toast } from './toast';

declare function acquireVsCodeApi(): { postMessage(msg: WebviewMsg): void };

const vscode = acquireVsCodeApi();
const app = document.getElementById('app')!;

let lastBoard: BoardMsg | undefined;
let lastDetail: IssueDetailMsg | undefined;
let createPending = false;
// section edits in flight, so a conflict can restore the unsent text
const pendingSectionEdits = new Map<
  string,
  { key: string; section: EditableSection; content: string }
>();
// drafts to restore into the next drawer render after a failed edit
let restoreDrafts: { key: string; drafts: Partial<Record<EditableSection, string>> } | undefined;

const handlers = {
  onOpenIssue: (key: string) => vscode.postMessage({ type: 'openIssue', key }),
  onPickProject: () => vscode.postMessage({ type: 'pickProject' }),
  onRefresh: () => vscode.postMessage({ type: 'refresh' }),
  onMoveIssue: (key: string, toStatus: Status) =>
    vscode.postMessage({ type: 'moveIssue', requestId: crypto.randomUUID(), key, toStatus }),
  onTickStep: (key: string, task: number, step: number) =>
    vscode.postMessage({ type: 'tickStep', key, task, step }),
  onAddLog: (key: string, message: string) => vscode.postMessage({ type: 'addLog', key, message }),
  onEditMeta: (key: string, ifUpdatedAt: string, patch: MetaPatch) =>
    vscode.postMessage({ type: 'editMeta', key, ifUpdatedAt, patch }),
  onEditSection: (key: string, section: EditableSection, content: string, ifUpdatedAt: string) => {
    const requestId = crypto.randomUUID();
    pendingSectionEdits.set(requestId, { key, section, content });
    vscode.postMessage({ type: 'editSection', requestId, key, section, content, ifUpdatedAt });
  },
  onNewIssue: () => {
    if (!lastBoard) return;
    createPending = false;
    openCreateForm(document.body, lastBoard.project, lastBoard.milestones, lastBoard.labels, {
      onSubmit: (requestId: string, draft: IssueDraft) => {
        createPending = true;
        vscode.postMessage({ type: 'createIssue', requestId, draft });
      },
    });
  },
};

function onMessage(msg: HostMsg): void {
  switch (msg.type) {
    case 'board':
      lastBoard = msg;
      renderBoard(app, msg, handlers);
      renderFeed(app, msg.events, { onOpenIssue: handlers.onOpenIssue });
      if (createPending) {
        createPending = false;
        createFormSucceeded();
      }
      break;
    case 'errorState':
      renderErrorState(app, msg.kind, msg.message);
      break;
    case 'busy':
      document.body.classList.toggle('busy', msg.on);
      break;
    case 'toast':
      toast(msg.level, msg.message);
      break;
    case 'openCreateForm':
      handlers.onNewIssue();
      break;
    case 'issueDetail': {
      lastDetail = msg;
      // anything still pending for this key either succeeded (this reload is
      // its confirmation) or was already consumed into restoreDrafts
      for (const [id, p] of pendingSectionEdits) {
        if (p.key === msg.issue.key) pendingSectionEdits.delete(id);
      }
      const drafts =
        restoreDrafts && restoreDrafts.key === msg.issue.key ? restoreDrafts.drafts : undefined;
      restoreDrafts = undefined;
      renderDrawer(
        document.body,
        msg,
        { milestones: lastBoard?.milestones ?? [], labels: lastBoard?.labels ?? [], drafts },
        {
          onClose: () => closeDrawer(document.body),
          onOpenIssue: handlers.onOpenIssue,
          onTickStep: handlers.onTickStep,
          onAddLog: handlers.onAddLog,
          onEditMeta: handlers.onEditMeta,
          onEditSection: handlers.onEditSection,
        },
      );
      break;
    }
    case 'mutationFailed': {
      const pending = pendingSectionEdits.get(msg.requestId);
      if (pending) {
        pendingSectionEdits.delete(msg.requestId);
        restoreDrafts = { key: pending.key, drafts: { [pending.section]: pending.content } };
        toast('error', msg.message);
        break;
      }
      if (!createFormFailed(msg.requestId, msg.message)) toast('error', msg.message);
      break;
    }
  }
}

window.addEventListener('message', (ev: MessageEvent<HostMsg>) => onMessage(ev.data));

vscode.postMessage({ type: 'ready' });
