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
const project = '[project]\nname = "gate-fixture"\nsource_roots = ["src"]\ndefault_namespace = "app"\nentry = "main.osp"\n';
const fixtures = {
  'app/osprey.toml': project, 'app/src/main.osp': good, 'app/scratch/stray.osp': red,
  'loose/good.osp': good, 'loose/bad.osp': red,
  'pyflavor/script.py': '#!/usr/bin/env python3\n# Licensed MIT.\n# osprey: flavor=python\n' + red,
  'pyflavor/slash.py': '// osprey: flavor=ml\nfn bad() { missing }\n',
  'pyflavor/plain.py': '# a plain python file\ndef add(a, b):\n    return a + b\n',
  'x/failscompilation/red.osp': red, 'x/sample-registry/red.osp': red,
};
const failures = ['app/scratch/stray.osp', 'loose/bad.osp', 'pyflavor/script.py', 'pyflavor/slash.py'];

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
  for (const call of ['app --source-graph', 'loose/good.osp --llvm', 'loose/bad.osp --llvm']) assert(calls.includes(call), call);
  assert.equal(calls.includes('app --llvm'), !failGraph, 'lower only after source discovery succeeds');
  for (const excluded of ['pyflavor/plain.py', 'x/failscompilation/red.osp', 'x/sample-registry/red.osp']) {
    assert(!calls.includes(excluded) && !stdout.includes(excluded), excluded);
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
  check('', failures, 6);
  check('1', ['app', ...failures.slice(1)], 5);
  console.log('check-programs gate regression: OK');
} finally {
  fs.rmSync(root, { recursive: true, force: true });
}
