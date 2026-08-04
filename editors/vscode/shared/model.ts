// Domain types mirroring cliban's JSON output. The CLI's list ("brief") shape
// omits empty and default-valued fields, so everything an `ls` row may drop is
// optional here; only `show`/mutation echoes are guaranteed fuller shapes.

export const STATUSES = ['backlog', 'in-progress', 'blocked', 'in-review', 'done'] as const;
export type Status = (typeof STATUSES)[number];

export const PRIORITIES = ['none', 'low', 'medium', 'high', 'urgent'] as const;
export type Priority = (typeof PRIORITIES)[number];

export interface Relation {
  type: 'blocks' | 'blocked_by' | 'related_to';
  target: string;
}

export interface Issue {
  key: string;
  title: string;
  status: Status;
  updated_at: string;
  priority?: Priority;
  description?: string;
  position?: number;
  archived?: boolean;
  milestone?: string | null;
  parent?: string | null;
  due_date?: string | null;
  labels?: string[];
  relations?: Relation[];
  git_branch_name?: string | null;
  created_at?: string;
  completed_at?: string;
  claimed_by?: string;
}

export interface Project {
  key: string;
  name?: string;
  archived?: boolean;
}

export interface Milestone {
  name: string;
  status?: 'open' | 'completed' | 'cancelled';
  target?: string | null;
  done_count?: number;
}

export interface Label {
  name: string;
}

export interface ActivityEvent {
  ts: string;
  key: string;
  kind: string;
  project?: string;
  issue_status?: Status;
  title?: string;
  message?: string | null;
  actor?: string;
  milestone?: string | null;
}

export interface IssueDraft {
  project: string;
  title: string;
  status?: Status;
  priority?: Priority;
  labels?: string[];
  milestone?: string;
  description?: string;
}

export interface MetaPatch {
  title?: string;
  priority?: Priority;
  addLabels?: string[];
  removeLabels?: string[];
  milestone?: string;
  clearMilestone?: boolean;
}
