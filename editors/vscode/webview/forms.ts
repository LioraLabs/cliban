import type { IssueDraft, Label, Milestone, Priority, Status } from '../shared/model';
import { PRIORITIES, STATUSES } from '../shared/model';

export interface CreateFormHandlers {
  onSubmit(requestId: string, draft: IssueDraft): void;
}

interface OpenForm {
  requestId: string;
  showError(message: string): void;
  close(): void;
}

let openForm: OpenForm | undefined;

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

export function openCreateForm(
  host: HTMLElement,
  project: string,
  milestones: Milestone[],
  labels: Label[],
  handlers: CreateFormHandlers,
): void {
  closeCreateForm();

  const overlay = el('div', 'modal-overlay');
  overlay.addEventListener('click', (ev) => {
    if (ev.target === overlay) closeCreateForm();
  });
  const modal = el('div', 'modal');
  modal.append(el('h2', undefined, `New issue in ${project}`));

  const error = el('div', 'form-error');
  error.hidden = true;
  modal.append(error);

  const title = el('input', 'form-input') as HTMLInputElement;
  title.placeholder = 'Title';
  modal.append(title);

  const row = el('div', 'form-row');
  const status = el('select', 'meta-select') as HTMLSelectElement;
  for (const s of STATUSES) {
    const opt = el('option', undefined, s);
    opt.value = s;
    status.append(opt);
  }
  const prio = el('select', 'meta-select') as HTMLSelectElement;
  for (const p of PRIORITIES) {
    const opt = el('option', undefined, p === 'none' ? 'priority' : p);
    opt.value = p;
    prio.append(opt);
  }
  const ms = el('select', 'meta-select') as HTMLSelectElement;
  const msNone = el('option', undefined, 'milestone');
  msNone.value = '';
  ms.append(msNone);
  for (const m of milestones) {
    const opt = el('option', undefined, m.name);
    opt.value = m.name;
    ms.append(opt);
  }
  row.append(status, prio, ms);
  modal.append(row);

  const labelRow = el('div', 'form-row form-labels');
  const picked = new Set<string>();
  for (const l of labels) {
    const chip = el('button', 'chip chip-toggle', l.name);
    chip.addEventListener('click', () => {
      if (picked.has(l.name)) {
        picked.delete(l.name);
        chip.classList.remove('chip-on');
      } else {
        picked.add(l.name);
        chip.classList.add('chip-on');
      }
    });
    labelRow.append(chip);
  }
  if (labels.length) modal.append(labelRow);

  const desc = el('textarea', 'form-textarea') as HTMLTextAreaElement;
  desc.placeholder = 'Description (markdown — e.g. start with ## Spec)';
  desc.rows = 8;
  modal.append(desc);

  const actions = el('div', 'form-row form-actions');
  const cancel = el('button', 'toolbar-btn', 'Cancel');
  cancel.addEventListener('click', () => closeCreateForm());
  const submit = el('button', 'toolbar-btn form-submit', 'Create');
  submit.addEventListener('click', () => {
    const t = title.value.trim();
    if (!t) {
      showError('A title is required.');
      return;
    }
    const requestId = crypto.randomUUID();
    const draft: IssueDraft = { project, title: t };
    if (status.value !== 'backlog') draft.status = status.value as Status;
    if (prio.value !== 'none') draft.priority = prio.value as Priority;
    if (ms.value) draft.milestone = ms.value;
    if (picked.size) draft.labels = [...picked];
    if (desc.value.trim()) draft.description = desc.value;
    openForm = { requestId, showError, close };
    handlers.onSubmit(requestId, draft);
  });
  actions.append(cancel, submit);
  modal.append(actions);

  overlay.append(modal);
  host.append(overlay);
  title.focus();

  function showError(message: string): void {
    error.textContent = message;
    error.hidden = false;
  }
  function close(): void {
    overlay.remove();
    openForm = undefined;
  }
}

export function closeCreateForm(): void {
  openForm?.close();
  document.querySelector('.modal-overlay')?.remove();
  openForm = undefined;
}

/** Route a mutationFailed at the open form; true when it was ours. */
export function createFormFailed(requestId: string, message: string): boolean {
  if (!openForm || openForm.requestId !== requestId) return false;
  openForm.showError(message);
  return true;
}

/** The board refreshed after our create landed — close the modal. */
export function createFormSucceeded(): void {
  closeCreateForm();
}
