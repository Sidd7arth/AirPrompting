#!/usr/bin/env node
/**
 * Guard: the frontend `api` object (src/services/ipc.ts) and the Tauri
 * `invoke_handler` (src-tauri/src/lib.rs) must expose the exact same command
 * names. A rename on one side silently breaks the other at runtime, so it is
 * checked here at CI time instead.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const root = process.cwd();
const ipcTs = readFileSync(join(root, 'src/services/ipc.ts'), 'utf8');
const libRs = readFileSync(join(root, 'src-tauri/src/lib.rs'), 'utf8');

// Frontend: every `call<...>("command_name"` inside ipc.ts.
const fe = new Set();
for (const m of ipcTs.matchAll(/call<[^>]*>\(\s*['"]([a-z_]+)['"]/g)) fe.add(m[1]);

// Backend: command function names registered in invoke_handler.
const handlers = libRs.match(/invoke_handler\(tauri::generate_handler!\s*\[([\s\S]*?)\]\)/);
if (!handlers) {
  console.error('FAIL: invoke_handler/generate_handler not found in lib.rs');
  process.exit(1);
}
const be = new Set();
for (const line of handlers[1].split('\n')) {
  const m = line.match(/([a-z_0-9]+)\s*,?\s*$/);
  if (m) be.add(m[1]);
}
// Also confirm each registered name has a #[tauri::command] fn in commands.rs.
const commandsRs = readFileSync(join(root, 'src-tauri/src/commands.rs'), 'utf8');
const defined = new Set();
for (const m of commandsRs.matchAll(/#\[tauri::command\]\s*(?:pub(?:\(crate\))?\s+)?(?:async\s+)?fn\s+([a-z_0-9]+)/g)) {
  defined.add(m[1]);
}

const feMissing = [...fe].filter((n) => !be.has(n)).sort();
const beOrphan = [...be].filter((n) => !fe.has(n)).sort();
const beUndef = [...be].filter((n) => !defined.has(n)).sort();

console.log(`frontend commands: ${fe.size}`);
console.log(`registered commands: ${be.size}`);
console.log(`defined #[tauri::command] fns: ${defined.size}`);

let fail = false;
if (feMissing.length) {
  fail = true;
  console.log('MISSING ON BACKEND (frontend calls, shell does not register):');
  feMissing.forEach((n) => console.log(`  - ${n}`));
}
if (beOrphan.length) {
  fail = true;
  console.log('ORPHANS (shell registers, frontend never calls):');
  beOrphan.forEach((n) => console.log(`  - ${n}`));
}
if (beUndef.length) {
  fail = true;
  console.log('UNDEFINED (registered but no #[tauri::command] fn):');
  beUndef.forEach((n) => console.log(`  - ${n}`));
}

console.log(fail ? 'IPC CONTRACT: FAIL' : 'IPC CONTRACT: PASS');
process.exit(fail ? 1 : 0);
