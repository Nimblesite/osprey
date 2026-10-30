#!/usr/bin/env node
// Regression contract for scripts/proof-export.sh ([PROOF-EXPORT]).
// `lake` and `lean` are mocked on a private PATH so theorem selection, flag
// validation, toolchain pinning and --check semantics are asserted without a
// real Lean build. Run: node --test scripts/test-proof-export.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const SCRIPT = path.join(REPO, 'scripts/proof-export.sh');
const TOOLS_DIR = path.join(REPO, 'benchmarks/proof/tools');
const RUN_TIMEOUT_MS = 60_000;
const EXPORTER_REV = 'cfefafacc917b2d14c5f9bb092ed337f71317a4a';
const CHECKER_REV = 'aa03950243dc18750c1546602ab054609ea8d57d';
const PINNED_LEAN = '4.25.0';
const HEADER_ERROR = /3\.2\.0/;

const RECORDED = Object.freeze({
  id_imp: path.join(REPO, 'examples/proof/kernel/exports/id_imp.ndjson'),
  and_swap: path.join(REPO, 'examples/proof/kernel/exports/and_swap.ndjson'),
  'Nat.add_comm': path.join(REPO, 'examples/proof/kernel/exports/nat_add_comm.ndjson'),
});

const VALID_EXPORT = [
  '{"meta":{"exporter":"lean4export","format":{"version":"3.1.0"}}}',
  '{"decl":{"name":"id_imp"}}',
  '',
].join('\n');

const MOCK_LAKE = `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\t%s\\n' "$PWD" "$*" >> "$PROOF_EXPORT_MOCK_LAKE_LOG"
[ "$1" = "exe" ] || exit 0
cat "$PROOF_EXPORT_MOCK_EXPORT"
`;

const MOCK_LEAN = `#!/usr/bin/env bash
set -euo pipefail
printf '%s\\t%s\\n' "$PWD" "$*" >> "$PROOF_EXPORT_MOCK_LEAN_LOG"
printf 'Lean (version %s, mock-commit, Release)\\n' "\${PROOF_EXPORT_MOCK_LEAN_VERSION:-${PINNED_LEAN}}"
`;

const writeExecutable = (file, body) => fs.writeFileSync(file, body, { mode: 0o755 });

const normalize = (text) => `${text.replace(/\n+$/, '')}\n`;

const createSandbox = (options = {}) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'proof-export-'));
  const bin = path.join(root, 'bin');
  fs.mkdirSync(bin, { recursive: true });
  writeExecutable(path.join(bin, 'lake'), MOCK_LAKE);
  writeExecutable(path.join(bin, 'lean'), MOCK_LEAN);
  const exportFile = path.join(root, 'export.ndjson');
  fs.writeFileSync(exportFile, options.exportText ?? VALID_EXPORT);
  return { root, bin, exportFile, options };
};

const MOCK_PATH = '/usr/bin:/bin:/usr/sbin:/sbin';

const sandboxEnv = (sandbox) => ({
  PATH: `${sandbox.bin}:${MOCK_PATH}`,
  HOME: path.join(sandbox.root, 'home'),
  PROOF_EXPORT_MOCK_LAKE_LOG: path.join(sandbox.root, 'lake.log'),
  PROOF_EXPORT_MOCK_LEAN_LOG: path.join(sandbox.root, 'lean.log'),
  PROOF_EXPORT_MOCK_EXPORT: sandbox.exportFile,
  PROOF_EXPORT_MOCK_LEAN_VERSION: sandbox.options.leanVersion ?? PINNED_LEAN,
});

const invoke = (sandbox, args) =>
  spawnSync('/bin/bash', [SCRIPT, ...args], {
    cwd: REPO,
    encoding: 'utf8',
    timeout: RUN_TIMEOUT_MS,
    env: sandboxEnv(sandbox),
  });

const readCalls = (file) =>
  fs.existsSync(file)
    ? fs
        .readFileSync(file, 'utf8')
        .split('\n')
        .filter(Boolean)
        .map((line) => {
          const [cwd, argv] = line.split('\t');
          return { cwd, argv };
        })
    : [];

// Runs one sandboxed invocation and returns its result plus the mock call log.
const runCase = (options, args) => {
  const sandbox = createSandbox(options);
  try {
    const result = invoke(sandbox, args);
    const lake = readCalls(path.join(sandbox.root, 'lake.log')).map((call) => call.argv);
    const lean = readCalls(path.join(sandbox.root, 'lean.log'));
    return { result, lake, lean, stdout: result.stdout, stderr: result.stderr };
  } finally {
    fs.rmSync(sandbox.root, { recursive: true, force: true });
  }
};

const recordedText = (name) => fs.readFileSync(RECORDED[name], 'utf8');

const assertLake = (lake, expected) => assert.deepEqual(lake, expected);

const assertRejected = (run, label) => {
  assert.notEqual(run.result.status, 0, `${label} must fail`);
  assert.equal(run.stdout, '', `${label} must not print an export`);
  assert.deepEqual(run.lake, [], `${label} must be rejected before any lake build`);
  assert.match(run.stderr, /proof-export\.sh:/, `${label} must explain the rejection`);
};

test('every selectable theorem has a recorded fixture', () => {
  for (const [name, file] of Object.entries(RECORDED)) {
    assert.ok(fs.existsSync(file), `missing recorded export for ${name}: ${file}`);
  }
});

test('default run exports IdImp and prints it to stdout', () => {
  const run = runCase({}, []);
  assert.equal(run.result.status, 0, run.stderr);
  assert.equal(run.stdout, normalize(VALID_EXPORT));
  assertLake(run.lake, ['build lean4export lean4checker IdImp', 'exe lean4export IdImp']);
  assert.match(run.stderr, new RegExp(EXPORTER_REV));
  assert.match(run.stderr, new RegExp(CHECKER_REV));
});

test('the pinned lean version is verified from the tools directory', () => {
  const run = runCase({}, []);
  assert.equal(run.result.status, 0, run.stderr);
  const versionCall = run.lean.find((call) => call.argv === '--version');
  assert.ok(versionCall, 'expected `lean --version` to be executed');
  assert.equal(fs.realpathSync(versionCall.cwd), fs.realpathSync(TOOLS_DIR));
});

test('--theorem and_swap builds AndSwap and exports AndSwap', () => {
  const run = runCase({}, ['--theorem', 'and_swap']);
  assert.equal(run.result.status, 0, run.stderr);
  assert.equal(run.stdout, normalize(VALID_EXPORT));
  assertLake(run.lake, ['build lean4export lean4checker AndSwap', 'exe lean4export AndSwap']);
});

test('--theorem Nat.add_comm builds only the pinned tools', () => {
  const run = runCase({}, ['--theorem', 'Nat.add_comm']);
  assert.equal(run.result.status, 0, run.stderr);
  assert.equal(run.stdout, normalize(VALID_EXPORT));
  assertLake(run.lake, [
    'build lean4export lean4checker',
    'exe lean4export Init.Data.Nat.Basic -- Nat.add_comm',
  ]);
});

test('--theorem and --check are accepted in any argument order', () => {
  const text = recordedText('and_swap');
  const first = runCase({ exportText: text }, ['--check', '--theorem', 'and_swap']);
  const second = runCase({ exportText: text }, ['--theorem', 'and_swap', '--check']);
  for (const run of [first, second]) {
    assert.equal(run.result.status, 0, run.stderr);
    assert.equal(run.stdout, '', '--check must print nothing to stdout');
  }
  assertLake(second.lake, first.lake);
  assertLake(first.lake, ['build lean4export lean4checker AndSwap', 'exe lean4export AndSwap']);
});

test('--check accepts a byte-identical and_swap fixture', () => {
  const run = runCase({ exportText: recordedText('and_swap') }, ['--check', '--theorem', 'and_swap']);
  assert.equal(run.result.status, 0, run.stderr);
  assert.equal(run.stdout, '');
  assert.match(run.stderr, /byte-identical/);
});

test('--check compares Nat.add_comm against its own recorded fixture', () => {
  const text = recordedText('Nat.add_comm');
  const run = runCase({ exportText: text }, ['--check', '--theorem', 'Nat.add_comm']);
  assert.equal(run.result.status, 0, run.stderr);
  assert.equal(run.stdout, '');
  assert.match(run.stderr, /byte-identical/);
});

test('--check fails when the export differs from the fixture', () => {
  const drifted = `${recordedText('id_imp')}\n{"decl":{"name":"drift"}}`;
  const run = runCase({ exportText: drifted }, ['--check', '--theorem', 'id_imp']);
  assert.notEqual(run.result.status, 0, 'a mismatch must fail');
  assert.equal(run.stdout, '');
  assert.match(run.stderr, /STOP — the export differs from/);
});

test('--check preserves exact trailing newline bytes', () => {
  for (const text of [recordedText('id_imp').trimEnd(), `${recordedText('id_imp')}\n`]) {
    const run = runCase({ exportText: text }, ['--check']);
    assert.notEqual(run.result.status, 0, 'a changed trailing newline must fail');
    assert.equal(run.stdout, '');
    assert.match(run.stderr, /STOP — the export differs from/);
  }
});

test('legacy --check selects the recorded IdImp fixture', () => {
  const run = runCase({ exportText: recordedText('id_imp') }, ['--check']);
  assert.equal(run.result.status, 0, run.stderr);
  assert.equal(run.stdout, '');
  assertLake(run.lake, ['build lean4export lean4checker IdImp', 'exe lean4export IdImp']);
});

test('an unexpected NDJSON format version stops the run', () => {
  const drifted = '{"meta":{"exporter":"lean4export","format":{"version":"3.2.0"}}}\n';
  const run = runCase({ exportText: drifted }, []);
  assert.notEqual(run.result.status, 0, 'a format drift must fail');
  assert.equal(run.stdout, '', 'a rejected export must never reach stdout');
  assert.match(run.stderr, /STOP/);
  assert.match(run.stderr, HEADER_ERROR);
});

test('a drifted lean toolchain stops the run before exporting', () => {
  const run = runCase({ leanVersion: '4.24.0' }, []);
  assert.notEqual(run.result.status, 0, 'a toolchain drift must fail');
  assert.equal(run.stdout, '');
  assert.match(run.stderr, /STOP/);
  assert.match(run.stderr, new RegExp(PINNED_LEAN));
  assert.ok(!run.lake.some((argv) => argv.startsWith('exe ')), 'no export may run');
});

test('rejects an unknown flag', () => {
  assertRejected(runCase({}, ['--bogus']), '--bogus');
});

test('rejects an unknown theorem name', () => {
  assertRejected(runCase({}, ['--theorem', 'nat_add_comm']), '--theorem nat_add_comm');
});

test('rejects --theorem without a value', () => {
  assertRejected(runCase({}, ['--theorem']), '--theorem');
});

// [TYPES-DISCIPLINE-REFERENCE] round trips end in `| diff - <golden>`, and make
// reads only the last command's status. A compiler that prints the whole golden
// and then dies — a runtime crash after output — would look green. Each real
// recipe line is replayed through a miniature Make target that includes the same
// config.mk, with a fake compiler that prints what the line reads and exits with
// a chosen status.
const PROOF_MK = path.join(REPO, 'scripts/make/proof.mk');
const CONFIG_MK = path.join(REPO, 'scripts/make/config.mk');
const GOLDEN_PIPE = /\| diff - (\S*examples\/disciplines\/\S+)$/;
const CRASH_STATUS = 23;

const goldenPipelines = () =>
  fs
    .readFileSync(PROOF_MK, 'utf8')
    .split('\n')
    .filter((line) => line.includes('$(BIN)') && GOLDEN_PIPE.test(line.trim()))
    .map((line) => line.trim());

// Run the fixture's real recipe lines against a fake compiler. `setup(root)`
// returns the fake's script body, so a fixture can stage whatever its recipe
// reads before make starts.
const runMakeFixture = (lines, setup) => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'discipline-pipe-'));
  try {
    writeExecutable(path.join(root, 'fake-osprey'), `#!/bin/bash\n${setup(root)}\n`);
    fs.writeFileSync(
      path.join(root, 'fixture.mk'),
      `include ${CONFIG_MK}\n\n.PHONY: check\ncheck:\n${lines.map((line) => `\t${line}`).join('\n')}\n`,
    );
    return spawnSync('make', ['-f', 'fixture.mk', 'check', 'BIN=fake-osprey'], { cwd: root, encoding: 'utf8' });
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
};

// The golden copied under the recipe's own relative path, so `diff - <golden>`
// compares against the fixture compiler's output.
const goldenRun = (line, status) =>
  runMakeFixture([line], (root) => {
    const golden = line.match(GOLDEN_PIPE)[1];
    const copy = path.join(root, golden);
    fs.mkdirSync(path.dirname(copy), { recursive: true });
    fs.copyFileSync(path.join(REPO, golden), copy);
    return `cat '${copy}'\nexit ${status}`;
  });

test('a discipline round trip fails when the compiler crashes after printing its golden', () => {
  const lines = goldenPipelines();
  assert.ok(lines.length >= 5, `expected at least five GC/ARC/marker-free golden pipelines:\n${lines.join('\n')}`);
  for (const line of lines) {
    const settled = goldenRun(line, 0);
    assert.equal(settled.status, 0, `the same fixture must pass a clean compiler: ${line}\n${settled.stdout}${settled.stderr}`);
    const crashed = goldenRun(line, CRASH_STATUS);
    assert.notEqual(crashed.status, 0, `make accepted a compiler that exited ${CRASH_STATUS} after printing its golden: ${line}`);
    assert.match(crashed.stderr, /Error 23/, `make must fail on the compiler's status, not on a broken fixture: ${line}\n${crashed.stdout}${crashed.stderr}`);
  }
});

// The bounded checker suite is run into a file, and its recipe then ends in
// `tail -2` plus `grep -q "failed=0"`: the runner's own status was the one thing
// no command read, so a runner that printed a clean summary and then crashed
// still looked green.
const SUITE_RUN = /\$\(BIN\) test .*bounded-suite\.txt/;
const SUITE_SUMMARY = '# tests=1 passed=1 failed=0 skipped=0';

const suiteRecipe = () => {
  const lines = fs.readFileSync(PROOF_MK, 'utf8').split('\n').map((line) => line.trim());
  const at = lines.findIndex((line) => SUITE_RUN.test(line));
  assert.notEqual(at, -1, 'the bounded checker suite recipe moved; re-point this guard');
  const pair = lines.slice(at, at + 2);
  assert.ok(pair[1].includes('bounded-suite.txt'), `the summary assertion must follow the run: ${pair.join(' / ')}`);
  return pair;
};

const suiteRun = (status) =>
  runMakeFixture(suiteRecipe(), (root) => {
    fs.mkdirSync(path.join(root, 'target/disciplines'), { recursive: true });
    return `printf '%s\n' '${SUITE_SUMMARY}'\nexit ${status}`;
  });

test('the bounded checker suite fails when the runner crashes after printing its summary', () => {
  const settled = suiteRun(0);
  assert.equal(settled.status, 0, `the same fixture must pass a clean runner:\n${settled.stdout}${settled.stderr}`);
  const crashed = suiteRun(CRASH_STATUS);
  assert.notEqual(crashed.status, 0, `make accepted a runner that printed a clean summary then exited ${CRASH_STATUS}`);
  assert.match(crashed.stderr, /Error 23/, `make must fail on the runner's status:\n${crashed.stdout}${crashed.stderr}`);
});
