import './styles.css';
import type { HostMsg, WebviewMsg } from '../shared/protocol';
import { renderBoard, renderErrorState } from './board';

declare function acquireVsCodeApi(): { postMessage(msg: WebviewMsg): void };

const vscode = acquireVsCodeApi();
const app = document.getElementById('app')!;

const handlers = {
  onOpenIssue: (key: string) => vscode.postMessage({ type: 'openIssue', key }),
  onPickProject: () => vscode.postMessage({ type: 'pickProject' }),
  onRefresh: () => vscode.postMessage({ type: 'refresh' }),
};

function onMessage(msg: HostMsg): void {
  switch (msg.type) {
    case 'board':
      renderBoard(app, msg, handlers);
      break;
    case 'errorState':
      renderErrorState(app, msg.kind, msg.message);
      break;
    case 'busy':
      document.body.classList.toggle('busy', msg.on);
      break;
  }
}

window.addEventListener('message', (ev: MessageEvent<HostMsg>) => onMessage(ev.data));

vscode.postMessage({ type: 'ready' });
