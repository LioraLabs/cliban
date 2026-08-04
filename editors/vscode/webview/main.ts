import './styles.css';
import type { BoardMsg, HostMsg, WebviewMsg } from '../shared/protocol';
import type { MetaPatch, Status } from '../shared/model';
import { renderBoard, renderErrorState } from './board';
import { closeDrawer, renderDrawer } from './drawer';
import { toast } from './toast';

declare function acquireVsCodeApi(): { postMessage(msg: WebviewMsg): void };

const vscode = acquireVsCodeApi();
const app = document.getElementById('app')!;

let lastBoard: BoardMsg | undefined;

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
};

function onMessage(msg: HostMsg): void {
  switch (msg.type) {
    case 'board':
      lastBoard = msg;
      renderBoard(app, msg, handlers);
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
    case 'issueDetail':
      renderDrawer(
        document.body,
        msg,
        { milestones: lastBoard?.milestones ?? [], labels: lastBoard?.labels ?? [] },
        {
          onClose: () => closeDrawer(document.body),
          onOpenIssue: handlers.onOpenIssue,
          onTickStep: handlers.onTickStep,
          onAddLog: handlers.onAddLog,
          onEditMeta: handlers.onEditMeta,
        },
      );
      break;
  }
}

window.addEventListener('message', (ev: MessageEvent<HostMsg>) => onMessage(ev.data));

vscode.postMessage({ type: 'ready' });
