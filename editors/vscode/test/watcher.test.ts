import { test } from 'node:test';
import assert from 'node:assert/strict';
import { resolveDbPath } from '../src/watcher';

test('explicit setting wins over everything', () => {
  const p = resolveDbPath('/custom/db.sqlite', {
    CLIBAN_DB: '/env/db',
    XDG_DATA_HOME: '/xdg',
    HOME: '/home/u',
  });
  assert.equal(p, '/custom/db.sqlite');
});

test('CLIBAN_DB beats XDG', () => {
  const p = resolveDbPath(undefined, {
    CLIBAN_DB: '/env/db.sqlite',
    XDG_DATA_HOME: '/xdg',
    HOME: '/home/u',
  });
  assert.equal(p, '/env/db.sqlite');
});

test('XDG_DATA_HOME chain', () => {
  const p = resolveDbPath(undefined, { XDG_DATA_HOME: '/xdg', HOME: '/home/u' });
  assert.equal(p, '/xdg/cliban/cliban.db');
});

test('falls back to ~/.local/share', () => {
  const p = resolveDbPath(undefined, { HOME: '/home/u' });
  assert.equal(p, '/home/u/.local/share/cliban/cliban.db');
});
