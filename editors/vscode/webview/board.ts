import type { BoardMsg } from '../shared/protocol';
import type { Issue, Status } from '../shared/model';
import { STATUSES } from '../shared/model';

const COLUMN_TITLES: Record<Status, string> = {
  backlog: 'Backlog',
  'in-progress': 'In Progress',
  blocked: 'Blocked',
  'in-review': 'In Review',
  done: 'Done',
};

export interface BoardHandlers {
  onOpenIssue(key: string): void;
  onOpenIssueDoc(key: string): void;
  onPickProject(): void;
  onPickMilestone(): void;
  onRefresh(): void;
  onMoveIssue(key: string, toStatus: Status): void;
  onNewIssue(): void;
  onArchiveDone(): void;
  onOpenSettings(): void;
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

function renderCard(issue: Issue, handlers: BoardHandlers): HTMLElement {
  const card = el('div', 'card');
  card.dataset['key'] = issue.key;
  card.draggable = true;
  card.addEventListener('dragstart', (ev) => {
    ev.dataTransfer?.setData('text/cliban-key', issue.key);
    ev.dataTransfer?.setData('text/cliban-status', issue.status);
    card.classList.add('dragging');
  });
  card.addEventListener('dragend', () => card.classList.remove('dragging'));

  const head = el('div', 'card-head');
  const key = el('span', 'card-key', issue.key);
  head.append(key);
  if (issue.priority && issue.priority !== 'none') {
    const dot = el('span', `prio prio-${issue.priority}`);
    dot.title = issue.priority;
    head.append(dot);
  }
  if (issue.claimed_by) {
    const claim = el('span', 'claim-badge', '⛿');
    claim.title = `claimed by ${issue.claimed_by}`;
    head.append(claim);
  }
  const details = el('button', 'card-details-btn', '▤');
  details.title = 'Quick view (tick steps, log, edit fields)';
  details.addEventListener('click', (ev) => {
    ev.stopPropagation();
    handlers.onOpenIssue(issue.key);
  });
  head.append(details);
  card.append(head);

  card.append(el('div', 'card-title', issue.title));

  const chips = el('div', 'card-chips');
  if (issue.parent) {
    const parent = el('button', 'chip chip-parent', `↳ ${issue.parent}`);
    parent.addEventListener('click', (ev) => {
      ev.stopPropagation();
      handlers.onOpenIssue(issue.parent!);
    });
    chips.append(parent);
  }
  for (const label of issue.labels ?? []) chips.append(el('span', 'chip', label));
  if (issue.milestone) chips.append(el('span', 'chip chip-milestone', `◇ ${issue.milestone}`));
  if (chips.childElementCount > 0) card.append(chips);

  // click = open as an editable document (the TUI `e` reflex);
  // the ▤ button opens the quick-view drawer
  card.addEventListener('click', () => handlers.onOpenIssueDoc(issue.key));
  return card;
}

export function renderBoard(root: HTMLElement, msg: BoardMsg, handlers: BoardHandlers): void {
  root.replaceChildren();

  const toolbar = el('div', 'toolbar');
  const projectBtn = el('button', 'toolbar-btn project-btn', msg.project);
  projectBtn.title = 'Switch project';
  projectBtn.addEventListener('click', () => handlers.onPickProject());
  const msLabel =
    msg.milestoneFilter === undefined
      ? '◇ all'
      : msg.milestoneFilter === null
        ? '◇ none'
        : `◇ ${msg.milestoneFilter}`;
  const msBtn = el('button', 'toolbar-btn', msLabel);
  msBtn.title = 'Switch milestone';
  msBtn.addEventListener('click', () => handlers.onPickMilestone());
  const refreshBtn = el('button', 'toolbar-btn', '↻');
  refreshBtn.title = 'Refresh';
  refreshBtn.addEventListener('click', () => handlers.onRefresh());
  const newBtn = el('button', 'toolbar-btn', '+ New');
  newBtn.title = 'New issue';
  newBtn.addEventListener('click', () => handlers.onNewIssue());
  toolbar.append(projectBtn, msBtn, newBtn, refreshBtn);
  root.append(toolbar);

  const board = el('div', 'board');
  for (const status of STATUSES) {
    const issues = msg.issues.filter((i) => i.status === status);
    const column = el('div', 'column');
    column.dataset['status'] = status;
    const head = el('div', 'column-head');
    head.append(el('span', 'column-title', COLUMN_TITLES[status]));
    head.append(el('span', 'column-count', String(issues.length)));
    column.append(head);
    const body = el('div', 'column-body');
    for (const issue of issues) body.append(renderCard(issue, handlers));
    column.append(body);
    if (status === 'done' && issues.length > 0) {
      const archive = el('button', 'column-footer-btn', 'Archive done…');
      archive.addEventListener('click', () => handlers.onArchiveDone());
      column.append(archive);
    }
    column.addEventListener('dragover', (ev) => {
      const from = ev.dataTransfer?.types.includes('text/cliban-key');
      if (!from) return;
      ev.preventDefault();
      column.classList.add('drop-target');
    });
    column.addEventListener('dragleave', () => column.classList.remove('drop-target'));
    column.addEventListener('drop', (ev) => {
      ev.preventDefault();
      column.classList.remove('drop-target');
      const key = ev.dataTransfer?.getData('text/cliban-key');
      const fromStatus = ev.dataTransfer?.getData('text/cliban-status');
      // same-column drops are a reorder, which the CLI cannot express — snap back
      if (!key || fromStatus === status) return;
      handlers.onMoveIssue(key, status);
    });
    board.append(column);
  }
  root.append(board);
}

export function renderErrorState(
  root: HTMLElement,
  kind: string,
  message: string,
  handlers?: Pick<BoardHandlers, 'onOpenSettings' | 'onRefresh'>,
): void {
  root.replaceChildren();
  const pane = el('div', 'empty-state');
  if (kind === 'cli-missing') {
    pane.append(el('h2', undefined, 'cliban not found'));
    pane.append(
      el('p', undefined, 'The cliban CLI is not installed or not on PATH.'),
      el('p', undefined, 'Install it with: cargo install cliban  ·  brew install lioralabs/tap/cliban  ·  AUR: cliban'),
    );
    if (handlers) {
      const row = el('div', 'form-row form-actions empty-actions');
      const settings = el('button', 'toolbar-btn', 'Open settings');
      settings.addEventListener('click', () => handlers.onOpenSettings());
      const retry = el('button', 'toolbar-btn form-submit', 'Retry');
      retry.addEventListener('click', () => handlers.onRefresh());
      row.append(settings, retry);
      pane.append(row);
    }
  } else {
    pane.append(el('h2', undefined, 'Board unavailable'), el('p', undefined, message));
  }
  root.append(pane);
}
