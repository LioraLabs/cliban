// The postMessage contract between the extension host and the board webview.
// Every optimistic mutation carries a requestId so the webview can correlate
// mutationOk/mutationFailed back to the pending local change.

import type {
  ActivityEvent,
  Issue,
  IssueDraft,
  Label,
  MetaPatch,
  Milestone,
  Status,
} from './model';

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

export interface MutationFailedMsg {
  type: 'mutationFailed';
  requestId: string;
  kind: ErrorKind;
  message: string;
}

export interface OpenCreateFormMsg {
  type: 'openCreateForm';
}

export type HostMsg =
  | BoardMsg
  | ErrorStateMsg
  | BusyMsg
  | ToastMsg
  | IssueDetailMsg
  | MutationFailedMsg
  | OpenCreateFormMsg;

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

export interface TickStepMsg {
  type: 'tickStep';
  key: string;
  task: number;
  step: number;
}

export interface AddLogMsg {
  type: 'addLog';
  key: string;
  message: string;
}

export interface EditMetaMsg {
  type: 'editMeta';
  key: string;
  ifUpdatedAt: string;
  patch: MetaPatch;
}

export interface CreateIssueMsg {
  type: 'createIssue';
  requestId: string;
  draft: IssueDraft;
}

export interface EditSectionMsg {
  type: 'editSection';
  requestId: string;
  key: string;
  section: 'spec' | 'plan' | 'notes';
  content: string;
  ifUpdatedAt: string;
}

export type WebviewMsg =
  | ReadyMsg
  | RefreshMsg
  | PickProjectMsg
  | OpenIssueMsg
  | MoveIssueMsg
  | TickStepMsg
  | AddLogMsg
  | EditMetaMsg
  | CreateIssueMsg
  | EditSectionMsg;

export type { ActivityEvent, Issue, Label, Milestone, Status };
