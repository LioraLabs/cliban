// Parser for the cliban plan contract: `### Task N: title` headings with
// column-zero GFM checkboxes as steps. Mirrors what `issue tick` accepts:
// indented bullets are not steps, steps number 1..n within their task.

export interface PlanStep {
  step: number;
  text: string;
  done: boolean;
}

export interface PlanTask {
  task: number;
  title: string;
  steps: PlanStep[];
}

const TASK_RE = /^### Task (\d+):\s*(.*)$/;
const STEP_RE = /^- \[([ xX])\]\s+(.*)$/;

export function parsePlan(md: string): PlanTask[] {
  const tasks: PlanTask[] = [];
  let current: PlanTask | null = null;
  for (const line of md.split('\n')) {
    const taskMatch = TASK_RE.exec(line);
    if (taskMatch) {
      current = { task: Number(taskMatch[1]), title: taskMatch[2]!.trim(), steps: [] };
      tasks.push(current);
      continue;
    }
    if (line.startsWith('### ')) {
      // some other H3 (e.g. a review checkpoint) — steps under it belong to no task
      current = null;
      continue;
    }
    const stepMatch = STEP_RE.exec(line);
    if (stepMatch && current) {
      const text = stepMatch[2]!.replace(/\*\*/g, '').trim();
      current.steps.push({
        step: current.steps.length + 1,
        text,
        done: stepMatch[1] !== ' ',
      });
    }
  }
  return tasks;
}
