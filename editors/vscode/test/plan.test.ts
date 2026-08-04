import { test } from 'node:test';
import assert from 'node:assert/strict';
import { parsePlan } from '../shared/plan';

const REAL_PLAN = `### Task 1: Scaffold the extension

**Files:** a, b

- [x] **Step 1: package.json and build**
- [ ] **Step 2: panel with CSP**
  - indented child bullet, not a step
- [ ] Step 3 without bold markers

### Task 2: Client

Some prose.

- [ ] **Step 1: model types**
- [x] **Step 2: promoted → CLI-77**
`;

test('parses tasks with numbers and titles', () => {
  const tasks = parsePlan(REAL_PLAN);
  assert.equal(tasks.length, 2);
  assert.equal(tasks[0]!.task, 1);
  assert.equal(tasks[0]!.title, 'Scaffold the extension');
  assert.equal(tasks[1]!.task, 2);
});

test('steps are column-zero checkboxes only, numbered within their task', () => {
  const tasks = parsePlan(REAL_PLAN);
  const t1 = tasks[0]!;
  assert.equal(t1.steps.length, 3); // indented bullet excluded
  assert.deepEqual(t1.steps.map((s) => s.step), [1, 2, 3]);
  assert.deepEqual(t1.steps.map((s) => s.done), [true, false, false]);
});

test('step text keeps promotion suffixes and strips checkbox syntax', () => {
  const tasks = parsePlan(REAL_PLAN);
  const last = tasks[1]!.steps[1]!;
  assert.equal(last.done, true);
  assert.match(last.text, /promoted → CLI-77/);
  assert.ok(!last.text.includes('- [x]'));
});

test('plan without task headings yields no tasks', () => {
  assert.deepEqual(parsePlan('- [ ] floating step\n'), []);
});

test('non-numbered H3 headings are ignored as tasks', () => {
  const tasks = parsePlan('### Review Checkpoint: scope\n\n- [ ] **Step 1: x**\n### Task 1: real\n- [ ] **Step 1: y**\n');
  assert.equal(tasks.length, 1);
  assert.equal(tasks[0]!.task, 1);
});
