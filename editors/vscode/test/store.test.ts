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
