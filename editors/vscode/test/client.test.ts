import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  ClibanClient,
  CliMissingError,
  ConflictError,
  InternalError,
  NotFoundError,
  ValidationError,
} from '../src/client/client';

const FAKE = join(__dirname, '..', 'test', 'fixtures', 'fake-cliban.js');

interface Captured {
  argv: string[];
  stdin: string;
}

function makeClient(env: Record<string, string>, dbPath?: string) {
  return new ClibanClient({
    exePath: process.execPath, // node itself; fake script goes first in extraArgs
    extraArgs: [FAKE],
    dbPath,
    env: { ...process.env, ...env },
  });
}

function withCapture(): { file: string; read: () => Captured; cleanup: () => void } {
  const dir = mkdtempSync(join(tmpdir(), 'cliban-fake-'));
  const file = join(dir, 'capture.json');
  return {
    file,
    read: () => JSON.parse(readFileSync(file, 'utf8')) as Captured,
    cleanup: () => rmSync(dir, { recursive: true, force: true }),
  };
}

test('listIssues builds args and parses NDJSON in order', async () => {
  const cap = withCapture();
  const client = makeClient({
    FAKE_CAPTURE: cap.file,
    FAKE_STDOUT: '{"key":"CLI-2","status":"backlog","title":"b","updated_at":"t"}\n{"key":"CLI-1","status":"done","title":"a","updated_at":"t"}\n',
  });
  const issues = await client.listIssues('CLI');
  assert.deepEqual(issues.map((i) => i.key), ['CLI-2', 'CLI-1']);
  assert.deepEqual(cap.read().argv, ['issue', 'ls', '--project', 'CLI', '--json']);
  cap.cleanup();
});

test('dbPath injects --db before the subcommand args', async () => {
  const cap = withCapture();
  const client = makeClient({ FAKE_CAPTURE: cap.file, FAKE_STDOUT: '' }, '/tmp/x.db');
  await client.listIssues('CLI');
  const argv = cap.read().argv;
  assert.deepEqual(argv, ['issue', 'ls', '--project', 'CLI', '--db', '/tmp/x.db', '--json']);
  cap.cleanup();
});

test('moveIssue passes status and optional note, returns the echo', async () => {
  const cap = withCapture();
  const client = makeClient({
    FAKE_CAPTURE: cap.file,
    FAKE_STDOUT: '{"key":"CLI-9","status":"in-progress","title":"x","updated_at":"u"}',
  });
  const echo = await client.moveIssue('CLI-9', 'in-progress', 'started');
  assert.equal(echo.status, 'in-progress');
  assert.deepEqual(cap.read().argv, [
    'issue', 'mv', 'CLI-9', 'in-progress', '--note', 'started', '--json',
  ]);
  cap.cleanup();
});

test('addIssue uses positional title and pipes the description to stdin', async () => {
  const cap = withCapture();
  const client = makeClient({
    FAKE_CAPTURE: cap.file,
    FAKE_STDOUT: '{"key":"CLI-10","status":"backlog","title":"New","updated_at":"u"}',
  });
  await client.addIssue({
    project: 'CLI',
    title: 'New',
    priority: 'high',
    labels: ['bug', 'ui'],
    description: '## Spec\n\nbody\n',
  });
  const got = cap.read();
  assert.deepEqual(got.argv, [
    'issue', 'add', 'New', '--project', 'CLI', '--priority', 'high',
    '--label', 'bug', '--label', 'ui', '--description-file', '-', '--json',
  ]);
  assert.equal(got.stdin, '## Spec\n\nbody\n');
  cap.cleanup();
});

test('editSection always sends --if-updated-at and pipes content', async () => {
  const cap = withCapture();
  const client = makeClient({
    FAKE_CAPTURE: cap.file,
    FAKE_STDOUT: '{"key":"CLI-9","status":"backlog","title":"x","updated_at":"u2"}',
  });
  await client.editSection('CLI-9', 'spec', 'new spec body', '2026-08-04T10:00:00.000001Z');
  const got = cap.read();
  assert.deepEqual(got.argv, [
    'issue', 'edit', 'CLI-9', '--section', 'spec', '--description-file', '-',
    '--if-updated-at', '2026-08-04T10:00:00.000001Z', '--json',
  ]);
  assert.equal(got.stdin, 'new spec body');
  cap.cleanup();
});

test('showSection returns null on exit 1 (section absent)', async () => {
  const client = makeClient({ FAKE_EXIT: '1', FAKE_STDERR: 'no such section' });
  assert.equal(await client.showSection('CLI-9', 'plan'), null);
});

test('showSection reads via cat --section, raw, without --json', async () => {
  const cap = withCapture();
  const client = makeClient({
    FAKE_CAPTURE: cap.file,
    FAKE_STDOUT: '### Task 1: x\n\n- [ ] **Step 1: y**\n',
  });
  assert.equal(await client.showSection('CLI-9', 'plan'), '### Task 1: x\n\n- [ ] **Step 1: y**\n');
  // `issue cat` is raw-only: it rejects --json, so the args must omit it
  assert.deepEqual(cap.read().argv, ['issue', 'cat', 'CLI-9', '--section', 'plan']);
  cap.cleanup();
});

test('exit 1 elsewhere raises NotFoundError', async () => {
  const client = makeClient({ FAKE_EXIT: '1', FAKE_STDERR: 'no such issue CLI-999' });
  await assert.rejects(client.showIssue('CLI-999'), NotFoundError);
});

test('exit 2 raises ValidationError carrying stderr', async () => {
  const client = makeClient({ FAKE_EXIT: '2', FAKE_STDERR: 'error: bad status "wip"' });
  await assert.rejects(client.moveIssue('CLI-9', 'in-progress'), (err: unknown) => {
    assert.ok(err instanceof ValidationError);
    assert.match(err.message, /bad status/);
    return true;
  });
});

test('exit 2 with a stale write stderr raises ConflictError instead', async () => {
  const client = makeClient({
    FAKE_EXIT: '2',
    FAKE_STDERR: 'stale write: CLI-9 was updated at X, you read Y — re-read and retry',
  });
  await assert.rejects(
    client.editSection('CLI-9', 'spec', 'x', 'Y'),
    ConflictError,
  );
});

test('exit 3 raises InternalError', async () => {
  const client = makeClient({ FAKE_EXIT: '3', FAKE_STDERR: 'db locked' });
  await assert.rejects(client.listIssues('CLI'), InternalError);
});

test('missing binary raises CliMissingError', async () => {
  const client = new ClibanClient({ exePath: '/nonexistent/cliban-definitely-not-here' });
  await assert.rejects(client.listIssues('CLI'), CliMissingError);
});

test('tick and log build the right args', async () => {
  const cap = withCapture();
  const client = makeClient({
    FAKE_CAPTURE: cap.file,
    FAKE_STDOUT: '{"key":"CLI-9","status":"in-progress","title":"x","updated_at":"u"}',
  });
  await client.tick('CLI-9', 2, 3);
  assert.deepEqual(cap.read().argv, [
    'issue', 'tick', 'CLI-9', '--task', '2', '--step', '3', '--json',
  ]);
  await client.log('CLI-9', 'found the root cause');
  assert.deepEqual(cap.read().argv, [
    'issue', 'log', 'CLI-9', 'found the root cause', '--json',
  ]);
  cap.cleanup();
});

test('editMeta maps the patch to flags with CAS', async () => {
  const cap = withCapture();
  const client = makeClient({
    FAKE_CAPTURE: cap.file,
    FAKE_STDOUT: '{"key":"CLI-9","status":"backlog","title":"x","updated_at":"u"}',
  });
  await client.editMeta(
    'CLI-9',
    { priority: 'urgent', addLabels: ['bug'], removeLabels: ['stale'], clearMilestone: true },
    'TS',
  );
  assert.deepEqual(cap.read().argv, [
    'issue', 'edit', 'CLI-9', '--priority', 'urgent', '--label', 'bug',
    '--remove-label', 'stale', '--clear-milestone', '--if-updated-at', 'TS', '--json',
  ]);
  cap.cleanup();
});

test('activity parses NDJSON events', async () => {
  const client = makeClient({
    FAKE_STDOUT: '{"ts":"T2","key":"CLI-9","kind":"status","message":"backlog → in-progress"}\n{"ts":"T1","key":"CLI-8","kind":"log","message":"note"}\n',
  });
  const events = await client.activity('1d', 'CLI');
  assert.equal(events.length, 2);
  assert.equal(events[0]!.kind, 'status');
});
