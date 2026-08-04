import { test } from 'node:test';
import assert from 'node:assert/strict';
import { BoardStore } from '../src/store';
import type { Issue } from '../shared/model';

function issue(key: string, status: Issue['status'], title = key): Issue {
  return { key, title, status, updated_at: 'T' };
}

test('setBoard keeps ls-order and exposes a snapshot', () => {
  const store = new BoardStore();
  const issues = [issue('C-2', 'backlog'), issue('C-1', 'backlog'), issue('C-3', 'done')];
  store.setBoard('C', issues, [], []);
  const snap = store.snapshot();
  assert.equal(snap.project, 'C');
  assert.deepEqual(snap.issues.map((i) => i.key), ['C-2', 'C-1', 'C-3']);
});

test('applyEcho replaces the issue in place by key', () => {
  const store = new BoardStore();
  store.setBoard('C', [issue('C-1', 'backlog'), issue('C-2', 'backlog')], [], []);
  store.applyEcho({ ...issue('C-2', 'in-progress'), title: 'moved' });
  const snap = store.snapshot();
  assert.deepEqual(snap.issues.map((i) => [i.key, i.status]), [
    ['C-1', 'backlog'],
    ['C-2', 'in-progress'],
  ]);
});

test('applyEcho on an unknown key appends (issue created elsewhere)', () => {
  const store = new BoardStore();
  store.setBoard('C', [issue('C-1', 'backlog')], [], []);
  store.applyEcho(issue('C-9', 'backlog'));
  assert.deepEqual(store.snapshot().issues.map((i) => i.key), ['C-1', 'C-9']);
});

test('change listener fires on every mutation with the fresh snapshot', () => {
  const store = new BoardStore();
  const seen: string[][] = [];
  store.onChange((snap) => seen.push(snap.issues.map((i) => i.key)));
  store.setBoard('C', [issue('C-1', 'backlog')], [], []);
  store.applyEcho(issue('C-2', 'backlog'));
  assert.deepEqual(seen, [['C-1'], ['C-1', 'C-2']]);
});

test('applyOptimistic patches immediately; commit swaps in the echo', () => {
  const store = new BoardStore();
  store.setBoard('C', [issue('C-1', 'backlog')], [], []);
  store.applyOptimistic('req1', 'C-1', { status: 'in-progress' });
  assert.equal(store.snapshot().issues[0]!.status, 'in-progress');
  store.commit('req1', { ...issue('C-1', 'in-progress'), updated_at: 'T2' });
  assert.equal(store.snapshot().issues[0]!.updated_at, 'T2');
});

test('rollback restores the pre-optimistic issue', () => {
  const store = new BoardStore();
  store.setBoard('C', [issue('C-1', 'backlog')], [], []);
  store.applyOptimistic('req1', 'C-1', { status: 'done' });
  store.rollback('req1');
  assert.equal(store.snapshot().issues[0]!.status, 'backlog');
});

test('interleaved optimistic ops roll back independently', () => {
  const store = new BoardStore();
  store.setBoard('C', [issue('C-1', 'backlog'), issue('C-2', 'blocked')], [], []);
  store.applyOptimistic('a', 'C-1', { status: 'in-progress' });
  store.applyOptimistic('b', 'C-2', { status: 'in-review' });
  store.rollback('a');
  store.commit('b', issue('C-2', 'in-review'));
  const snap = store.snapshot();
  assert.equal(snap.issues[0]!.status, 'backlog');
  assert.equal(snap.issues[1]!.status, 'in-review');
});

test('rollback after the issue vanished is a no-op', () => {
  const store = new BoardStore();
  store.setBoard('C', [issue('C-1', 'backlog')], [], []);
  store.applyOptimistic('a', 'C-1', { status: 'done' });
  store.setBoard('C', [], [], []);
  store.rollback('a');
  assert.deepEqual(store.snapshot().issues, []);
});

test('mergeActivity dedupes by ts/key/kind and advances lastActivityTs', () => {
  const store = new BoardStore();
  const e1 = { ts: '2026-08-04T10:00:00Z', key: 'C-1', kind: 'status' };
  const e2 = { ts: '2026-08-04T11:00:00Z', key: 'C-2', kind: 'log' };
  store.mergeActivity([e2, e1]); // newest-first, as the CLI emits
  store.mergeActivity([e2]); // watcher refetch overlaps
  assert.equal(store.snapshot().events.length, 2);
  assert.equal(store.lastActivityTs, '2026-08-04T11:00:00Z');
});

test('mergeActivity keeps newest first and caps the feed', () => {
  const store = new BoardStore();
  const events = Array.from({ length: 120 }, (_, i) => ({
    ts: `2026-08-04T10:00:${String(i % 60).padStart(2, '0')}.${String(i).padStart(3, '0')}Z`,
    key: `C-${i}`,
    kind: 'log',
  }));
  store.mergeActivity(events);
  const snap = store.snapshot();
  assert.equal(snap.events.length, 100);
  assert.ok(snap.events[0]!.ts >= snap.events[1]!.ts);
});
