// The package registry's contracts: its security suites gate CI and local
// tests under GC and ARC with a real uv, nested compiler invocations keep the
// selected compiler and memory backend, suites default to GC over stale
// inherited E2E context, and an invalid REGISTRY_MEMORY fails before any
// compiler runs.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { isAbsolute, join, relative, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { job, makeRecipe } from './ci-workflow-text.mjs'

function assertRealUv(registry, memory) {
  for (const requirement of [/uses: astral-sh\/setup-uv@[0-9a-f]{40}\b/,
    /version: "0\.11\.3"/, /python-version: "3\.12"/, /uv python install 3\.12/, /run: chmod \+x target\/release\/osprey target\/osprey-pkg/]) assert.match(registry, requirement)
  const available = registry.indexOf('uv --version')
  assert.ok(available >= 0, 'real uv must be available')
  assert.ok(available < registry.indexOf('run: make _test_registry'), 'uv must be installed before package tests')
  const local = makeRecipe('test')
  const suite = `$(MAKE) _test_registry REGISTRY_MEMORY=${memory}`
  assert.ok(local.includes(suite))
  assert.ok(local.indexOf('$(MAKE) target/osprey-pkg') >= 0 && local.indexOf('$(MAKE) target/osprey-pkg') < local.indexOf(suite))
}

const registrySuites = ['fixture-check', 'manifest/tests', 'cli/tests', 'tests', 'e2e']

// A fake compiler records, for every process the Make target starts, its argv plus
// the two E2E selection variables. Only those three fields are stored — no secret
// from the inherited environment — and the contract is read from real child
// processes rather than a source-text mirror.

function registryFixture() {
  mkdirSync(resolve('target'), { recursive: true })
  const root = mkdtempSync(resolve('target/registry-selection-'))
  const compiler = join(root, 'osprey')
  writeFileSync(compiler, `#!${process.execPath}

const fs = require('node:fs');
fs.appendFileSync(process.env.REGISTRY_TEST_RECEIPT, JSON.stringify({ argv: process.argv.slice(2),
  OSPREY_E2E_COMPILER: process.env.OSPREY_E2E_COMPILER, OSPREY_E2E_MEMORY: process.env.OSPREY_E2E_MEMORY }) + '\\n');
`, { mode: 0o755 })
  nativePythonFixture(root)
  return { root, compiler, receipt: join(root, 'receipt.jsonl') }
}

function nativePythonFixture(root) {
  writeFileSync(join(root, 'python3'), `#!${process.execPath}

const fs = require('node:fs');

const scripts = ['scripts/packages/test-package-publication.py', 'scripts/packages/test-package-interruption.py'];
if (process.argv.length !== 3 || !scripts.includes(process.argv[2])) process.exitCode = 1;
else fs.appendFileSync(process.env.REGISTRY_TEST_RECEIPT + '.native', JSON.stringify({
  script: process.argv[2], compiler: process.env.OSPREY_E2E_COMPILER,
  memory: process.env.OSPREY_E2E_MEMORY, client: process.env.OSPREY_PACKAGE_CLIENT }) + '\\n');
`, { mode: 0o755 })
}

function registryEnv({ root, receipt }, overrides) {
  const env = { ...process.env, PATH: `${root}:${process.env.PATH ?? ''}`, REGISTRY_TEST_RECEIPT: receipt }
  for (const [name, value] of Object.entries(overrides)) {
    if (value === undefined) delete env[name]
    else env[name] = value
  }
  return env
}

function runRegistryMake(fixture, overrides) {
  const result = spawnSync('make', ['-s', '-f', 'scripts/make/tests.mk', '_test_registry',
    `BIN=${relative(process.cwd(), fixture.compiler)}`], { encoding: 'utf8', env: registryEnv(fixture, overrides) })
  return { result, invocations: registryCalls(fixture.receipt), native: registryCalls(fixture.receipt + '.native') }
}

function registryCalls(path) {
  const recorded = existsSync(path) ? readFileSync(path, 'utf8') : ''
  return recorded.split('\n').filter(Boolean).map(line => JSON.parse(line))
}

function assertNativeRegistryCalls(native, fixture, memory) {
  const selected = memory === 'arc' ? 'target/registry-tests/osprey-pkg-arc' : 'target/osprey-pkg'
  assert.deepEqual(native.map(call => call.script), ['scripts/packages/test-package-publication.py', 'scripts/packages/test-package-interruption.py'])
  for (const call of native) {
    assert.equal(call.compiler, fixture.compiler)
    assert.equal(call.memory, memory)
    assert.equal(call.client, process.env.OSPREY_PACKAGE_CLIENT || resolve(selected))
  }
}

function withRegistryFixture(body) {
  const fixture = registryFixture()
  try { body(fixture) } finally { rmSync(fixture.root, { recursive: true, force: true }) }
}

export function registerRegistryContracts() {
  test('package manager security suites gate CI and local tests under GC and ARC', () => {
    const coverage = job('coverage', '.github/workflows/ci.yml')
    assert.match(coverage, /if:.*always\(\)/)
    for (const memory of ['gc', 'arc']) {
      const registry = job(`test-registry-${memory}`, '.github/workflows/ci.yml')
      assert.match(registry, /needs: \[changes, build\]/)
      assert.ok(registry.includes(`name: "Tests: package manager (${memory})"`))
      assert.ok(registry.includes(`run: make _test_registry REGISTRY_MEMORY=${memory}`))
      assertRealUv(registry, memory)
      assert.ok(coverage.includes(`test-registry-${memory}`))
      assert.ok(coverage.includes('test "${{ needs.test-registry-' + memory + '.result }}" = "success"'))
    }
    const commands = makeRecipe('_test_registry', 'scripts/make/tests.mk')
    for (const script of ['publication', 'interruption']) assert.match(commands.join('\n'), new RegExp(`OSPREY_PACKAGE_CLIENT=.*REGISTRY_NATIVE_CLIENT.*python3 scripts/packages/test-package-${script}\\.py`))
    for (const suite of ['fixture-check', 'manifest/tests', 'cli/tests', 'tests', 'e2e']) {
      assert.ok(commands.includes(`./$(BIN) test src/osprey/packages/manager/${suite} --memory=$(REGISTRY_MEMORY)`), suite)
    }
  })

  test('registry nested compiler invocations retain the selected compiler and memory backend', () => {
    withRegistryFixture(fixture => {
      const { result, invocations, native } = runRegistryMake(fixture, { REGISTRY_MEMORY: 'arc' })
      assert.equal(result.status, 0, result.stdout + result.stderr)
      assertNativeRegistryCalls(native, fixture, 'arc')
      // Order proves the fail-fast fixture suite still runs first.
      const expected = registrySuites.map(suite => ['test', `src/osprey/packages/manager/${suite}`])
      if (!process.env.OSPREY_PACKAGE_CLIENT) expected.push(['build', 'src/osprey/packages/manager/cli'])
      assert.deepEqual(invocations.map(seen => seen.argv.slice(0, 2)), expected)
      const builds = invocations.filter(seen => seen.argv[0] === 'build')
      assert.equal(builds.length, process.env.OSPREY_PACKAGE_CLIENT ? 0 : 1)
      if (builds.length) assert.ok(['src/osprey/packages/manager/cli', '--memory=arc', '--no-warnings', resolve('target/registry-tests/osprey-pkg-arc')].every(arg => builds[0].argv.includes(arg)))
      for (const seen of invocations) {
        assert.equal(seen.OSPREY_E2E_COMPILER, fixture.compiler)
        assert.ok(isAbsolute(seen.OSPREY_E2E_COMPILER), 'selection must stay absolute')
        assert.equal(seen.OSPREY_E2E_MEMORY, 'arc')
        assert.ok(seen.argv.includes('--memory=arc'), `${seen.argv[1]} must run ARC`)
      }
    })
  })

  test('registry suites default to GC and override stale inherited E2E context', () => {
    withRegistryFixture(fixture => {
      const { result, invocations, native } = runRegistryMake(fixture, { REGISTRY_MEMORY: undefined,
        OSPREY_E2E_COMPILER: '/stale/compiler', OSPREY_E2E_MEMORY: 'default' })
      assert.equal(result.status, 0, result.stdout + result.stderr)
      assertNativeRegistryCalls(native, fixture, 'gc')
      assert.equal(invocations.length, registrySuites.length)
      for (const seen of invocations) {
        assert.equal(seen.OSPREY_E2E_COMPILER, fixture.compiler)
        assert.equal(seen.OSPREY_E2E_MEMORY, 'gc')
        assert.ok(seen.argv.includes('--memory=gc'), `${seen.argv[1]} must run GC`)
      }
    })
  })

  test('an invalid REGISTRY_MEMORY fails before any compiler executes', () => {
    withRegistryFixture(fixture => {
      const { result, invocations } = runRegistryMake(fixture, { REGISTRY_MEMORY: 'default' })
      assert.notEqual(result.status, 0, 'unsupported backends must not be tolerated')
      assert.match(result.stdout + result.stderr, /Registry tests require GC or ARC/)
      assert.deepEqual(invocations, [], 'no suite may start with an unsupported backend')
    })
  })
}
