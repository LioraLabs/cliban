import type { IssueDetailMsg } from '../shared/protocol';
import { parsePlan } from '../shared/plan';
import { renderMarkdown } from './md';

export interface DrawerHandlers {
  onClose(): void;
  onOpenIssue(key: string): void;
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

function planSection(md: string | null): HTMLElement {
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
      box.disabled = true; // ticking arrives with drawer mutations
      box.dataset['task'] = String(task.task);
      box.dataset['step'] = String(step.step);
      item.append(box, el('span', step.done ? 'plan-step-done' : undefined, ` ${step.text}`));
      list.append(item);
    }
    taskEl.append(list);
    section.append(taskEl);
  }
  return section;
}

export function renderDrawer(
  host: HTMLElement,
  detail: IssueDetailMsg,
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

  const body = el('div', 'drawer-body');
  body.append(mdSection('Spec', sections.spec));
  body.append(planSection(sections.plan));
  if (sections.notes !== null) body.append(mdSection('Notes', sections.notes));
  body.append(mdSection('Activity', sections.activity));
  drawer.append(body);

  overlay.append(drawer);
  host.append(overlay);
}

export function closeDrawer(host: HTMLElement): void {
  host.querySelector('.drawer-overlay')?.remove();
}
