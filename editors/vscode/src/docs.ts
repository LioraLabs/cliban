import * as vscode from 'vscode';
import { ClibanClient, ConflictError, NotFoundError } from './client/client';

/**
 * `cliban:` filesystem — every issue is a document (`cliban:/CLI-60.md`)
 * whose content is the raw stored description. Read pins the issue's usec
 * `updated_at` as a CAS token; save writes the whole description back with
 * it, so a concurrent board edit fails the save instead of being overwritten.
 * The VS Code equivalent of the TUI's `e`.
 */
export async function openIssueDocument(key: string): Promise<void> {
  const doc = await vscode.workspace.openTextDocument(ClibanDocsProvider.uriFor(key));
  await vscode.window.showTextDocument(doc, { preview: false });
}

export class ClibanDocsProvider implements vscode.FileSystemProvider {
  static readonly scheme = 'cliban';

  static uriFor(key: string): vscode.Uri {
    return vscode.Uri.parse(`${ClibanDocsProvider.scheme}:/${key}.md`);
  }

  private readonly emitter = new vscode.EventEmitter<vscode.FileChangeEvent[]>();
  readonly onDidChangeFile = this.emitter.event;

  private readonly cas = new Map<string, string>();
  private readonly sizes = new Map<string, number>();
  private readonly mtimes = new Map<string, number>();
  private clock = 1;
  private onSaved: ((key: string) => void) | undefined;

  constructor(private readonly getClient: () => ClibanClient) {}

  setOnSaved(fn: (key: string) => void): void {
    this.onSaved = fn;
  }

  /** The board changed under us — make VS Code re-read clean open editors. */
  externalChange(): void {
    if (this.cas.size === 0) return;
    this.clock++;
    const events: vscode.FileChangeEvent[] = [];
    for (const key of this.cas.keys()) {
      this.mtimes.set(key, this.clock);
      events.push({ type: vscode.FileChangeType.Changed, uri: ClibanDocsProvider.uriFor(key) });
    }
    this.emitter.fire(events);
  }

  private keyOf(uri: vscode.Uri): string {
    return uri.path.replace(/^\//, '').replace(/\.md$/i, '');
  }

  watch(): vscode.Disposable {
    return new vscode.Disposable(() => {});
  }

  stat(uri: vscode.Uri): vscode.FileStat {
    const key = this.keyOf(uri);
    return {
      type: vscode.FileType.File,
      ctime: 0,
      mtime: this.mtimes.get(key) ?? 0,
      size: this.sizes.get(key) ?? 0,
    };
  }

  async readFile(uri: vscode.Uri): Promise<Uint8Array> {
    const key = this.keyOf(uri);
    try {
      const issue = await this.getClient().showIssue(key);
      this.cas.set(key, issue.updated_at);
      const bytes = new TextEncoder().encode(issue.description ?? '');
      this.sizes.set(key, bytes.length);
      if (!this.mtimes.has(key)) this.mtimes.set(key, this.clock);
      return bytes;
    } catch (err) {
      if (err instanceof NotFoundError) throw vscode.FileSystemError.FileNotFound(uri);
      throw vscode.FileSystemError.Unavailable(err instanceof Error ? err.message : String(err));
    }
  }

  async writeFile(uri: vscode.Uri, content: Uint8Array): Promise<void> {
    const key = this.keyOf(uri);
    const token = this.cas.get(key);
    if (!token) throw vscode.FileSystemError.Unavailable(`open ${key} before saving it`);
    try {
      const echo = await this.getClient().editDescription(
        key,
        new TextDecoder().decode(content),
        token,
      );
      this.cas.set(key, echo.updated_at);
      this.sizes.set(key, content.length);
      this.mtimes.set(key, ++this.clock);
      this.onSaved?.(key);
    } catch (err) {
      if (err instanceof ConflictError) {
        throw vscode.FileSystemError.Unavailable(
          `${key} changed on the board since you opened it — Revert File to load the latest, then re-apply your edit`,
        );
      }
      if (err instanceof NotFoundError) throw vscode.FileSystemError.FileNotFound(uri);
      throw vscode.FileSystemError.Unavailable(err instanceof Error ? err.message : String(err));
    }
  }

  readDirectory(): [string, vscode.FileType][] {
    return [];
  }
  createDirectory(): void {
    throw vscode.FileSystemError.NoPermissions('cliban documents have no directories');
  }
  delete(): void {
    throw vscode.FileSystemError.NoPermissions('archive issues from the board or CLI instead');
  }
  rename(): void {
    throw vscode.FileSystemError.NoPermissions('issue keys are assigned by cliban');
  }
}
