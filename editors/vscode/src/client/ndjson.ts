/** Parse cliban's NDJSON list output: one compact JSON object per line. */
export function parseNdjson(text: string): unknown[] {
  const out: unknown[] = [];
  const lines = text.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!.replace(/\r$/, '').trim();
    if (line === '') continue;
    try {
      out.push(JSON.parse(line));
    } catch {
      throw new Error(`invalid NDJSON at line ${i + 1}: ${line.slice(0, 80)}`);
    }
  }
  return out;
}
