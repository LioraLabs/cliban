import type { ActivityEvent } from '../shared/model';

export interface FeedHandlers {
  onOpenIssue(key: string): void;
}

let collapsed = true;
let knownIds = new Set<string>();

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

function relative(ts: string): string {
  const then = Date.parse(ts);
  if (Number.isNaN(then)) return ts;
  const s = Math.max(0, Math.floor((Date.now() - then) / 1000));
  if (s < 60) return `${s}s ago`;
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
  return `${Math.floor(s / 86400)}d ago`;
}

export function renderFeed(
  root: HTMLElement,
  events: ActivityEvent[],
  handlers: FeedHandlers,
): void {
  root.querySelector('.feed')?.remove();
  const feed = el('div', 'feed');

  const head = el('button', 'feed-head');
  head.append(el('span', undefined, `Activity (${events.length})`));
  head.append(el('span', 'feed-caret', collapsed ? '▴' : '▾'));
  head.addEventListener('click', () => {
    collapsed = !collapsed;
    renderFeed(root, events, handlers);
  });
  feed.append(head);

  if (!collapsed) {
    const list = el('div', 'feed-list');
    const nextKnown = new Set<string>();
    for (const ev of events) {
      const id = `${ev.ts} ${ev.key} ${ev.kind}`;
      nextKnown.add(id);
      const row = el('div', 'feed-row');
      if (knownIds.size > 0 && !knownIds.has(id)) row.classList.add('feed-new');
      const keyBtn = el('button', 'feed-key', ev.key);
      keyBtn.addEventListener('click', () => handlers.onOpenIssue(ev.key));
      row.append(
        el('span', 'feed-time', relative(ev.ts)),
        el('span', `feed-kind feed-kind-${ev.kind}`, ev.kind),
        keyBtn,
      );
      const text = ev.message ?? ev.title ?? '';
      if (text) row.append(el('span', 'feed-msg', text));
      if (ev.actor) row.append(el('span', 'feed-actor', ev.actor));
      list.append(row);
    }
    knownIds = nextKnown;
    feed.append(list);
  }

  root.append(feed);
}
