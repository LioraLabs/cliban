import type { IssueDetailMsg } from '../shared/protocol';
import type { Label, MetaPatch, Milestone, Priority } from '../shared/model';
import { PRIORITIES } from '../shared/model';
import { parsePlan } from '../shared/plan';
import { renderMarkdown } from './md';

export type EditableSection = 'spec' | 'plan' | 'notes';

export interface DrawerHandlers {
  onClose(): void;
  onOpenIssue(key: string): void;
  onTickStep(key: string, task: number, step: number): void;
  onAddLog(key: string, message: string): void;
  onEditMeta(key: string, ifUpdatedAt: string, patch: MetaPatch): void;
  onEditSection(key: string, section: EditableSection, content: string, ifUpdatedAt: string): void;
}

export interface DrawerContext {
  milestones: Milestone[];
  labels: Label[];
  /** Unsent editor text to restore (e.g. after a stale-write conflict), by section. */
  drafts?: Partial<Record<EditableSection, string>>;
}

function sectionEditor(
  section: HTMLElement,
  initial: string,
  onSave: (content: string) => void,
): void {
  const existing = section.querySelector('.md, .drawer-empty, .plan-task');
  const editor = el('div', 'section-editor');
  const area = el('textarea', 'form-textarea') as HTMLTextAreaElement;
  area.value = initial;
  area.rows = Math.min(20, Math.max(6, initial.split('\n').length + 2));
  const actions = el('div', 'form-row form-actions');
  const cancel = el('button', 'toolbar-btn', 'Cancel');
  cancel.addEventListener('click', () => editor.remove());
  const save = el('button', 'toolbar-btn form-submit', 'Save');
  save.addEventListener('click', () => onSave(area.value));
  actions.append(cancel, save);
  editor.append(area, actions);
  existing?.after(editor);
  if (!existing) section.append(editor);
  area.focus();
}

function editButton(
  section: HTMLElement,
  key: string,
  name: EditableSection,
  currentMd: string | null,
  cas: string,
  handlers: DrawerHandlers,
  draft?: string,
): void {
  const title = section.querySelector('.drawer-section-title');
  const btn = el('button', 'chip section-edit-btn', 'edit');
  btn.addEventListener('click', () =>
    sectionEditor(section, currentMd ?? '', (content) =>
      handlers.onEditSection(key, name, content, cas),
    ),
  );
  title?.append(btn);
  if (draft !== undefined) {
    sectionEditor(section, draft, (content) => handlers.onEditSection(key, name, content, cas));
  }
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

  const cas = issue.updated_at;
  const body = el('div', 'drawer-body');
  const spec = mdSection('Spec', sections.spec);
  if (sections.spec !== null) {
    editButton(spec, issue.key, 'spec', sections.spec, cas, handlers, ctx.drafts?.spec);
  }
  body.append(spec);
  const plan = planSection(issue.key, sections.plan, handlers);
  if (sections.plan !== null) {
    editButton(plan, issue.key, 'plan', sections.plan, cas, handlers, ctx.drafts?.plan);
  }
  body.append(plan);
  if (sections.notes !== null) {
    const notes = mdSection('Notes', sections.notes);
    editButton(notes, issue.key, 'notes', sections.notes, cas, handlers, ctx.drafts?.notes);
    body.append(notes);
  }
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
