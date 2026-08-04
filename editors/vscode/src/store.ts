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

  private emit(): void {
    const snap = this.snapshot();
    for (const fn of this.listeners) fn(snap);
  }
}
