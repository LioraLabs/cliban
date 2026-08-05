import { test } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync } from 'node:fs';
import { join } from 'node:path';

// dist/ is built by `npm run build`, which `npm run check` runs before tests.
const dist = join(__dirname, '..', 'dist');

test('build produces the extension and webview bundles', () => {
  for (const f of ['extension.js', 'webview.js', 'webview.css']) {
    assert.ok(existsSync(join(dist, f)), `missing dist/${f}`);
  }
});
