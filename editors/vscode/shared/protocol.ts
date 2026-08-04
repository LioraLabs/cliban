// The postMessage contract between the extension host and the board webview.
// Every optimistic mutation carries a requestId so the webview can correlate
// mutationOk/mutationFailed back to the pending local change.

export type Status = 'backlog' | 'in-progress' | 'blocked' | 'in-review' | 'done';

export interface PingMsg {
  type: 'ping';
  nonce: number;
}

export interface PongMsg {
  type: 'pong';
  nonce: number;
}

export interface ReadyMsg {
  type: 'ready';
}

// Placeholder unions — grown task by task alongside the features they carry.
export type HostMsg = PongMsg;
export type WebviewMsg = ReadyMsg | PingMsg;
