#!/usr/bin/env node
// Dead-code gate for the Rust workspace. [LINTS-DEADCODE-REACH]
//
// WHY THIS EXISTS. `dead_code = "deny"` (workspace Cargo.toml) catches every
// unused item that is private to a crate, and `unreachable_pub = "deny"` forces
// a `pub` item that nobody can name from outside down to `pub(crate)`, which
// puts it back under `dead_code`. One hole survives both: an item that IS
// reachable from its crate root and IS re-exported, but that no OTHER crate and
// no product code path ever names. rustc assumes such an item is a library's
// public API and stays silent, so it can rot in the tree forever.
//
// That hole is not theoretical. `osprey_debug::DebugBuild` was a two-state
// wrapper over `BuildKind` whose only callers were its own unit tests, and
// `DocComment::summary_only` was a constructor the real parser never used.
// Both compiled clean under the full lint set above.
//
// WHAT COUNTS AS DEAD. An item is dead when no PRODUCT line outside its own
// definition names it. A line is product code unless it sits in a test file
// (`tests/`, `*_tests.rs`, `test_support`/`testutil`) or inside a
// `#[cfg(test)]` block. Tests alone never keep an item alive: code whose sole
// purpose is to be tested is exactly the thing this gate removes.
//
// An item's "own definition" is its declaration block plus, for a type, every
// `impl` block for that type. Without that, a dead type looks alive because its
// own inherent methods mention it.
//
// WHAT IT DELIBERATELY MISSES. Items are tracked by bare NAME, so a dead item
// sharing a name with a live one anywhere in the workspace (`new`, `kind`,
// `path`) is held alive by its namesake, and macro-generated items are not seen
// at all. Both are FALSE NEGATIVES: the gate under-reports and never invents a
// finding, which is what lets it run with no allowlist. It is a floor on top of
// `make hawk`, not a replacement for reading the code.
//
// NO ALLOWLIST, deliberately. A gate you can turn off is not a gate. When this
// fails, the fix is to call the item or delete it, never to exempt it.

import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, extname, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");

// Directories that never hold hand-written product source.
const SKIP_DIRS = new Set([
  "target",
  "node_modules",
  ".git",
  "_site",
  "out",
  ".vscode-test",
  "coverage",
  "test-results",
]);

// Only these can NAME a Rust item. Rust is obvious; C is here because an
// `extern "C"` declaration's real user is the C definition it links to, and
// dropping `.c`/`.h` would condemn the whole runtime FFI surface.
//
// Prose and config are deliberately absent. Markdown cannot call a function, so
// counting a doc mention as a use lets any item stay alive by being written
// about — this gate caught its own header comment resurrecting the very example
// it describes.
const REFERENCE_EXTS = new Set([".rs", ".c", ".h"]);

const TYPE_KINDS = new Set(["struct", "enum", "trait", "union"]);

const DECL =
  /^(\s*)pub(?:\s*\(\s*(?:crate|self|super)\s*\))?\s+(?:(?:const|async|unsafe|extern\s+"[^"]*")\s+)*(fn|struct|enum|trait|type|const|static|union)\s+([A-Za-z_]\w*)/;
const IMPL = /^\s*impl\b[^{]*?\b([A-Za-z_]\w*)\s*(?:<[^{]*>)?\s*(?:where[^{]*)?\{/;
const VARIANT = /^\s{4,}([A-Z]\w*)\s*(?:[{(,=]|$)/;
const FIELD = /^\s{4,}pub\s+(?:\([^)]*\)\s*)?([a-z_]\w*)\s*:/;
const CFG_TEST = /^\s*#\[cfg\(test\)\]/;

const walk = (dir, out = []) => {
  for (const entry of readdirSync(dir)) {
    if (SKIP_DIRS.has(entry)) continue;
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) walk(path, out);
    else out.push(path);
  }
  return out;
};

const isTestFile = (path) =>
  path.includes("/tests/") ||
  path.includes("/test/") ||
  path.endsWith("_tests.rs") ||
  path.includes("test_support") ||
  path.includes("testutil") ||
  path.includes("/benchmarks/");

// The 1-based [start, end] of the brace block opening at `start`, or of a
// single `;`-terminated declaration when no brace follows.
const blockSpan = (lines, start) => {
  let depth = 0;
  let sawBrace = false;
  for (let j = start - 1; j < lines.length; j += 1) {
    const line = lines[j];
    depth += (line.match(/\{/g) ?? []).length - (line.match(/\}/g) ?? []).length;
    if (line.includes("{")) sawBrace = true;
    if (sawBrace && depth <= 0) return [start, j + 1];
    if (!sawBrace && line.trimEnd().endsWith(";")) return [start, j + 1];
  }
  return [start, lines.length];
};

// Every 1-based line number sitting inside a `#[cfg(test)]` block.
const testMask = (lines) => {
  const mask = new Set();
  let i = 0;
  while (i < lines.length) {
    if (CFG_TEST.test(lines[i])) {
      const [a, b] = blockSpan(lines, i + 1);
      for (let k = a; k <= b; k += 1) mask.add(k);
      i = b;
    } else i += 1;
  }
  return mask;
};

const files = new Map();
for (const path of walk(REPO_ROOT)) {
  if (!REFERENCE_EXTS.has(extname(path))) continue;
  try {
    files.set(path, readFileSync(path, "utf8"));
  } catch {
    /* unreadable file cannot name anything */
  }
}

const linesOf = new Map();
const maskOf = new Map();
for (const [path, text] of files) {
  const lines = text.split("\n");
  linesOf.set(path, lines);
  maskOf.set(path, path.endsWith(".rs") ? testMask(lines) : new Set());
}

const crateSrc = (path) =>
  path.startsWith(join(REPO_ROOT, "crates")) && path.endsWith(".rs") && !isTestFile(path);

// name -> { kind, path, line, spans: [[path, start, end]] }
const items = new Map();
const record = (name, kind, path, line, span) => {
  const found = items.get(name);
  if (found) found.spans.push(span);
  else items.set(name, { kind, path, line, spans: [span] });
};

for (const [path, text] of files) {
  if (!crateSrc(path)) continue;
  const lines = linesOf.get(path);
  const mask = maskOf.get(path);
  lines.forEach((line, index) => {
    const lineNo = index + 1;
    if (mask.has(lineNo)) return;
    const decl = DECL.exec(line);
    if (!decl) return;
    const [, , kind, name] = decl;
    const [a, b] = blockSpan(lines, lineNo);
    record(name, kind, path, lineNo, [path, a, b]);
    // Enum variants and public struct fields are members of the same block, and
    // rustc treats both as used the moment the enclosing type is `pub`.
    if (kind === "enum" || kind === "struct") {
      for (let j = a; j < b - 1; j += 1) {
        const member = kind === "enum" ? VARIANT.exec(lines[j]) : FIELD.exec(lines[j]);
        if (member && member[1] !== "Self") record(member[1], `${kind} member`, path, j + 1, [path, a, b]);
      }
    }
  });
}

// A type's inherent and trait `impl` blocks are part of its own definition.
for (const [path, text] of files) {
  if (!path.endsWith(".rs")) continue;
  const lines = linesOf.get(path);
  lines.forEach((line, index) => {
    const impl = IMPL.exec(line);
    if (!impl) return;
    const found = items.get(impl[1]);
    if (!found || !TYPE_KINDS.has(found.kind)) return;
    const [a, b] = blockSpan(lines, index + 1);
    found.spans.push([path, a, b]);
  });
}

const insideOwnSpans = (item, path, lineNo) =>
  item.spans.some(([p, a, b]) => p === path && lineNo >= a && lineNo <= b);

const hasProductReference = (name, item) => {
  const word = new RegExp(`\\b${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\b`);
  for (const [path, text] of files) {
    if (!text.includes(name)) continue;
    const testFile = isTestFile(path);
    const mask = maskOf.get(path);
    const lines = linesOf.get(path);
    for (let i = 0; i < lines.length; i += 1) {
      const lineNo = i + 1;
      if (testFile || mask.has(lineNo)) continue;
      if (insideOwnSpans(item, path, lineNo)) continue;
      if (word.test(lines[i])) return true;
    }
  }
  return false;
};

const dead = [];
for (const [name, item] of items) {
  if (!hasProductReference(name, item)) dead.push({ name, ...item });
}
dead.sort((a, b) => a.path.localeCompare(b.path) || a.line - b.line);

if (dead.length === 0) {
  console.log(`==> Dead-code gate: ${items.size} public items, all reachable from product code.`);
  process.exit(0);
}

console.error(
  `FAIL: ${dead.length} public item(s) are never named by product code.\n` +
    "Each is reachable only from its own definition or its own tests, so no\n" +
    "rustc lint can see it. Call it from product code, or delete it.\n",
);
for (const item of dead) {
  console.error(`  ${item.path.slice(REPO_ROOT.length + 1)}:${item.line}  ${item.kind} ${item.name}`);
}
process.exit(1);
