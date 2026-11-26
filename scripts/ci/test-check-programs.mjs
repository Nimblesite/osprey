// [CI-PROGRAMS-COMPILE] Exercise project discovery and lowering in a real git checkout.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { parseArgs } from 'node:util';
import { fileURLToPath } from 'node:url';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const { values } = parseArgs({ options: { compiler: { type: 'string' }, script: { type: 'string', default: 'scripts/ci/check-programs.py' } } });
assert(values.compiler, 'Pass --compiler <osprey>');
const compiler = path.resolve(values.compiler);
const root = fs.mkdtempSync(path.join(repo, 'target/check-programs-test-'));
const good = 'print("ok")\n';
const red = 'print(1 + "x")\n';
const recursiveModule = (entry) => `module Survey {
    fn first(n) = match n { 0 => 0 _ => second(wrapSub(n, 1)) }
    fn second(n) = first(n)
    export fn run(n) = ${entry}
}
print(Survey::run(4))
`;
const project = '[project]\nname = "gate-fixture"\nsource_roots = ["src"]\ndefault_namespace = "app"\nentry = "main.osp"\n';
const fixtures = {
  'app/osprey.toml': project, 'app/src/main.osp': good, 'app/scratch/stray.osp': red,
  'loose/good.osp': good, 'loose/bad.osp': red,
  'loose/live-cycle.osp': recursiveModule('first(n)'),
  'loose/dead-cycle.osp': recursiveModule('n'),
  'pyflavor/script.py': '#!/usr/bin/env python3\n# Licensed MIT.\n# osprey: flavor=python\n' + red,
  'pyflavor/slash.py': '// osprey: flavor=ml\nfn bad() { missing }\n',
  'pyflavor/plain.py': '# a plain python file\ndef add(a, b):\n    return a + b\n',
  'x/failscompilation/red.osp': red, 'x/sample-registry/red.osp': red,
  // A type-discipline package names its checker project and has no [project] of its own ([TYPES-DISCIPLINE-REGISTRY]).
  'discipline/osprey.toml': '[package]\nname = "@fixture/discipline"\n\n[discipline]\nname = "fixture"\nchecker = "checker"\n',
  'discipline/checker/osprey.toml': project, 'discipline/checker/src/main.osp': good,
};
const failures = ['app/scratch/stray.osp', 'loose/bad.osp', 'loose/dead-cycle.osp', 'pyflavor/script.py', 'pyflavor/slash.py'];

function put(relative, body) {
  const file = path.join(root, relative);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, body);
}

function check(failGraph, expected, total) {
  const log = path.join(root, 'wrapper.log');
  fs.writeFileSync(log, '');
  const stdout = execFileSync(compiler, ['scripts/ci/check-programs.py', '--run', '--quiet', '--no-warnings'], {
    cwd: root, encoding: 'utf8', timeout: 90000,
    env: { ...process.env, REAL_COMPILER: compiler, WRAPPER_LOG: log, FAIL_GRAPH: failGraph, OSPREY_BIN: path.join(root, 'compiler-wrapper.sh') },
  });
  assert.deepEqual([...stdout.matchAll(/^FAIL (\S+)$/gm)].map(match => match[1]).sort(), expected.toSorted(), stdout);
  assert.equal(stdout.trimEnd().split('\n').at(-1), `${total} programs, ${expected.length} failed`);
  const calls = fs.readFileSync(log, 'utf8');
  for (const call of ['app --source-graph', 'discipline/checker --source-graph', 'loose/good.osp --llvm', 'loose/bad.osp --llvm']) assert(calls.includes(call), call);
  assert(!calls.includes("discipline --source-graph") && !calls.includes("discipline --llvm"), 'a manifest with no [project] is a package, never a program');
  assert.equal(calls.includes('app --llvm'), !failGraph, 'lower only after source discovery succeeds');
  for (const excluded of ['pyflavor/plain.py', 'x/failscompilation/red.osp', 'x/sample-registry/red.osp']) {
    assert(!calls.includes(excluded) && !stdout.includes(excluded), excluded);
  }
  const logs = path.join(root, 'target/check-programs');
  for (const file of fs.readdirSync(logs).filter(file => file.endsWith('.log'))) {
    assert(!/^define /m.test(fs.readFileSync(path.join(logs, file), 'utf8')), 'successful LLVM output accumulated in ' + file);
  }
  assert.match(stdout, /loose\/bad\.osp:1:[0-9]+: type mismatch:/, 'the failure report includes compiler diagnostics');
  for (const name of ['first', 'second']) {
    assert(stdout.includes(`private function \`Survey::${name}\` is never used; delete it`), stdout);
  }
  assert(!fs.existsSync(path.join(root, 'target/release/osprey')), 'all jobs must use the supplied compiler');
}

try {
  for (const [file, body] of Object.entries(fixtures)) put(file, body);
  const git = (...args) => execFileSync('git', args, { cwd: root });
  git('init', '-q');
  git('add', '--', ...Object.keys(fixtures));
  put('scripts/ci/check-programs.py', fs.readFileSync(path.resolve(repo, values.script)));
  put('compiler-wrapper.sh', '#!/bin/sh\nprintf "%s\\n" "$*" >> "$WRAPPER_LOG"\nif [ "$FAIL_GRAPH" = 1 ] && [ "$2" = --source-graph ]; then exit 1; fi\nexec "$REAL_COMPILER" "$@"\n');
  fs.chmodSync(path.join(root, 'compiler-wrapper.sh'), 0o755);
  check('', failures, 9);
  check('1', ['app', 'discipline/checker', ...failures.slice(1)], 8);
  console.log('check-programs gate regression: OK');
} finally {
  fs.rmSync(root, { recursive: true, force: true });
}
