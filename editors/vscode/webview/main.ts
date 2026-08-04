import './styles.css';
import type { HostMsg, WebviewMsg } from '../shared/protocol';

declare function acquireVsCodeApi(): { postMessage(msg: WebviewMsg): void };

const vscode = acquireVsCodeApi();
const app = document.getElementById('app')!;

function onMessage(msg: HostMsg): void {
  switch (msg.type) {
    case 'pong':
      app.textContent = `Cliban board scaffold — host round-trip ok (nonce ${msg.nonce})`;
      break;
  }
}

window.addEventListener('message', (ev: MessageEvent<HostMsg>) => onMessage(ev.data));

vscode.postMessage({ type: 'ready' });
vscode.postMessage({ type: 'ping', nonce: Date.now() });
