#!/usr/bin/env node
// Scripted stand-in for the cliban binary, driven entirely by env vars:
//   FAKE_STDOUT      — exact stdout to emit (default '{}')
//   FAKE_STDERR      — stderr to emit
//   FAKE_EXIT        — exit code (default 0)
//   FAKE_CAPTURE     — path to write {argv, stdin} as JSON, for arg assertions
'use strict';

const fs = require('node:fs');

let stdin = '';
try {
  stdin = fs.readFileSync(0, 'utf8');
} catch {
  // stdin not piped
}

if (process.env.FAKE_CAPTURE) {
  fs.writeFileSync(
    process.env.FAKE_CAPTURE,
    JSON.stringify({ argv: process.argv.slice(2), stdin }),
  );
}

if (process.env.FAKE_STDERR) process.stderr.write(process.env.FAKE_STDERR);
process.stdout.write(process.env.FAKE_STDOUT ?? '{}');
process.exit(Number(process.env.FAKE_EXIT ?? 0));
