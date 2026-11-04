#!/usr/bin/env node
/**
 * Phase 0 verification harness (docs/plans/go-to-rust-migration.md, task 0.3):
 * parse every VALID Osprey example with tree-sitter and assert ZERO ERROR/MISSING
 * nodes. Exits non-zero on any error node so it can gate CI.
 *
 * Valid examples = src/osprey/examples/**.osp (the *.ospo files under
 * failscompilation/ are deliberately broken and are excluded).
 */
const { execFileSync } = require('node:child_process');
const { readdirSync, statSync } = require('node:fs');
const { join, relative } = require('node:path');

const ROOT = join(__dirname, '..');
const EXAMPLES = join(ROOT, '..', '..', 'osprey', 'examples');
const TS = join(ROOT, 'node_modules', '.bin', 'tree-sitter');

function ospFiles(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) {
      if (entry === 'failscompilation') continue; // deliberately-broken cases
      out.push(...ospFiles(p));
    } else if (entry.endsWith('.osp')) {
      out.push(p);
    }
  }
  return out;
}

const files = ospFiles(EXAMPLES).sort();
let failed = 0;
for (const f of files) {
  const tree = execFileSync(TS, ['parse', f], { encoding: 'utf8' });
  const errors = (tree.match(/\((ERROR|MISSING)\b/g) || []).length;
  if (errors > 0) {
    failed++;
    console.error(`FAIL (${errors} error nodes): ${relative(EXAMPLES, f)}`);
  }
}

const ok = files.length - failed;
console.log(`\nparse-all: ${ok}/${files.length} valid examples parsed with 0 ERROR nodes`);
if (failed > 0) {
  console.error(`\n${failed} file(s) produced ERROR/MISSING nodes — grammar regression.`);
  process.exit(1);
}
