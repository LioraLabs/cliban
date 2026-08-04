// Minimal escape-first renderer for cliban's constrained description markdown:
// H2/H3 headings, column-zero lists and GFM checkboxes, fenced/inline code,
// bold/italic/links, paragraphs. Everything else renders as escaped text.

function escapeHtml(text: string): string {
  return text
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

function inline(text: string): string {
  let out = '';
  // tokenize inline code first so nothing inside backticks is styled
  const parts = text.split(/(`[^`]*`)/);
  for (const part of parts) {
    if (part.startsWith('`') && part.endsWith('`') && part.length >= 2) {
      out += `<code>${escapeHtml(part.slice(1, -1))}</code>`;
      continue;
    }
    let chunk = escapeHtml(part);
    chunk = chunk.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (m, label: string, href: string) => {
      if (!/^https?:\/\//.test(href)) return label;
      return `<a href="${href}">${label}</a>`;
    });
    chunk = chunk.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
    chunk = chunk.replace(/(^|[^*])\*([^*]+)\*/g, '$1<em>$2</em>');
    out += chunk;
  }
  return out;
}

export function renderMarkdown(md: string): string {
  const lines = md.split('\n');
  const out: string[] = [];
  let para: string[] = [];
  let list: string[] | null = null;
  let fence: string[] | null = null;

  const flushPara = () => {
    if (para.length) {
      out.push(`<p>${inline(para.join(' '))}</p>`);
      para = [];
    }
  };
  const flushList = () => {
    if (list) {
      out.push(`<ul>${list.join('')}</ul>`);
      list = null;
    }
  };

  for (const raw of lines) {
    if (fence !== null) {
      if (raw.startsWith('```')) {
        out.push(`<pre><code>${escapeHtml(fence.join('\n'))}</code></pre>`);
        fence = null;
      } else {
        fence.push(raw);
      }
      continue;
    }
    const line = raw.replace(/\s+$/, '');
    if (line.startsWith('```')) {
      flushPara();
      flushList();
      fence = [];
      continue;
    }
    const heading = /^(#{2,4})\s+(.*)$/.exec(line);
    if (heading) {
      flushPara();
      flushList();
      const level = heading[1]!.length;
      out.push(`<h${level}>${inline(heading[2]!)}</h${level}>`);
      continue;
    }
    const checkbox = /^- \[([ xX])\]\s+(.*)$/.exec(line);
    if (checkbox) {
      flushPara();
      list = list ?? [];
      const checked = checkbox[1] !== ' ' ? ' checked' : '';
      list.push(
        `<li class="task"><input type="checkbox" disabled${checked}> ${inline(checkbox[2]!)}</li>`,
      );
      continue;
    }
    const bullet = /^[-*]\s+(.*)$/.exec(line);
    if (bullet) {
      flushPara();
      list = list ?? [];
      list.push(`<li>${inline(bullet[1]!)}</li>`);
      continue;
    }
    if (line.trim() === '') {
      flushPara();
      flushList();
      continue;
    }
    flushList();
    para.push(line.trim());
  }
  if (fence !== null) out.push(`<pre><code>${escapeHtml(fence.join('\n'))}</code></pre>`);
  flushPara();
  flushList();
  return out.join('\n');
}
