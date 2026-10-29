import assert from 'node:assert/strict'
import { readFileSync, mkdtempSync, rmSync, mkdirSync, writeFileSync, cpSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'
import { classify } from './ci-scope.mjs'
import { targetsMainPullRequests } from './workflow-pr-target.mjs'

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
  for (const payload of ['target/osprey-pkg', 'target/package-workspace.wasm']) {
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

function restoreStagedBundle(root) {
  cpSync(join(root, 'vscode-extension/bin'), join(root, 'extension/bin'), { recursive: true })
  const zipped = spawnSync('zip', ['-qr', 'vscode-extension/osprey-fixture.vsix', 'extension'], { cwd: root, encoding: 'utf8' })
  assert.equal(zipped.status, 0, zipped.stderr)
  rmSync(join(root, 'extension'), { recursive: true })
  rmSync(join(root, 'vscode-extension/bin'), { recursive: true })
  const block = job('test-extension', '.github/workflows/ci.yml').split('- name: Restore bundled extension assets\n')[1] ?? ''
  const run = block.match(/^ {8}run: \|\n((?: {10}.*\n)+)/m)?.[1] ?? ''
  assert.notEqual(run, '', 'CI must restore assets from the built VSIX')
  const result = spawnSync('bash', ['-euo', 'pipefail', '-c', run.split('\n').map(line => line.slice(10)).join('\n')], { cwd: root, encoding: 'utf8' })
  assert.equal(result.status, 0, result.stdout + result.stderr)
}

const nativeArchives = ['fiber_runtime', 'http_runtime'].flatMap(name => ['', '_gc', '_arc'].map(mode => `lib${name}${mode}.a`))

function writeBundlePayloads(root, platform) {
  const payloads = [['target/release/osprey', 'osprey'], ['target/osprey-pkg', 'osprey-pkg'], ['target/osprey-registry', 'osprey-registry'],
    ['target/package-workspace.wasm', 'package-workspace.wasm'], ...nativeArchives.map(name => [`compiler/bin/${name}`, name])]
  for (const dir of ['target/release', 'compiler/bin', platform, 'vscode-extension/scripts']) mkdirSync(join(root, dir), { recursive: true })
  writeFileSync(join(root, 'vscode-extension/scripts/verify-package-client.mjs'), readFileSync('vscode-extension/scripts/verify-package-client.mjs'))
  for (const [source, name] of payloads) {
    const bytes = name.endsWith('.wasm') ? Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]) : '#!/bin/sh\nexit 97\n'
    for (const file of [source, `${platform}/${name}`]) writeFileSync(join(root, file), bytes, { mode: 0o755 })
  }
}

function stagedBundleFixture(change = () => {}) {
  const root = mkdtempSync(join(tmpdir(), 'osprey-staged-bundle-'))
  const host = `${process.platform === 'win32' ? 'win32' : process.platform === 'darwin' ? 'darwin' : 'linux'}-${process.arch === 'arm64' ? 'arm64' : 'x64'}`
  const platform = `vscode-extension/bin/${host}`
  try {
    writeBundlePayloads(root, platform)
    restoreStagedBundle(root)
    change(root, platform)
    return spawnSync('make', ['-f', resolve('scripts/make/extension.mk'), '_vsix_verify',
      'BIN=target/release/osprey', 'EXT_DIR=vscode-extension', `EXT_PLATFORM=${host}`, 'RTB=compiler/bin',
      `NATIVE_RUNTIME_ARCHIVES=${nativeArchives.map(name => `compiler/bin/${name}`).join(' ')}`], { cwd: root, encoding: 'utf8' })
  } finally { rmSync(root, { recursive: true, force: true }) }
}

test('staged bundle verification fails closed without recompiling a payload', () => {
  const complete = stagedBundleFixture()
  assert.equal(complete.status, 0, complete.stdout + complete.stderr)
  for (const name of ['osprey', 'osprey-pkg', 'osprey-registry', 'package-workspace.wasm', ...nativeArchives]) {
    const missing = stagedBundleFixture((root, platform) => rmSync(join(root, platform, name)))
    assert.notEqual(missing.status, 0, `${name}: missing payload accepted`)
    const stale = stagedBundleFixture((root, platform) => writeFileSync(join(root, platform, name), 'stale'))
    assert.notEqual(stale.status, 0, `${name}: stale payload accepted`)
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

test('the KLEE engine gates stay out of CI; the mock oracle and known-bug target stay in the tree', () => {
  // Owner decision 2026-10-26: the engine gates do not guarantee a green
  // signal while issue #306 is open (clean.osp never completes inside the
  // pinned budget), so CI runs none of them. Verification is local:
  // make _test_hunt / make _test_hunt_clean / make _test_hunt_crashes.
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
    assert.ok(readFileSync('scripts/verify-branch-protection.mjs', 'utf8').includes(`'Tests: package manager (${memory})'`))
  }
  const commands = makeRecipe('_test_registry', 'scripts/make/tests.mk')
  assert.match(commands.join('\n'), /OSPREY_PACKAGE_CLIENT=.*target\/osprey-pkg.*python3 scripts\/test-package-publication\.py/)
  for (const suite of ['fixture-check', 'manifest/tests', 'cli/tests', 'tests', 'e2e']) {
    assert.ok(commands.includes(`./$(BIN) test packages/manager/${suite} --memory=$(REGISTRY_MEMORY)`), suite)
  }
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
