// The TypeScript parser conformance check ([FLAVOR-TS-CST]): run the pinned
// `oxc_parser` and the pinned `tsc` 7 over TypeScript's own parser
// conformance corpus, and record how often they agree that a file is
// syntactically valid. It also proves the TypeScript-flavor frontend never
// panics on the corpus. Run from the repository root:
//   make typescript-conformance
// Writes target/ts-conformance/agreement.json and prints a markdown table.
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../..");
const out = path.join(root, "target/ts-conformance");
/** The TypeScript commit `tsc` 7.0.2 (typescript-go) pins as its test corpus. */
const CORPUS_COMMIT = "4d4f005c8541e0255a9d8791205fdce326e462bc";
const corpus = path.join(out, "TypeScript");
const SUITES = { parser: "tests/cases/conformance/parser", compiler: "tests/cases/compiler" };
const tsc = path.join(here, "node_modules/.bin/tsc");
const oxc = path.join(root, "target/release/examples/ts_conformance");
/**
 * The second parser the conformance check compares: `tsrs_parser`, the published Rust port
 * of the TypeScript 7 parser. It is built here, outside the workspace, so the
 * compiler never depends on it.
 */
const probe = path.join(out, "tsrs-probe");
const PROBE_MANIFEST = `[package]
name = "tsrs-probe"
version = "0.0.0"
edition = "2021"
publish = false

[workspace]

[dependencies]
tsrs_parser = "=7.1.0-beta.tsrs1"
tsrs_ast = "=7.1.0-beta.tsrs1"
tsrs_core = "=7.1.0-beta.tsrs1"
`;
const PROBE_MAIN = `//! The conformance check's second parser: \`tsrs_parser\`, a Rust port of
//! the TypeScript 7 parser. Prints \`path<TAB>accept|reject\` for each file
//! named on stdin.
use std::io::{BufRead, Write};

fn main() -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in std::io::stdin().lock().lines() {
        let path = line?;
        let Ok(text) = std::fs::read_to_string(path.trim()) else {
            writeln!(out, "{}\\tunreadable", path.trim())?;
            continue;
        };
        let options = tsrs_ast::SourceFileParseOptions {
            file_name: path.trim().to_owned(),
            path: tsrs_core::tspath::Path::from(path.trim()),
            external_module_indicator_options: Default::default(),
        };
        let file = tsrs_parser::parse_source_file(options, &text, tsrs_core::ScriptKind::TS);
        let verdict = if file.diagnostics().is_empty() { "accept" } else { "reject" };
        writeln!(out, "{}\\t{verdict}", path.trim())?;
    }
    Ok(())
}
`;

function buildProbe() {
  mkdirSync(path.join(probe, "src"), { recursive: true });
  writeFileSync(path.join(probe, "Cargo.toml"), PROBE_MANIFEST);
  writeFileSync(path.join(probe, "src/main.rs"), PROBE_MAIN);
  const target = path.join(probe, "target");
  execFileSync("cargo", ["build", "--release", "--quiet", "--target-dir", target], { cwd: probe, stdio: "inherit" });
  return path.join(target, "release/tsrs-probe");
}

/** `tsrs_parser`'s verdict for each file. */
function tsrsVerdicts(binary, files) {
  const result = spawnSync(binary, [], { input: `${files.join("\n")}\n`, encoding: "utf8", maxBuffer: 1 << 28 });
  if (result.status !== 0) throw new Error(`tsrs-probe failed: ${result.stderr}`);
  return new Map(result.stdout.trim().split("\n").map((line) => line.split("\t")));
}

function fetchCorpus() {
  if (existsSync(path.join(corpus, SUITES.parser))) return;
  mkdirSync(corpus, { recursive: true });
  const git = (...args) => execFileSync("git", args, { cwd: corpus, stdio: "inherit" });
  git("init", "-q");
  git("remote", "add", "origin", "https://github.com/microsoft/TypeScript.git");
  git("config", "core.sparseCheckout", "true");
  writeFileSync(path.join(corpus, ".git/info/sparse-checkout"), `${SUITES.parser}/\n${SUITES.compiler}/\n`);
  git("fetch", "-q", "--depth", "1", "origin", CORPUS_COMMIT);
  git("checkout", "-q", "FETCH_HEAD");
}

/** Every `.ts` case under `directory`, declaration files excluded. */
function cases(directory) {
  return readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".ts") && !entry.name.endsWith(".d.ts"))
    .map((entry) => path.join(entry.parentPath, entry.name))
    .sort();
}

/** `oxc_parser`'s verdict and the frontend's outcome for each file. */
function oxcVerdicts(files) {
  const result = spawnSync(oxc, [], { input: `${files.join("\n")}\n`, encoding: "utf8", maxBuffer: 1 << 28 });
  if (result.status !== 0) throw new Error(`ts_conformance failed: ${result.stderr}`);
  return new Map(
    result.stdout
      .trim()
      .split("\n")
      .map((line) => line.split("\t"))
      .map(([file, parser, frontend]) => [file, { parser, frontend }]),
  );
}

/**
 * The files `tsc` rejects for syntax: any TS1xxx diagnostic, the codes of its
 * parser and of the grammar checks its checker runs (`await` as an
 * identifier, a misplaced numeric separator), which ECMAScript calls early
 * errors and `oxc_parser` reports itself. Type errors (TS2xxx and above) are
 * not syntax and are ignored; `--noLib` and `--noResolve` keep every file
 * standalone.
 */
function tscRejects(files, oxcRejected) {
  // `tsc` checks a program only when no file of it has a parse error, and its
  // grammar checks run in the checker. So every file `oxc` rejects but the
  // parse pass accepted is checked again alone, which runs those checks.
  const parsed = tscPass(files, "parse");
  const recheck = files.filter((file) => oxcRejected.has(file) && !parsed.has(file));
  const checked = recheck.filter((file) => tscPass([file], "single").size > 0);
  return new Set([...parsed, ...checked]);
}

function tscPass(files, name) {
  const config = path.join(out, `tsconfig.${name}.json`);
  const compilerOptions = { noEmit: true, noResolve: true, noLib: true, types: [], target: "esnext", module: "esnext" };
  writeFileSync(config, JSON.stringify({ compilerOptions, files }, null, 1));
  const result = spawnSync(tsc, ["--project", config, "--pretty", "false"], { encoding: "utf8", maxBuffer: 1 << 28 });
  const rejected = new Set();
  for (const line of result.stdout.split("\n")) {
    const match = /^(.+?)\(\d+,\d+\): error TS1\d{3}:/.exec(line);
    if (match) rejected.add(path.resolve(root, match[1]));
  }
  return rejected;
}

function agreement(files, verdicts, rejected, tsrs) {
  const table = { both_accept: 0, both_reject: 0, only_oxc_rejects: 0, only_tsc_rejects: 0, tsrs_agrees: 0, not_utf8: [], panics: [] };
  const read = [];
  for (const file of files) {
    const { parser, frontend } = verdicts.get(file) ?? {};
    // Osprey sources are UTF-8; a UTF-16 or Latin-1 case is reported, not compared.
    if (parser === "unreadable") {
      table.not_utf8.push(path.relative(corpus, file));
      continue;
    }
    read.push(file);
    const oxcRejects = parser !== "accept";
    const tscRejected = rejected.has(file);
    const key = oxcRejects === tscRejected ? (oxcRejects ? "both_reject" : "both_accept") : oxcRejects ? "only_oxc_rejects" : "only_tsc_rejects";
    table[key] += 1;
    if ((tsrs.get(file) === "reject") === tscRejected) table.tsrs_agrees += 1;
    if (frontend !== "ok") table.panics.push(path.relative(corpus, file));
  }
  table.files = files.length;
  table.agreement = (table.both_accept + table.both_reject) / read.length;
  table.tsrs_agreement = table.tsrs_agrees / read.length;
  return table;
}

mkdirSync(out, { recursive: true });
fetchCorpus();
const tsrsBinary = buildProbe();
const results = { spec: "[FLAVOR-TS-CST]", corpusCommit: CORPUS_COMMIT, tsc: execFileSync(tsc, ["--version"], { encoding: "utf8" }).trim(), oxcParser: "0.158.0", tsrsParser: "7.1.0-beta.tsrs1", suites: {} };
for (const [suite, directory] of Object.entries(SUITES)) {
  const files = cases(path.join(corpus, directory));
  const verdicts = oxcVerdicts(files);
  const oxcRejected = new Set(files.filter((file) => verdicts.get(file)?.parser === "reject"));
  results.suites[suite] = agreement(files, verdicts, tscRejects(files, oxcRejected), tsrsVerdicts(tsrsBinary, files));
}
writeFileSync(path.join(out, "agreement.json"), `${JSON.stringify(results, null, 2)}\n`);
console.log("| Suite | Files | Not UTF-8 | Both accept | Both reject | Only oxc rejects | Only tsc rejects | oxc agreement | tsrs agreement | Frontend panics |");
console.log("| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
for (const [suite, t] of Object.entries(results.suites)) {
  console.log(`| ${suite} | ${t.files} | ${t.not_utf8.length} | ${t.both_accept} | ${t.both_reject} | ${t.only_oxc_rejects} | ${t.only_tsc_rejects} | ${(t.agreement * 100).toFixed(1)}% | ${(t.tsrs_agreement * 100).toFixed(1)}% | ${t.panics.length} |`);
}
console.log(`results: ${path.relative(root, path.join(out, "agreement.json"))}`);
const panicked = Object.values(results.suites).some((t) => t.panics.length > 0);
process.exit(panicked ? 1 : 0);
