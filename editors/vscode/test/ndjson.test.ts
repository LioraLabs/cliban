import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parseNdjson } from '../src/client/ndjson';

test('parses one object per line preserving order', () => {
  const out = parseNdjson('{"key":"A-1"}\n{"key":"A-2"}\n');
  assert.deepEqual(out, [{ key: 'A-1' }, { key: 'A-2' }]);
});

test('tolerates trailing newline, blank lines, and CRLF', () => {
  const out = parseNdjson('{"a":1}\r\n\n{"b":2}\n\n');
  assert.deepEqual(out, [{ a: 1 }, { b: 2 }]);
});

test('empty input yields empty array', () => {
  assert.deepEqual(parseNdjson(''), []);
  assert.deepEqual(parseNdjson('\n'), []);
});

test('a malformed line throws with the line number', () => {
  assert.throws(() => parseNdjson('{"ok":true}\nnot json\n'), /line 2/);
});
