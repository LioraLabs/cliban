import { test } from 'node:test';
import assert from 'node:assert/strict';
import { renderMarkdown } from '../webview/md';

test('escapes HTML in text — injection renders inert', () => {
  const html = renderMarkdown('hello <script>alert(1)</script> & "quotes"');
  assert.ok(!html.includes('<script>'));
  assert.ok(html.includes('&lt;script&gt;'));
  assert.ok(html.includes('&amp;'));
});

test('escapes HTML inside code fences and inline code', () => {
  const html = renderMarkdown('```\n<img onerror=x>\n```\nand `<b>inline</b>`');
  assert.ok(!html.includes('<img'));
  assert.ok(html.includes('&lt;img onerror=x&gt;'));
  assert.ok(html.includes('<code>&lt;b&gt;inline&lt;/b&gt;</code>'));
});

test('renders headings, bold, italic, links', () => {
  const html = renderMarkdown('## Spec\n\n### Detail\n\n**bold** and *ital* and [x](https://e.io)');
  assert.ok(html.includes('<h2>Spec</h2>'));
  assert.ok(html.includes('<h3>Detail</h3>'));
  assert.ok(html.includes('<strong>bold</strong>'));
  assert.ok(html.includes('<em>ital</em>'));
  assert.ok(html.includes('<a href="https://e.io"'));
});

test('only http(s) link targets survive', () => {
  const html = renderMarkdown('[bad](javascript:alert(1))');
  assert.ok(!html.includes('javascript:'));
});

test('renders lists and checkboxes', () => {
  const html = renderMarkdown('- one\n- two\n\n- [ ] open\n- [x] closed');
  assert.ok(html.includes('<li>one</li>'));
  assert.match(html, /checkbox[^>]*disabled[^>]*>\s*open|open/);
  assert.ok(html.includes('checked'));
});

test('paragraphs separated by blank lines', () => {
  const html = renderMarkdown('first para\n\nsecond para');
  assert.equal((html.match(/<p>/g) ?? []).length, 2);
});
