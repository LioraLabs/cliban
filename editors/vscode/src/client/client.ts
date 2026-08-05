import { spawn } from 'node:child_process';
import { parseNdjson } from './ndjson';
import type {
  ActivityEvent,
  Issue,
  IssueDraft,
  Label,
  MetaPatch,
  Milestone,
  Project,
  Status,
} from '../../shared/model';

export type Section = 'spec' | 'plan' | 'notes' | 'activity';

export class ClibanError extends Error {
  constructor(
    message: string,
    readonly exitCode: number | null,
  ) {
    super(message);
  }
}
export class NotFoundError extends ClibanError {}
export class ValidationError extends ClibanError {}
export class ConflictError extends ClibanError {}
export class InternalError extends ClibanError {}
export class CliMissingError extends ClibanError {
  constructor(readonly exePath: string) {
    super(`cliban binary not found: ${exePath}`, null);
  }
}

export interface ClibanClientOptions {
  exePath: string;
  /** Prepended to every invocation's args (test hook: run the fake via node). */
  extraArgs?: string[];
  /** Passed as --db on every call when set. */
  dbPath?: string;
  env?: NodeJS.ProcessEnv;
}

interface RunResult {
  stdout: string;
  stderr: string;
  code: number;
}

export class ClibanClient {
  constructor(private readonly opts: ClibanClientOptions) {}

  // ---- reads ----

  async listProjects(): Promise<Project[]> {
    return (await this.runJsonLines(['project', 'ls'])) as Project[];
  }

  async listIssues(project: string): Promise<Issue[]> {
    return (await this.runJsonLines(['issue', 'ls', '--project', project])) as Issue[];
  }

  async listMilestones(project: string): Promise<Milestone[]> {
    return (await this.runJsonLines(['milestone', 'ls', '--project', project])) as Milestone[];
  }

  async listLabels(project: string): Promise<Label[]> {
    return (await this.runJsonLines(['label', 'ls', '--project', project])) as Label[];
  }

  async activity(since: string, project: string): Promise<ActivityEvent[]> {
    return (await this.runJsonLines([
      'activity', '--since', since, '--project', project,
    ])) as ActivityEvent[];
  }

  async showIssue(key: string): Promise<Issue> {
    return (await this.runJson(['issue', 'show', key])) as Issue;
  }

  /**
   * Raw markdown of one section via `issue cat`; null when the section
   * doesn't exist (exit 1). `cat` is raw-only and rejects --json.
   */
  async showSection(key: string, section: Section): Promise<string | null> {
    const res = await this.run(['issue', 'cat', key, '--section', section], {
      allowExit1: true,
      raw: true,
    });
    if (res.code === 1) return null;
    this.raiseOnFailure(res);
    return res.stdout;
  }

  // ---- mutations (all echo the mutated issue) ----

  async moveIssue(key: string, status: Status, note?: string): Promise<Issue> {
    const args = ['issue', 'mv', key, status];
    if (note !== undefined) args.push('--note', note);
    return (await this.runJson(args)) as Issue;
  }

  async addIssue(draft: IssueDraft): Promise<Issue> {
    const args = ['issue', 'add', draft.title, '--project', draft.project];
    if (draft.status) args.push('--status', draft.status);
    if (draft.priority) args.push('--priority', draft.priority);
    for (const label of draft.labels ?? []) args.push('--label', label);
    if (draft.milestone) args.push('--milestone', draft.milestone);
    let stdin: string | undefined;
    if (draft.description !== undefined) {
      args.push('--description-file', '-');
      stdin = draft.description;
    }
    return (await this.runJson(args, stdin)) as Issue;
  }

  async editMeta(key: string, patch: MetaPatch, ifUpdatedAt: string): Promise<Issue> {
    const args = ['issue', 'edit', key];
    if (patch.title !== undefined) args.push('--title', patch.title);
    if (patch.priority !== undefined) args.push('--priority', patch.priority);
    for (const label of patch.addLabels ?? []) args.push('--label', label);
    for (const label of patch.removeLabels ?? []) args.push('--remove-label', label);
    if (patch.milestone !== undefined) args.push('--milestone', patch.milestone);
    if (patch.clearMilestone) args.push('--clear-milestone');
    args.push('--if-updated-at', ifUpdatedAt);
    return (await this.runJson(args)) as Issue;
  }

  async editSection(
    key: string,
    section: Exclude<Section, 'activity'>,
    content: string,
    ifUpdatedAt: string,
  ): Promise<Issue> {
    const args = [
      'issue', 'edit', key,
      '--section', section,
      '--description-file', '-',
      '--if-updated-at', ifUpdatedAt,
    ];
    return (await this.runJson(args, content)) as Issue;
  }

  /**
   * Replace the ENTIRE description — the document-editor path (TUI `e`
   * equivalent). Always CAS-guarded: this is the one call that can destroy
   * sections, so a stale token must fail it.
   */
  async editDescription(key: string, content: string, ifUpdatedAt: string): Promise<Issue> {
    const args = ['issue', 'edit', key, '--description-file', '-', '--if-updated-at', ifUpdatedAt];
    return (await this.runJson(args, content)) as Issue;
  }

  async tick(key: string, task: number, step: number): Promise<Issue> {
    return (await this.runJson([
      'issue', 'tick', key, '--task', String(task), '--step', String(step),
    ])) as Issue;
  }

  async log(key: string, message: string): Promise<void> {
    await this.runJson(['issue', 'log', key, message]);
  }

  async archiveDone(project: string): Promise<void> {
    await this.runJson(['issue', 'archive-done', '--project', project]);
  }

  // ---- plumbing ----

  private async runJson(args: string[], stdin?: string): Promise<unknown> {
    const res = await this.run(args, { stdin });
    return res.stdout.trim() === '' ? {} : JSON.parse(res.stdout);
  }

  private async runJsonLines(args: string[]): Promise<unknown[]> {
    const res = await this.run(args);
    return parseNdjson(res.stdout);
  }

  private async run(
    args: string[],
    opts: { stdin?: string; allowExit1?: boolean; raw?: boolean } = {},
  ): Promise<RunResult> {
    const fullArgs = [...(this.opts.extraArgs ?? []), ...args];
    if (this.opts.dbPath) fullArgs.push('--db', this.opts.dbPath);
    if (!opts.raw) fullArgs.push('--json');

    const res = await new Promise<RunResult>((resolve, reject) => {
      const child = spawn(this.opts.exePath, fullArgs, {
        env: this.opts.env ?? process.env,
        stdio: ['pipe', 'pipe', 'pipe'],
      });
      let stdout = '';
      let stderr = '';
      child.stdout.setEncoding('utf8').on('data', (d: string) => (stdout += d));
      child.stderr.setEncoding('utf8').on('data', (d: string) => (stderr += d));
      child.on('error', (err: NodeJS.ErrnoException) => {
        if (err.code === 'ENOENT') reject(new CliMissingError(this.opts.exePath));
        else reject(new ClibanError(err.message, null));
      });
      child.on('close', (code) => resolve({ stdout, stderr, code: code ?? -1 }));
      if (opts.stdin !== undefined) child.stdin.end(opts.stdin);
      else child.stdin.end();
    });

    if (opts.allowExit1 && res.code === 1) return res;
    this.raiseOnFailure(res);
    return res;
  }

  private raiseOnFailure(res: RunResult): void {
    if (res.code === 0) return;
    const msg = res.stderr.trim() || `cliban exited ${res.code}`;
    switch (res.code) {
      case 1:
        throw new NotFoundError(msg, 1);
      case 2:
        if (msg.includes('stale write:')) throw new ConflictError(msg, 2);
        throw new ValidationError(msg, 2);
      default:
        throw new InternalError(msg, res.code);
    }
  }
}
