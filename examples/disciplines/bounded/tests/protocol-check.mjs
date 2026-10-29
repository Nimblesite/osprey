#!/usr/bin/env node
// Drive the built `bounded` checker over the protocol fixtures and hold every
// answer to its golden byte for byte ([TYPES-DISCIPLINE-PROTOCOL], spec 0050).
//
// usage: node protocol-check.mjs <checker> <compiler> <cache-dir> <fixtures>
//
// Each `*.ospml` fixture becomes one request whose `module.source` is that
// file's text; each `*.raw` fixture is fed as the request line itself. The
// expected response is the sibling `<name>.expected`.

import { readFileSync, readdirSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { join, resolve } from "node:path";

const [checker, compiler, cache, fixtures] = process.argv.slice(2);
if (!checker || !compiler || !cache || !fixtures) {
  console.error("usage: protocol-check.mjs <checker> <compiler> <cache-dir> <fixtures>");
  process.exit(2);
}

const request = (name, source) =>
  JSON.stringify({
    version: 2,
    module: { path: join(resolve(fixtures), `${name}.ospml`), flavor: "ml", source },
    imports: [],
    options: { project_root: resolve(fixtures), cache_dir: resolve(cache), compiler: resolve(compiler) },
  }) + "\n";

const failures = [];
const names = readdirSync(fixtures)
  .filter((file) => file.endsWith(".ospml"))
  .map((file) => file.slice(0, -".ospml".length))
  .sort();

const run = (name, line) =>
  execFileSync(checker, { input: line, encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] });

for (const name of names) {
  const line = request(name, readFileSync(join(fixtures, `${name}.ospml`), "utf8"));
  const expected = readFileSync(join(fixtures, `${name}.expected`), "utf8");
  const actual = run(name, line);
  if (actual !== expected) {
    failures.push(`${name}: response differs\n--- expected\n${expected}\n--- actual\n${actual}`);
  } else {
    console.log(`ok ${name}`);
  }
}

const raws = readdirSync(fixtures).filter((file) => file.endsWith(".raw")).sort();

for (const name of raws) {
  const stem = name.slice(0, -".raw".length);
  const expected = readFileSync(join(fixtures, `${stem}.expected`), "utf8");
  const actual = run(stem, readFileSync(join(fixtures, name), "utf8"));
  if (actual !== expected) {
    failures.push(`${stem}: response differs\n--- expected\n${expected}\n--- actual\n${actual}`);
  } else {
    console.log(`ok ${stem}`);
  }
}

if (failures.length > 0) {
  for (const failure of failures) console.error(failure);
  process.exit(1);
}
console.log(`protocol: ${names.length + raws.length} responses match their goldens`);
