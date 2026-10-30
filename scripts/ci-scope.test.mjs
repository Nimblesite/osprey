import assert from 'node:assert/strict'
import { readFileSync, readdirSync, mkdtempSync, rmSync, mkdirSync, writeFileSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve, relative, isAbsolute } from 'node:path'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'
import { classify } from './ci-scope.mjs'
import { runStagedBundleFixture, nativeArchives, proofAssets } from './ci-bundle-fixture.mjs'
import { targetsMainPullRequests } from './workflow-pr-target.mjs'
import { registerLintContracts } from './ci-lint-contract-tests.mjs'

function deadCodeFixture(product, tests) {
  const root = mkdtempSync(join(tmpdir(), 'osprey-dead-code-'))
  try {
    mkdirSync(join(root, 'scripts'))
    mkdirSync(join(root, 'crates/fixture/src'), { recursive: true })
    writeFileSync(join(root, 'scripts/check.mjs'), readFileSync('scripts/verify-no-dead-code.mjs'))
    writeFileSync(join(root, 'crates/fixture/src/main.rs'), product)
    writeFileSync(join(root, 'crates/fixture/src/tests.rs'), tests)
    return spawnSync(process.execPath, [join(root, 'scripts/check.mjs')], { encoding: 'utf8' })
  } finally { rmSync(root, { recursive: true, force: true }) }
}

test('dead-code gate distinguishes test module helpers from unused product code', () => {
  const helper = deadCodeFixture('#[cfg(test)]\nmod tests;\nfn main() {}',
    'pub(crate) fn assembled_program() {}')
  assert.equal(helper.status, 0, helper.stderr)
  const dead = deadCodeFixture('pub fn orphan() {}\n#[cfg(test)]\nmod tests;\nfn main() {}',
    '#[test]\nfn regression() { crate::orphan(); }')
  assert.equal(dead.status, 1)
  assert.match(dead.stderr, /crates\/fixture\/src\/main.rs:1  fn orphan/)
  const productModule = deadCodeFixture('mod tests;\nfn main() {}', 'pub fn unused_product() {}')
  assert.equal(productModule.status, 1)
  assert.match(productModule.stderr, /fn unused_product/)
  const comment = deadCodeFixture('// #[cfg(test)]\nmod tests;\nfn main() {}', 'pub fn unused_product() {}')
  assert.equal(comment.status, 1)
  const live = deadCodeFixture('pub fn active() {}\nfn main() { active(); }', '')
  assert.equal(live.status, 0, live.stderr)
})

const cases = [
  ['book chapter and runnable example', ['Book/manuscript/04.md', 'Book/examples/chapter-04/main.osp'], false, true, false],
  ['benchmark tooling and data', ['benchmarks/run.sh', 'benchmarks/results/results.json'], false, false, true],
  ['specification prose', ['docs/specs/language/0007-PatternMatching.md'], false, false, false],
  ['repository prose', ['README.md', 'CLAUDE.md'], false, false, false],
  ['compiler source', ['crates/osprey-codegen/src/lib.rs'], true, false, false],
  ['runtime source', ['compiler/runtime/random_runtime.c'], true, false, false],
  ['build orchestration', ['Makefile'], true, false, false],
  ['included build rules', ['scripts/make/runtime.mk'], true, false, false],
  ['changed CI classifier', ['scripts/ci-scope.mjs'], true, false, false],
  ['changed CI workflow', ['.github/workflows/ci.yml'], true, false, false],
  ['unknown future component', ['new-compiler/main.py'], true, false, false],
  ['embedded compiler documentation', ['crates/osprey-cli/assets/help.md'], true, false, false],
  ['ported package documentation the package suite checks', ['packages/testing/README.md'], true, false, false],
  ['Python-flavor package regression', ['packages/faiss/tests/main.py'], true, false, false],
  ['symbolic execution regression', ['scripts/test-hunt-default-budget.py'], true, false, false],
  ['GPU regression in both flavors', ['tests/core/gpu/scalar_contracts.test.osp', 'tests/core/gpu/scalar_contracts.test.ospml'], true, false, false],
  ['proof kernel sources are compiler code', ['examples/proof/kernel/main.ospml', 'examples/proof/kernel/expectedoutput'], true, false, false],
  ['proof speed benchmark harness and cases', ['benchmarks/proof/run.sh', 'benchmarks/proof/m0_identity/id_imp.lean'], false, false, true],
]
for (const [label, files, code, book, benchmarks] of cases) {
  test(label, () => assert.deepEqual(classify(files), { code, website: false, book, benchmarks }))
}

test('website content skips compiler checks but the embedded grammar never does', () => {
  assert.deepEqual(classify(['website/src/benchmarks.md']), { code: false, website: true, book: false, benchmarks: false })
  assert.equal(classify(['website/src/js/osprey-grammar.mjs']).code, true)
})

test('mixed content and compiler changes retain every applicable check', () => {
  assert.deepEqual(classify(['Book/book.json', 'benchmarks/run.sh', 'compiler/runtime/memory.c']),
    { code: true, website: false, book: true, benchmarks: true })
})

test('all workflows use the shared classifier and content checks remain required', () => {
  const linux = readFileSync('.github/workflows/ci.yml', 'utf8')
  const windows = readFileSync('.github/workflows/ci-windows.yml', 'utf8')
  assert.equal(linux.split('uses: ./.github/actions/detect-ci-scope').length - 1, 2)
  assert.match(windows, /uses: \.\/\.github\/actions\/detect-ci-scope/)
  assert.match(linux, /python3 benchmarks\/test_merge_results.py --tooling-only/)
  assert.match(linux, /make -C Book check-examples/)
  assert.match(linux, /make -C Book check\n/)
})

test('extension tests receive and reuse the build job\'s complete bundle', () => {
  const build = job('build', '.github/workflows/ci.yml')
  for (const payload of ['target/osprey-pkg', 'target/package-workspace.wasm', 'target/workbench.wasm']) {
    assert.match(build, new RegExp(`^ +${payload.replaceAll('.', '\\.')}\\s*$`, 'm'), payload)
  }
  const tests = readFileSync('scripts/make/tests.mk', 'utf8')
  const prerequisites = tests.match(/^_test_vscode_extension:(.*)$/m)?.[1] ?? ''
  assert.match(prerequisites, /\b_vsix_verify\b/)
  assert.doesNotMatch(prerequisites, /\b_vsix_bundle\b/)
  const commands = makeRecipe('test')
  assert.ok(commands.indexOf('$(MAKE) _vsix_bundle') >= 0)
  assert.ok(commands.indexOf('$(MAKE) _vsix_bundle') < commands.indexOf('$(MAKE) _test_vscode_extension'))
  const extension = job('test-extension', '.github/workflows/ci.yml')
  for (const required of [/name: Restore bundled extension assets/, /chmod \+x[^\n]*target\/osprey-pkg/,
    /chmod \+x[^\n]*vscode-extension\/bin\/linux-x64\/osprey-pkg/]) assert.match(extension, required)
})

test('extension source fixtures have the WASI linker and sysroot before coverage', () => {
  const extension = job('test-extension', '.github/workflows/ci.yml')
  const beforeCoverage = extension.split('- name: Extension suite with V8 coverage')[0]
  assert.match(beforeCoverage, /apt-get install[^\n]*\blld\b/)
  assert.match(beforeCoverage, /wasi-sysroot-\$\{WASI_SDK_VERSION\}/)
  assert.match(beforeCoverage, /test -d .*lib\/wasm32-wasip1/)
  assert.match(beforeCoverage, /OSPREY_WASI_SYSROOT=.*>> "\$GITHUB_ENV"/)
  const build = job('build', '.github/workflows/ci.yml')
  assert.match(build, /compiler\/bin\/\*\.a/)
})

// The bundle stages apps/workbench/assets/. and apps/workbench/lessons/media/.
// Their *.png, *.gif and *.mp4 are LFS-stored, so a fetch that omits either path
// stages a pointer and the packaged extension ships bytes the Marketplace (and
// the client verifier) reject. The logo and both README editor GIFs are
// LFS-stored too, so each job fetches exactly the paths its bundle stages.
test('LFS fetches cover every workbench source asset the bundle stages', () => {
  const required = ['website/src/assets/images/logo.png', 'vscode-extension/media/package-tour.gif',
    'vscode-extension/media/language-tour.gif', 'apps/workbench/assets/**', 'apps/workbench/lessons/media/**']
  for (const [name, file] of [['test-extension', '.github/workflows/ci.yml'], ['vsix', '.github/workflows/release.yml']]) {
    const include = job(name, file).match(/git lfs pull --include="([^"]+)"/)?.[1] ?? ''
    for (const path of required) assert.ok(include.split(',').includes(path), `${name}: LFS fetch omits ${path}`)
  }
})

test('the required macOS gate builds and tests the native Workbench host', () => {
  const coverage = job('coverage', '.github/workflows/ci.yml')
  const step = (coverage.split('- name: Native Workbench host and rendering regressions\n')[1] ?? '').split('\n      - ')[0]
  assert.match(step, /^ {8}run: \|\n {10}bash apps\/workbench\/hosts\/apple\/run\.sh --build\n {10}swift test --package-path apps\/workbench\/hosts\/apple$/m)
  assert.match(step, /^ {10}OSPREY_BIN: \$\{\{ github\.workspace \}\}\/target\/release\/osprey$/m)
  assert.doesNotMatch(step, /^\s*(?:-\s*)?if:/m)
  assert.doesNotMatch(step, /continue-on-error/)
  assert.match(coverage, /^    runs-on: macos-15$/m)
  assert.match(coverage, /^ {10}lfs: true$/m)
})

const stagedBundleFixture = (change) => runStagedBundleFixture(job('test-extension', '.github/workflows/ci.yml'), change)

test('staged bundle verification fails closed without recompiling a payload', () => {
  const complete = stagedBundleFixture()
  assert.equal(complete.status, 0, complete.stdout + complete.stderr)
  for (const name of ['osprey', 'osprey-pkg', 'osprey-registry', 'package-workspace.wasm', 'workbench.wasm', 'workbench/lessons/catalogue.json', 'workbench/osprey-logo.png', 'workbench/chalk-study.png', 'workbench/workbench.css', 'workbench/symbols.json', 'workbench/icons/moon.svg', 'workbench/icons/LICENSE.txt', ...nativeArchives, ...proofAssets]) {
    const missing = stagedBundleFixture((root, platform) => rmSync(join(root, platform, name)))
    assert.notEqual(missing.status, 0, `${name}: missing payload accepted`)
    const stale = stagedBundleFixture((root, platform) => writeFileSync(join(root, platform, name), 'stale'))
    assert.notEqual(stale.status, 0, `${name}: stale payload accepted`)
  }
  if (process.platform !== 'win32') {
    const unregistered = stagedBundleFixture((root, platform) => rmSync(join(root, platform, 'registry'), { recursive: true }))
    assert.notEqual(unregistered.status, 0, 'a bundle without its local registry packages accepted')
  }
  const invalid = stagedBundleFixture((root, platform) => {
    for (const file of ['target/package-workspace.wasm', `${platform}/package-workspace.wasm`]) writeFileSync(join(root, file), 'invalid WASM')
  })
  assert.notEqual(invalid.status, 0, 'matching invalid WASM accepted')
  assert.match(invalid.stderr, /Invalid package workspace WASM/)
  const shadowed = stagedBundleFixture((root, platform) => {
    writeFileSync(join(root, '_vsix_verify'), 'unrelated file')
    rmSync(join(root, platform, 'osprey-pkg'))
  })
  assert.notEqual(shadowed.status, 0, 'a same-named file must not bypass bundle verification')
})

test('malformed detection input fails the gate instead of skipping required jobs', () => {
  const temporary = mkdtempSync(join(tmpdir(), 'osprey-ci-scope-'))
  try {
    const env = { ...process.env, GITHUB_OUTPUT: join(temporary, 'output'), CHANGED_FILES: '{}' }
    assert.notEqual(spawnSync(process.execPath, ['scripts/ci-scope.mjs'], { env }).status, 0)
    env.CHANGED_FILES = '["Book/book.json"]'
    assert.equal(spawnSync(process.execPath, ['scripts/ci-scope.mjs'], { env }).status, 0)
    assert.match(readFileSync(env.GITHUB_OUTPUT, 'utf8'), /^code=false\nwebsite=false\nbook=true\nbenchmarks=false\n$/)
  } finally { rmSync(temporary, { recursive: true }) }
})

const workflowJobs = (file) => readFileSync(file, 'utf8').split(/(?=^  [\w-]+:\s*$)/m)
const job = (name, file = '.github/workflows/release.yml') => workflowJobs(file).find((block) => block.startsWith(`  ${name}:`)) ?? ''

const makeRecipe = (name, file = 'Makefile') => {
  const commands = []
  let active = false
  for (const line of readFileSync(file, 'utf8').split('\n')) {
    if (line.startsWith(`${name}:`)) { active = true; continue }
    if (active && /^[^\s#][^:]*:/.test(line)) break
    if (active && line.startsWith('\t')) commands.push(line.trim())
  }
  return commands
}

function rustCoverageFixture(entries, covered = 100) {
  const root = mkdtempSync(join(tmpdir(), 'osprey-rust-coverage-'))
  try {
    for (const name of ['one', 'two']) {
      mkdirSync(join(root, 'crates', name), { recursive: true })
      writeFileSync(join(root, 'crates', name, 'Cargo.toml'), `[package]\nname = "${name}"\n`)
    }
    writeFileSync(join(root, 'thresholds.json'), JSON.stringify({ projects: { rust: entries } }))
    writeFileSync(join(root, 'lcov.info'), ['one', 'two'].map(name =>
      `SF:${root}/crates/${name}/src/lib.rs\nLF:100\nLH:${covered}\nend_of_record\n`).join(''))
    return spawnSync('make', ['-f', resolve('scripts/make/tests.mk'), '_coverage_check_rust',
      'COVERAGE_THRESHOLDS_FILE=thresholds.json', 'RUST_LCOV=lcov.info'], { cwd: root, encoding: 'utf8' })
  } finally { rmSync(root, { recursive: true, force: true }) }
}

test('Rust coverage gate rejects unlisted crates and invalid thresholds', () => {
  const gate = { threshold: 95, language: 'rust' }
  const missing = rustCoverageFixture({ one: gate })
  assert.notEqual(missing.status, 0, missing.stdout)
  assert.match(missing.stdout, /two FAIL: missing coverage threshold/)
  for (const threshold of [null, 0, 94, '95', 101]) {
    const invalid = rustCoverageFixture({ one: gate, two: { threshold } })
    assert.notEqual(invalid.status, 0, `threshold ${threshold}: ${invalid.stdout}`)
  }
  const passing = rustCoverageFixture({ one: gate, two: gate }, 95)
  assert.equal(passing.status, 0, passing.stdout + passing.stderr)
  const failing = rustCoverageFixture({ one: gate, two: gate }, 94)
  assert.notEqual(failing.status, 0, failing.stdout)
  assert.match(failing.stdout, /two FAIL: 94.0% < 95%/)
})

test('package coverage preserves the complete test recipe and checks without rerunning suites', () => {
  const commands = makeRecipe('test')
  for (const target of ['_test_packages', '_coverage_check_packages', '_test_python_app_ports', '_conformance-gc',
    '_conformance-arc', '_test_profiler', '_test_vscode_extension', '_coverage_check_vscode_extension', '_test_bench_tools']) {
    assert.equal(commands.filter((line) => line === `$(MAKE) ${target}`).length, 1, target)
  }
  const check = makeRecipe('_coverage_check_packages')
  assert.equal(check.length, 1, 'checking collected coverage must not run unrelated suites')
  assert.match(check[0], /^python3 scripts\/package_coverage\.py /)
  assert.doesNotMatch(check[0], /--collect/)
})

test('the KLEE engine gates stay out of CI and make ci; the mock oracle and known-bug target stay in the tree', () => {
  // Owner decision 2026-10-26: the engine gates do not guarantee a green
  // signal while issue #306 is open (clean.osp never completes inside the
  // pinned budget), so CI runs none of them, and neither does `make ci`.
  // Verification is by hand:
  // make _test_hunt / make _test_hunt_clean / make _test_hunt_crashes.
  const local = makeRecipe('test')
  assert.ok(local.includes('$(MAKE) _test_hunt_mock'))
  assert.deepEqual(local.filter((command) => /_test_hunt(?!_mock)/.test(command)), [])
  assert.doesNotMatch(readFileSync('Makefile', 'utf8').match(/^ci:.*$/m)[0], /hunt/)
  const workflow = readFileSync('.github/workflows/ci.yml', 'utf8')
  assert.doesNotMatch(workflow, /brew install klee/)
  assert.doesNotMatch(workflow, /make _test_hunt(?!_mock)/)
  assert.doesNotMatch(workflow, /make _runtime_klee/)
  assert.doesNotMatch(workflow, /_test_hunt_crashes/)
  assert.match(job('test-c-runtime', '.github/workflows/ci.yml'), /make _test_hunt_mock/)
  const expectations = readFileSync('scripts/make/tests.mk', 'utf8')
  assert.match(expectations, /HUNT_KNOWN_BUGS \?= clean/)
  assert.match(expectations, /^_test_hunt_clean:/m)
  for (const control of ['clean.osp', 'clean.expected']) assert.ok(readFileSync(`tests/hunt/${control}`).length > 0)
  assert.match(readFileSync('scripts/test-hunt-default-budget.py', 'utf8'), /\('gc', 'arc'\), \('crash',\)/)
})

test('ported packages remain in the unfiltered Rust workspace gate', () => {
  assert.match(job('test-rust', '.github/workflows/ci.yml'), /run: make _test_rust/)
  const make = readFileSync('scripts/make/tests.mk', 'utf8')
  assert.match(make, /cargo llvm-cov --workspace --profile ci --lcov/)
  assert.doesNotMatch(make, /cargo llvm-cov[^\n]*(?:--exclude|--skip)/)
})

registerLintContracts(job('build', '.github/workflows/ci.yml'))

test('package source coverage runs independently and blocks the required coverage gate', () => {
  const packages = job('test-packages', '.github/workflows/ci.yml')
  assert.match(packages, /needs: \[changes, build\]/)
  assert.match(packages, /if:.*needs\.changes\.outputs\.code/)
  assert.match(packages, /run: make _test_packages/)
  assert.match(packages, /name: Upload package coverage\n\s+if: always\(\)/)
  assert.match(packages, /name: coverage-packages/)
  assert.match(packages, /if-no-files-found: error/)
  assert.match(job('coverage', '.github/workflows/ci.yml'), /needs:.*\btest-packages\b/)
})

test('application ports have an independent complete backend gate', () => {
  const apps = job('test-python-app-ports', '.github/workflows/ci.yml')
  assert.match(apps, /needs: \[changes, build\]/)
  assert.match(apps, /if:.*needs\.changes\.outputs\.code/)
  assert.match(apps, /run: make _test_python_app_ports/)
})

test('the proof kernel and the Lean speed gate run on the staged build', () => {
  const proof = job('test-proof', '.github/workflows/ci.yml')
  assert.match(proof, /needs: \[changes, build\]/)
  // examples/proof/** classifies as code and benchmarks/proof/** as
  // benchmarks; both outputs gate the job exactly as the changes job gates
  // its own benchmark steps.
  assert.match(proof, /needs\.changes\.outputs\.code == 'true' \|\| needs\.changes\.outputs\.benchmarks == 'true'/)
  assert.match(proof, /run: make _test_proof_kernel/)
  assert.match(proof, /run: zsh benchmarks\/proof\/run\.sh/)
  // Lean is the speed gate's baseline, so the toolchain is pinned to an exact
  // release (`stable` floats) and a non-zero exit is never softened. The pin
  // lives in the tools' lean-toolchain, which the job installs and the tools
  // cache key hashes, so the two cannot disagree.
  assert.match(proof, /--default-toolchain "\$\(cat benchmarks\/proof\/tools\/lean-toolchain\)"/)
  assert.match(readFileSync('benchmarks/proof/tools/lean-toolchain', 'utf8'), /^leanprover\/lean4:v\d+\.\d+\.\d+\n$/)
  assert.match(proof, /hashFiles\([^)]*benchmarks\/proof\/tools\/lean-toolchain/)
  assert.doesNotMatch(proof, /--default-toolchain stable/)
  assert.doesNotMatch(proof, /continue-on-error/)
  assert.doesNotMatch(proof, /\|\| true/)
  // The recorded export is re-derived from the pinned exporter in CI.
  assert.match(proof, /run: bash scripts\/proof-export\.sh --check/)
  assert.match(proof, /run: bash scripts\/proof-export\.sh --check --theorem and_swap/)
  assert.match(proof, /run: bash scripts\/proof-export\.sh --check --theorem Nat\.add_comm/)
  assert.match(proof, /run: node --test scripts\/test-proof-export\.mjs/)
  assert.match(proof, /run: python3 scripts\/test-proof-openai\.py/)
  assert.match(proof, /run: python3 scripts\/proof-openai\.py --compiler target\/release\/osprey/)
  assert.match(proof, /run: python3 scripts\/proof-openai\.py --corpus lean-core --compiler target\/release\/osprey/)
  assert.match(proof, /run: python3 scripts\/test-proof-literals\.py/)
})

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
  for (const script of ['publication', 'interruption']) assert.match(commands.join('\n'), new RegExp(`OSPREY_PACKAGE_CLIENT=.*REGISTRY_NATIVE_CLIENT.*python3 scripts/test-package-${script}\\.py`))
  for (const suite of ['fixture-check', 'manifest/tests', 'cli/tests', 'tests', 'e2e']) {
    assert.ok(commands.includes(`./$(BIN) test packages/manager/${suite} --memory=$(REGISTRY_MEMORY)`), suite)
  }
})

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
const scripts = ['scripts/test-package-publication.py', 'scripts/test-package-interruption.py'];
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
  assert.deepEqual(native.map(call => call.script), ['scripts/test-package-publication.py', 'scripts/test-package-interruption.py'])
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

test('registry nested compiler invocations retain the selected compiler and memory backend', () => {
  withRegistryFixture(fixture => {
    const { result, invocations, native } = runRegistryMake(fixture, { REGISTRY_MEMORY: 'arc' })
    assert.equal(result.status, 0, result.stdout + result.stderr)
    assertNativeRegistryCalls(native, fixture, 'arc')
    // Order proves the fail-fast fixture suite still runs first.
    const expected = registrySuites.map(suite => ['test', `packages/manager/${suite}`])
    if (!process.env.OSPREY_PACKAGE_CLIENT) expected.push(['build', 'packages/manager/cli'])
    assert.deepEqual(invocations.map(seen => seen.argv.slice(0, 2)), expected)
    const builds = invocations.filter(seen => seen.argv[0] === 'build')
    assert.equal(builds.length, process.env.OSPREY_PACKAGE_CLIENT ? 0 : 1)
    if (builds.length) assert.ok(['packages/manager/cli', '--memory=arc', '--no-warnings', resolve('target/registry-tests/osprey-pkg-arc')].every(arg => builds[0].argv.includes(arg)))
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

test('every release publishes only after an unconditional dependency audit', () => {
  assert.match(job('audit'), /uses: \.\/\.github\/workflows\/ci-audit\.yml/)
  assert.doesNotMatch(job('audit'), /^    if:/m)
  for (const name of ['github-release', 'brew', 'scoop', 'vsix', 'publish-marketplace',
    'publish-openvsx', 'deploy-site', 'deploy-webcompiler']) {
    assert.match(job(name), /needs:.*\baudit\b/, `${name} must wait for the audit`)
  }
})

test('standalone deployment dispatch runs the mandatory dependency audit first', () => {
  for (const file of ['.github/workflows/deploy-pages.yml', '.github/workflows/deploy-webcompiler.yml']) {
    const audit = job('audit', file)
    assert.match(audit, /uses: \.\/\.github\/workflows\/ci-audit\.yml/)
    assert.doesNotMatch(audit, /^    if:/m)
    assert.match(audit, /^    permissions:\n      contents: read$/m)
  }
  assert.match(job('build-and-deploy', '.github/workflows/deploy-pages.yml'), /needs:.*\baudit\b/)
  assert.match(job('deploy', '.github/workflows/deploy-webcompiler.yml'), /needs:.*\baudit\b/)
})

test('Dependabot stages updates and keeps a consolidation PR flowing to main', () => {
  const config = readFileSync('.github/dependabot.yml', 'utf8')
  assert.equal((config.match(/package-ecosystem:/g) ?? []).length,
    (config.match(/target-branch: ["']?dependabot-upgrades/g) ?? []).length)
  const sweep = readFileSync('.github/workflows/dependabot-automerge.yml', 'utf8')
  assert.match(sweep, /github.actor == 'dependabot\[bot\]'/)
  assert.doesNotMatch(sweep, /pull_request_target:/)
  assert.match(sweep, /--head dependabot-upgrades --base main/)
  assert.match(sweep, /git diff -z --name-only/)
})

test('main requires one root job that needs every job, so the ruleset lists none', () => {
  const ci = '.github/workflows/ci.yml'
  const gate = job('ci', ci)
  assert.match(gate, /^ {4}name: CI\s*$/m)
  assert.match(gate, /if: \$\{\{ always\(\) \}\}/)
  const needs = [...gate.matchAll(/^ {6}- ([\w-]+)\s*$/gm)].map((m) => m[1])
  const jobs = readFileSync(ci, 'utf8').split(/^jobs:\s*$/m)[1]
  const others = [...jobs.matchAll(/^ {2}([\w-]+):\s*$/gm)].map((m) => m[1]).filter((key) => key !== 'ci')
  assert.deepEqual([...needs].sort(), [...others].sort())
  for (const called of ['ci-windows.yml', 'gpui.yml', 'winui.yml']) {
    assert.ok(readFileSync(ci, 'utf8').includes(`uses: ./.github/workflows/${called}`), called)
  }
  const beside = readdirSync('.github/workflows').filter((f) => f !== 'ci.yml' && /\.ya?ml$/.test(f))
    .filter((f) => targetsMainPullRequests(readFileSync(`.github/workflows/${f}`, 'utf8')))
  assert.deepEqual(beside, [])
  const guard = readFileSync('scripts/verify-branch-protection.mjs', 'utf8')
  assert.match(guard, /const REQUIRED_CHECK = 'CI'/)
  assert.doesNotMatch(guard, /EXPECTED_CONTEXTS/)
})

test('branch protection distinguishes staging automation from main CI without exempting jobs', () => {
  for (const branches of ['[main]', '[main, dependabot-upgrades]', '["m*"]']) {
    assert.equal(targetsMainPullRequests(`on:\n  pull_request:\n    branches: ${branches}\n`), true)
  }
  assert.equal(targetsMainPullRequests('on:\n  pull_request:\n'), true)
  assert.equal(targetsMainPullRequests('on:\n  pull_request:\n    branches:\n      - dependabot-upgrades\n'), false)
  assert.equal(targetsMainPullRequests('on:\n  pull_request:\n    branches: [dependabot-upgrades]\n'), false)
  assert.equal(targetsMainPullRequests('on:\n  workflow_call:\n'), false)
  // A Windows checkout with core.autocrlf ends every line with CRLF.
  assert.equal(targetsMainPullRequests('on:\r\n  pull_request:\r\n    branches:\r\n      - dependabot-upgrades\r\n'), false)
  assert.equal(targetsMainPullRequests('on:\r\n  pull_request:\r\n    branches: [dependabot-upgrades]\r\n'), false)
  assert.equal(targetsMainPullRequests('on:\r\n  pull_request:\r\n    branches:\r\n      - main\r\n'), true)
})
