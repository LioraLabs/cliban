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

export interface ToastMsg {
  type: 'toast';
  level: 'info' | 'error';
  message: string;
}

export interface IssueSections {
  spec: string | null;
  plan: string | null;
  notes: string | null;
  activity: string | null;
}

export interface IssueDetailMsg {
  type: 'issueDetail';
  issue: Issue;
  sections: IssueSections;
}

export type HostMsg = BoardMsg | ErrorStateMsg | BusyMsg | ToastMsg | IssueDetailMsg;

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

export interface MoveIssueMsg {
  type: 'moveIssue';
  requestId: string;
  key: string;
  toStatus: Status;
}

export type WebviewMsg = ReadyMsg | RefreshMsg | PickProjectMsg | OpenIssueMsg | MoveIssueMsg;

export type { ActivityEvent, Issue, Label, Milestone, Status };
