import { build } from 'esbuild';
import { globSync, mkdirSync } from 'node:fs';

const testsOnly = process.argv.includes('--tests');
const watch = process.argv.includes('--watch');

const common = {
  bundle: true,
  sourcemap: true,
  logLevel: 'info',
};

async function buildBundles() {
  await build({
    ...common,
    entryPoints: ['src/extension.ts'],
    outfile: 'dist/extension.js',
    platform: 'node',
    format: 'cjs',
    target: 'node20',
    external: ['vscode'],
  });
  await build({
    ...common,
    entryPoints: ['webview/main.ts'],
    outfile: 'dist/webview.js',
    platform: 'browser',
    format: 'iife',
    target: 'es2022',
  });
}

async function buildTests() {
  mkdirSync('dist-test', { recursive: true });
  const entries = globSync('test/*.test.ts');
  if (entries.length === 0) return;
  await build({
    ...common,
    entryPoints: entries,
    outdir: 'dist-test',
    platform: 'node',
    format: 'cjs',
    target: 'node20',
    external: ['vscode'],
  });
}

if (testsOnly) {
  await buildTests();
} else {
  await buildBundles();
}
if (watch) {
  console.error('watch mode not wired; re-run on change');
}
