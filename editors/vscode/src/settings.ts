import * as vscode from 'vscode';

export interface ClibanSettings {
  executablePath: string;
  dbPath: string | undefined;
  defaultProject: string | undefined;
  watchMode: 'auto' | 'poll' | 'off';
  pollIntervalSeconds: number;
}

export function readSettings(): ClibanSettings {
  const cfg = vscode.workspace.getConfiguration('cliban');
  const dbPath = cfg.get<string>('dbPath', '');
  const defaultProject = cfg.get<string>('defaultProject', '');
  return {
    executablePath: cfg.get<string>('executablePath', 'cliban') || 'cliban',
    dbPath: dbPath === '' ? undefined : dbPath,
    defaultProject: defaultProject === '' ? undefined : defaultProject,
    watchMode: cfg.get<'auto' | 'poll' | 'off'>('watch.mode', 'auto'),
    pollIntervalSeconds: Math.max(2, cfg.get<number>('watch.pollIntervalSeconds', 15)),
  };
}
