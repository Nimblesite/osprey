#!/usr/bin/env node
// OPTIONAL compatibility lint ([FLAVOR-TS-LINT]): checks that every
// TypeScript-flavor source in the repository is also valid TypeScript under
// the pinned `tsc`. The Osprey compiler does not use TypeScript tooling and
// does not need this check; it is one lint among others, like a formatter.
//
//   cd scripts/typescript && npm ci && node lint.mjs     (needs `make build`)
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const osprey = process.env.OSPREY ?? path.join(root, "target/debug/osprey");
const tsc = path.join(here, "node_modules/.bin/tsc");
const out = path.join(root, "target/typescript-lint");
const packages = path.join(root, "src/osprey/packages");
const SKIPPED = new Set(["node_modules", "target", ".git", ".osprey"]);

/** Every `osprey.toml` under `dir` that lists `typescript_sources`. */
function manifests(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const found = path.join(dir, entry.name);
    if (entry.isDirectory()) return SKIPPED.has(entry.name) ? [] : manifests(found);
    return entry.name === "osprey.toml" && /^typescript_sources = /m.test(readFileSync(found, "utf8")) ? [found] : [];
  });
}

/** A project's own listed sources; a package's sources are linted with that package. */
function ownSources(manifest) {
  const listed = /^typescript_sources = \[(.*)\]$/m.exec(readFileSync(manifest, "utf8"))?.[1] ?? "";
  return [...listed.matchAll(/"([^"]+)"/g)]
    .map((match) => match[1])
    .filter((source) => !source.startsWith(".."))
    .map((source) => path.join(path.dirname(manifest), source));
}

// ---- Builtin declarations from `osprey --hover` ----------------------------

const SPELLINGS = { float: "number", bool: "boolean", Unit: "void", any: "unknown", printable: "unknown" };

/** Translate one Osprey type (`List<t0>`, `(int) -> bool`, `a | b`) to TypeScript. */
function typeOf(text, generics) {
  const source = text.trim();
  const union = split(source, "|");
  if (union.length > 1) return union.map((part) => typeOf(part, generics)).join(" | ");
  const arrow = topLevelArrow(source);
  if (arrow >= 0) {
    const params = split(source.slice(1, source.lastIndexOf(")", arrow)), ",").filter((part) => part.trim() !== "");
    const named = params.map((param, index) => `a${index}: ${typeOf(param, generics)}`);
    return `(${named.join(", ")}) => ${typeOf(source.slice(arrow + 2), generics)}`;
  }
  const open = source.indexOf("<");
  const head = open < 0 ? source : source.slice(0, open);
  if (/^t\d+$/.test(head)) {
    generics.add(head.toUpperCase());
    return head.toUpperCase();
  }
  const name = SPELLINGS[head] ?? head;
  if (open < 0) return name;
  const args = split(source.slice(open + 1, source.lastIndexOf(">")), ",").map((arg) => typeOf(arg, generics));
  return `${name}<${args.join(", ")}>`;
}

/** Split at `separator` outside brackets. */
function split(text, separator) {
  const parts = [];
  let depth = 0;
  let start = 0;
  for (let index = 0; index < text.length; index += 1) {
    const ch = text[index];
    if (ch === "<" || ch === "(") depth += 1;
    else if (ch === ">" && text[index - 1] !== "-") depth -= 1;
    else if (ch === ")") depth -= 1;
    else if (ch === separator && depth === 0) {
      parts.push(text.slice(start, index));
      start = index + 1;
    }
  }
  parts.push(text.slice(start));
  return parts;
}

function topLevelArrow(text) {
  if (!text.startsWith("(")) return -1;
  let depth = 0;
  for (let index = 0; index < text.length; index += 1) {
    if (text[index] === "(") depth += 1;
    if (text[index] === ")") depth -= 1;
    if (depth === 0) return text.startsWith(" -> ", index + 1) ? index + 2 : -1;
  }
  return -1;
}

/** `name(a: T, b: U) -> R` as a `declare function`, or nothing for a non-builtin. */
function declaration(name) {
  const hover = spawnSync(osprey, ["--hover", name], { encoding: "utf8" }).stdout ?? "";
  const signature = hover.split("\n").find((line) => line.startsWith(`${name}(`));
  if (!signature) return undefined;
  const arrow = signature.lastIndexOf(") -> ");
  const generics = new Set();
  const params = split(signature.slice(name.length + 1, arrow), ",")
    .filter((param) => param.trim() !== "")
    .map((param, index) => {
      const colon = param.indexOf(":");
      return `a${index}: ${typeOf(param.slice(colon + 1), generics)}`;
    });
  const result = typeOf(signature.slice(arrow + 5), generics);
  const list = generics.size === 0 ? "" : `<${[...generics].sort().join(", ")}>`;
  return `declare function ${name}${list}(${params.join(", ")}): ${result};`;
}

/** Declarations for every name the sources call that the compiler knows as a builtin. */
function builtinDeclarations(files, core) {
  const called = new Set();
  for (const file of files) {
    for (const match of readFileSync(file, "utf8").matchAll(/(?<![.\w])([a-z]\w*)\s*\(/g)) called.add(match[1]);
  }
  return [...called]
    .sort()
    .filter((name) => !new RegExp(`declare (function|const) ${name}\\b`).test(core))
    .map(declaration)
    .filter(Boolean)
    .join("\n");
}

// ---- The check ------------------------------------------------------------

/** The declaration `tsc` reads for each ported package a program may import. */
function packageDeclarations() {
  const declared = readdirSync(packages)
    .map((name) => [name, path.join(packages, name, "types", `${name}.d.ts`)])
    .filter(([, declaration]) => existsSync(declaration));
  return Object.fromEntries(declared.map(([name, declaration]) => [name, [declaration]]));
}

function lint(name, lib, files) {
  const directory = path.join(out, name);
  mkdirSync(directory, { recursive: true });
  const project = path.join(directory, "tsconfig.json");
  const compilerOptions = {
    noLib: true,
    strict: true,
    noImplicitAny: false,
    noEmit: true,
    target: "esnext",
    module: "esnext",
    moduleResolution: "bundler",
    types: [],
    paths: packageDeclarations(),
  };
  writeFileSync(project, `${JSON.stringify({ compilerOptions, files: [lib, ...files] }, null, 2)}\n`);
  return spawnSync(tsc, ["--project", project, "--pretty", "false"], { encoding: "utf8" });
}

/** The lint has teeth: a program `tsc` must reject is rejected. */
function rejectsBadProgram(core) {
  const bad = path.join(out, "rejected.ts");
  writeFileSync(bad, 'const count: int = "three";\nconst now = new Date();\nconst size = "text".length;\n');
  const lib = path.join(out, "rejected.d.ts");
  writeFileSync(lib, core);
  const result = lint("rejected", lib, [bad]);
  const missing = ["TS2322", "TS2304", "TS2339"].filter((code) => !result.stdout.includes(code));
  if (result.status === 0 || missing.length > 0) {
    throw new Error(`the lint accepted a program it must reject (missing ${missing.join(", ")}):\n${result.stdout}`);
  }
}

mkdirSync(out, { recursive: true });
const core = readFileSync(path.join(here, "osprey.d.ts"), "utf8");
rejectsBadProgram(core);
let failed = 0;
let checked = 0;
for (const manifest of manifests(path.join(root, "src"))) {
  const files = ownSources(manifest);
  if (files.length === 0) continue;
  const where = path.relative(root, path.dirname(manifest));
  const name = where.replaceAll(path.sep, "__");
  const lib = path.join(out, `${name}.d.ts`);
  writeFileSync(lib, `${core}${builtinDeclarations(files, core)}\n`);
  const result = lint(name, lib, files);
  checked += files.length;
  if (result.status === 0) {
    console.log(`ok ${where}`);
  } else {
    failed += 1;
    console.error(`FAILED ${where}\n${result.stdout}${result.stderr}`);
  }
}
if (checked === 0) throw new Error("no TypeScript-flavor source was found to lint");
const version = execFileSync(tsc, ["--version"], { encoding: "utf8" }).trim();
console.log(`${checked} TypeScript-flavor sources linted by tsc ${version}, ${failed} failed`);
process.exit(failed === 0 ? 0 : 1);
