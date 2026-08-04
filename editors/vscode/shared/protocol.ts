// The postMessage contract between the extension host and the board webview.
// Every optimistic mutation carries a requestId so the webview can correlate
// mutationOk/mutationFailed back to the pending local change.

import type { ActivityEvent, Issue, Label, Milestone, Status } from './model';

export type ErrorKind = 'not-found' | 'validation' | 'conflict' | 'internal' | 'cli-missing';

// ---- host → webview ----

export interface BoardMsg {
  type: 'board';
  project: string;
  issues: Issue[];
  milestones: Milestone[];
  labels: Label[];
}

export interface ErrorStateMsg {
  type: 'errorState';
  kind: ErrorKind;
  message: string;
}

export interface BusyMsg {
  type: 'busy';
  on: boolean;
}

export type HostMsg = BoardMsg | ErrorStateMsg | BusyMsg;

// ---- webview → host ----

export interface ReadyMsg {
  type: 'ready';
}

export interface RefreshMsg {
  type: 'refresh';
}

export interface PickProjectMsg {
  type: 'pickProject';
}

export interface OpenIssueMsg {
  type: 'openIssue';
  key: string;
}

export type WebviewMsg = ReadyMsg | RefreshMsg | PickProjectMsg | OpenIssueMsg;

export type { ActivityEvent, Issue, Label, Milestone, Status };
