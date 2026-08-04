import type { IssueDetailMsg } from '../shared/protocol';
import type { Label, MetaPatch, Milestone, Priority } from '../shared/model';
import { PRIORITIES } from '../shared/model';
import { parsePlan } from '../shared/plan';
import { renderMarkdown } from './md';

export interface DrawerHandlers {
  onClose(): void;
  onOpenIssue(key: string): void;
  onTickStep(key: string, task: number, step: number): void;
  onAddLog(key: string, message: string): void;
  onEditMeta(key: string, ifUpdatedAt: string, patch: MetaPatch): void;
}

export interface DrawerContext {
  milestones: Milestone[];
  labels: Label[];
}

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function mdSection(title: string, md: string | null): HTMLElement {
  const section = el('section', 'drawer-section');
  section.append(el('h3', 'drawer-section-title', title));
  if (md === null || md.trim() === '') {
    section.append(el('p', 'drawer-empty', `no ${title.toLowerCase()} yet`));
  } else {
    const body = el('div', 'md');
    body.innerHTML = renderMarkdown(md);
    section.append(body);
  }
  return section;
}

function planSection(key: string, md: string | null, handlers: DrawerHandlers): HTMLElement {
  const section = el('section', 'drawer-section');
  section.append(el('h3', 'drawer-section-title', 'Plan'));
  if (md === null || md.trim() === '') {
    section.append(el('p', 'drawer-empty', 'no plan yet'));
    return section;
  }
  const tasks = parsePlan(md);
  if (tasks.length === 0) {
    const body = el('div', 'md');
    body.innerHTML = renderMarkdown(md);
    section.append(body);
    return section;
  }
  for (const task of tasks) {
    const taskEl = el('div', 'plan-task');
    taskEl.append(el('h4', 'plan-task-title', `Task ${task.task}: ${task.title}`));
    const list = el('ul', 'plan-steps');
    for (const step of task.steps) {
      const item = el('li', 'plan-step');
      const box = el('input') as HTMLInputElement;
      box.type = 'checkbox';
      box.checked = step.done;
      // ticking is one-way: done steps stay done (the CLI has no untick)
      box.disabled = step.done;
      box.addEventListener('change', () => {
        box.disabled = true;
        handlers.onTickStep(key, task.task, step.step);
      });
      item.append(box, el('span', step.done ? 'plan-step-done' : undefined, ` ${step.text}`));
      list.append(item);
    }
    taskEl.append(list);
    section.append(taskEl);
  }
  return section;
}

function logInput(key: string, handlers: DrawerHandlers): HTMLElement {
  const row = el('div', 'log-input-row');
  const input = el('input', 'log-input') as HTMLInputElement;
  input.type = 'text';
  input.placeholder = 'Add a note to the activity log…';
  const send = () => {
    const message = input.value.trim();
    if (!message) return;
    input.value = '';
    handlers.onAddLog(key, message);
  };
  input.addEventListener('keydown', (ev) => {
    if (ev.key === 'Enter') send();
  });
  const btn = el('button', 'toolbar-btn', 'Log');
  btn.addEventListener('click', send);
  row.append(input, btn);
  return row;
}

function metaEditors(
  detail: IssueDetailMsg,
  ctx: DrawerContext,
  handlers: DrawerHandlers,
): HTMLElement {
  const { issue } = detail;
  const cas = issue.updated_at;
  const row = el('div', 'meta-editors');

  const prio = el('select', 'meta-select') as HTMLSelectElement;
  for (const p of PRIORITIES) {
    const opt = el('option', undefined, p === 'none' ? 'priority: none' : `priority: ${p}`);
    opt.value = p;
    if ((issue.priority ?? 'none') === p) opt.selected = true;
    prio.append(opt);
  }
  prio.addEventListener('change', () =>
    handlers.onEditMeta(issue.key, cas, { priority: prio.value as Priority }),
  );
  row.append(prio);

  const ms = el('select', 'meta-select') as HTMLSelectElement;
  const noneOpt = el('option', undefined, 'milestone: none');
  noneOpt.value = '';
  ms.append(noneOpt);
  for (const m of ctx.milestones) {
    const opt = el('option', undefined, `◇ ${m.name}`);
    opt.value = m.name;
    if (issue.milestone === m.name) opt.selected = true;
    ms.append(opt);
  }
  ms.addEventListener('change', () => {
    const patch: MetaPatch = ms.value === '' ? { clearMilestone: true } : { milestone: ms.value };
    handlers.onEditMeta(issue.key, cas, patch);
  });
  row.append(ms);

  const labelSel = el('select', 'meta-select') as HTMLSelectElement;
  const head = el('option', undefined, 'labels…');
  head.value = '';
  head.disabled = false;
  labelSel.append(head);
  const attached = new Set(issue.labels ?? []);
  for (const l of ctx.labels) {
    const opt = el('option', undefined, `${attached.has(l.name) ? '✓ ' : ''}${l.name}`);
    opt.value = l.name;
    labelSel.append(opt);
  }
  labelSel.addEventListener('change', () => {
    const name = labelSel.value;
    if (!name) return;
    labelSel.value = '';
    const patch: MetaPatch = attached.has(name)
      ? { removeLabels: [name] }
      : { addLabels: [name] };
    handlers.onEditMeta(issue.key, cas, patch);
  });
  row.append(labelSel);

  return row;
}

export function renderDrawer(
  host: HTMLElement,
  detail: IssueDetailMsg,
  ctx: DrawerContext,
  handlers: DrawerHandlers,
): void {
  closeDrawer(host);
  const { issue, sections } = detail;

  const overlay = el('div', 'drawer-overlay');
  overlay.addEventListener('click', (ev) => {
    if (ev.target === overlay) handlers.onClose();
  });
  const drawer = el('aside', 'drawer');

  const head = el('div', 'drawer-head');
  head.append(el('span', 'card-key', issue.key));
  const close = el('button', 'toolbar-btn drawer-close', '✕');
  close.addEventListener('click', () => handlers.onClose());
  head.append(close);
  drawer.append(head);

  drawer.append(el('h2', 'drawer-title', issue.title));

  const meta = el('div', 'drawer-meta');
  meta.append(el('span', `chip status-${issue.status}`, issue.status));
  if (issue.priority && issue.priority !== 'none') {
    meta.append(el('span', 'chip', `priority: ${issue.priority}`));
  }
  if (issue.milestone) meta.append(el('span', 'chip chip-milestone', `◇ ${issue.milestone}`));
  if (issue.due_date) meta.append(el('span', 'chip', `due ${issue.due_date}`));
  if (issue.claimed_by) meta.append(el('span', 'chip', `⛿ ${issue.claimed_by}`));
  for (const label of issue.labels ?? []) meta.append(el('span', 'chip', label));
  if (issue.parent) {
    const parent = el('button', 'chip chip-parent', `↳ ${issue.parent}`);
    parent.addEventListener('click', () => handlers.onOpenIssue(issue.parent!));
    meta.append(parent);
  }
  drawer.append(meta);
  drawer.append(metaEditors(detail, ctx, handlers));

  const body = el('div', 'drawer-body');
  body.append(mdSection('Spec', sections.spec));
  body.append(planSection(issue.key, sections.plan, handlers));
  if (sections.notes !== null) body.append(mdSection('Notes', sections.notes));
  const activity = mdSection('Activity', sections.activity);
  activity.append(logInput(issue.key, handlers));
  body.append(activity);
  drawer.append(body);

  overlay.append(drawer);
  host.append(overlay);
}

export function closeDrawer(host: HTMLElement): void {
  host.querySelector('.drawer-overlay')?.remove();
}
