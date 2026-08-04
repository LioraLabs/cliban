let container: HTMLElement | undefined;

export function toast(level: 'info' | 'error', message: string): void {
  if (!container) {
    container = document.createElement('div');
    container.className = 'toasts';
    document.body.append(container);
  }
  const node = document.createElement('div');
  node.className = `toast toast-${level}`;
  node.textContent = message;
  container.append(node);
  setTimeout(() => node.remove(), level === 'error' ? 8000 : 4000);
}
