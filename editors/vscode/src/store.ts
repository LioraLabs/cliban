import type { Issue, Label, Milestone } from '../shared/model';

export interface BoardSnapshot {
  project: string | undefined;
  issues: Issue[];
  milestones: Milestone[];
  labels: Label[];
}

/**
 * Host-side source of truth for the open board. Issues stay in the CLI's
 * ls-order — cliban lists by (status, position), so array order IS the
 * within-column card order.
 */
export class BoardStore {
  private project: string | undefined;
  private issues: Issue[] = [];
  private milestones: Milestone[] = [];
  private labels: Label[] = [];
  private listeners: Array<(snap: BoardSnapshot) => void> = [];

  onChange(fn: (snap: BoardSnapshot) => void): void {
    this.listeners.push(fn);
  }

  snapshot(): BoardSnapshot {
    return {
      project: this.project,
      issues: [...this.issues],
      milestones: [...this.milestones],
      labels: [...this.labels],
    };
  }

  setBoard(project: string, issues: Issue[], milestones: Milestone[], labels: Label[]): void {
    this.project = project;
    this.issues = issues;
    this.milestones = milestones;
    this.labels = labels;
    this.emit();
  }

  /** Merge a mutation echo: replace by key, or append if unseen. */
  applyEcho(echo: Issue): void {
    const idx = this.issues.findIndex((i) => i.key === echo.key);
    if (idx >= 0) this.issues[idx] = echo;
    else this.issues.push(echo);
    this.emit();
  }

  private pending = new Map<string, { key: string; before: Issue }>();

  /** Apply a local patch ahead of the CLI round-trip, remembering the pre-state. */
  applyOptimistic(requestId: string, key: string, patch: Partial<Issue>): void {
    const idx = this.issues.findIndex((i) => i.key === key);
    if (idx < 0) return;
    const before = this.issues[idx]!;
    this.pending.set(requestId, { key, before });
    this.issues[idx] = { ...before, ...patch };
    this.emit();
  }

  /** The mutation landed: drop the pending record and take the echo as truth. */
  commit(requestId: string, echo: Issue): void {
    this.pending.delete(requestId);
    this.applyEcho(echo);
  }

  /** The mutation failed: restore the remembered pre-state if still present. */
  rollback(requestId: string): void {
    const entry = this.pending.get(requestId);
    this.pending.delete(requestId);
    if (!entry) return;
    const idx = this.issues.findIndex((i) => i.key === entry.key);
    if (idx < 0) return;
    this.issues[idx] = entry.before;
    this.emit();
  }

  private emit(): void {
    const snap = this.snapshot();
    for (const fn of this.listeners) fn(snap);
  }
}
