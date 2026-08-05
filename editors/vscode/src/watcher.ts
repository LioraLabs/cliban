import * as fs from 'node:fs';
import * as path from 'node:path';

/** Resolve the DB path exactly as the cliban CLI does. */
export function resolveDbPath(
  setting: string | undefined,
  env: Record<string, string | undefined>,
): string {
  if (setting) return setting;
  if (env['CLIBAN_DB']) return env['CLIBAN_DB'];
  const dataHome = env['XDG_DATA_HOME'] || path.join(env['HOME'] ?? '', '.local', 'share');
  return path.join(dataHome, 'cliban', 'cliban.db');
}

export interface DbWatcherOptions {
  dbPath: string;
  mode: 'auto' | 'poll' | 'off';
  pollIntervalMs: number;
  debounceMs?: number;
  onFire: () => void;
}

/**
 * Fires when the board's SQLite files change. Watches the containing
 * directory (SQLite recreates the -wal file, which breaks per-file watches),
 * filters to the db and its -wal sibling, debounces bursts. `poll` mode (or a
 * failed watch under `auto`) falls back to a plain interval.
 */
export class DbWatcher {
  private watcher: fs.FSWatcher | undefined;
  private pollTimer: ReturnType<typeof setInterval> | undefined;
  private debounceTimer: ReturnType<typeof setTimeout> | undefined;
  private disposed = false;

  constructor(private readonly opts: DbWatcherOptions) {
    if (opts.mode === 'off') return;
    if (opts.mode === 'poll') {
      this.startPolling();
      return;
    }
    try {
      const dir = path.dirname(opts.dbPath);
      const base = path.basename(opts.dbPath);
      const targets = new Set([base, `${base}-wal`]);
      this.watcher = fs.watch(dir, (_event, filename) => {
        if (filename && !targets.has(filename)) return;
        this.fireDebounced();
      });
      this.watcher.on('error', () => {
        this.watcher?.close();
        this.watcher = undefined;
        if (!this.disposed) this.startPolling();
      });
    } catch {
      this.startPolling();
    }
  }

  private startPolling(): void {
    this.pollTimer = setInterval(() => this.opts.onFire(), this.opts.pollIntervalMs);
  }

  private fireDebounced(): void {
    if (this.debounceTimer) clearTimeout(this.debounceTimer);
    this.debounceTimer = setTimeout(() => {
      this.debounceTimer = undefined;
      if (!this.disposed) this.opts.onFire();
    }, this.opts.debounceMs ?? 300);
  }

  dispose(): void {
    this.disposed = true;
    this.watcher?.close();
    if (this.pollTimer) clearInterval(this.pollTimer);
    if (this.debounceTimer) clearTimeout(this.debounceTimer);
  }
}
