import assert from 'node:assert/strict'
import { readFileSync, readdirSync, mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'
import { classify } from './ci-scope.mjs'
import { runStagedBundleFixture, nativeArchives, proofAssets } from './ci-bundle-fixture.mjs'
import { targetsMainPullRequests } from './workflow-pr-target.mjs'
import { registerLintContracts } from './ci-lint-contract-tests.mjs'
import { registerProofContracts } from './ci-proof-contract-tests.mjs'
import { registerRegistryContracts } from './ci-registry-contract-tests.mjs'
import { job, makeRecipe } from './ci-workflow-text.mjs'

// The workflow nearly every contract here is written against: its text, and
// single jobs of it, named once so the workflow path never drifts between tests.
const CI = '.github/workflows/ci.yml'
const ciText = readFileSync(CI, 'utf8')
const ciJob = (name) => job(name, CI)
// The jobs more than one test asserts against, read once.
const buildJob = job('build', CI)
const extensionJob = job('test-extension', CI)
const coverageJob = job('coverage', CI)
const rustJob = job('test-rust', CI)
// The test suite's own definition, and the recipe every gate contract walks over.
const testsMk = readFileSync('scripts/make/tests.mk', 'utf8')
const testRecipe = makeRecipe('test')

const cases = [
  ['book chapter and runnable example', ['docs/book/manuscript/04.md', 'docs/book/examples/chapter-04/main.osp'], false, true, false, true],
  ['benchmark tooling and data', ['src/toolchain/benchmarks/run.sh', 'src/toolchain/benchmarks/results/results.json'], false, false, true, false],
  ['specification prose', ['docs/specs/language/0007-PatternMatching.md'], false, false, false, false],
  ['repository prose', ['README.md', 'CLAUDE.md'], false, false, false, false],
  ['compiler source', ['src/toolchain/crates/osprey-codegen/src/lib.rs'], true, false, false, false],
  ['runtime source', ['src/toolchain/compiler/runtime/random_runtime.c'], true, false, false, false],
  ['build orchestration', ['Makefile'], true, false, false, false],
  ['included build rules', ['scripts/make/runtime.mk'], true, false, false, false],
  ['changed CI classifier', ['scripts/ci/ci-scope.mjs'], true, false, false, false],
  ['changed CI workflow', ['.github/workflows/ci.yml'], true, false, false, false],
  ['unknown future component', ['new-compiler/main.py'], true, false, false, true],
  ['embedded compiler documentation', ['src/toolchain/crates/osprey-cli/assets/help.md'], true, false, false, false],
  ['ported package documentation the package suite checks', ['src/osprey/packages/testing/README.md'], true, false, false, false],
  ['Python-flavor package regression', ['src/osprey/packages/faiss/tests/main.py'], true, false, false, true],
  ['TypeScript-flavor package and corpus twin', ['src/osprey/packages/express/src/express.ts', 'src/osprey/tests/flavors/smoke/smoke.test.ts'], true, false, false, true],
  ['TypeScript tooling is not an Osprey program', ['src/toolchain/vscode-extension/client/src/extension.ts'], true, false, false, false],
  ['a package declaration file is not an Osprey program', ['src/osprey/packages/express/types/express.d.ts'], true, false, false, false],
  ['a TypeScript benchmark case is a Node.js program', ['src/toolchain/benchmarks/cases/fib/fib.ts'], false, false, true, false],
  ['symbolic execution app and engine gates', ['scripts/hunt/src/sweep.ospml', 'src/toolchain/crates/osprey-cli/tests/hunt_engine.rs'], true, false, false, true],
  ['GPU regression in both flavors', ['src/osprey/tests/core/gpu/scalar_contracts.test.osp', 'src/osprey/tests/core/gpu/scalar_contracts.test.ospml'], true, false, false, true],
  ['proof kernel sources are compiler code', ['src/osprey/examples/proof/kernel/main.ospml', 'src/osprey/examples/proof/kernel/expectedoutput'], true, false, false, true],
  ['proof speed benchmark harness and cases', ['src/toolchain/benchmarks/proof/run.sh', 'src/toolchain/benchmarks/proof/m0_identity/id_imp.lean'], false, false, true, false],
  ['benchmark case program only', ['src/toolchain/benchmarks/cases/primes/primes.osp'], false, false, true, true],
  ['book example program only', ['docs/book/examples/chapter-04/main.osp'], false, true, false, true],
  ['book prose only', ['docs/book/manuscript/04.md'], false, true, false, false],
  ['benchmark harness script only', ['src/toolchain/benchmarks/run.sh'], false, false, true, false],
]
for (const [label, files, code, book, benchmarks, programs] of cases) {
  test(label, () => assert.deepEqual(classify(files), { code, website: false, book, benchmarks, programs }))
}

test('website content skips compiler checks but the embedded grammar never does', () => {
  assert.deepEqual(classify(['src/toolchain/website/src/benchmarks.md']), { code: false, website: true, book: false, benchmarks: false, programs: false })
  assert.equal(classify(['src/toolchain/website/src/js/osprey-grammar.mjs']).code, true)
})

test('mixed content and compiler changes retain every applicable check', () => {
  assert.deepEqual(classify(['docs/book/book.json', 'src/toolchain/benchmarks/run.sh', 'src/toolchain/compiler/runtime/memory.c']),
    { code: true, website: false, book: true, benchmarks: true, programs: false })
})

test('all workflows use the shared classifier and content checks remain required', () => {
  const windows = readFileSync('.github/workflows/ci-windows.yml', 'utf8')
  assert.equal(ciText.split('uses: ./.github/actions/detect-ci-scope').length - 1, 2)
  assert.match(windows, /uses: \.\/\.github\/actions\/detect-ci-scope/)
  assert.match(ciText, /python3 src\/toolchain\/benchmarks\/test_merge_results.py --tooling-only/)
  assert.match(ciText, /make -C docs\/book check-examples/)
  assert.match(ciText, /make -C docs\/book check\n/)
})

// [CI-PROGRAMS-COMPILE] A PR changing only Osprey programs (book examples,
// benchmark cases) is not a code change, yet the compile-everything gate must
// still compile it: both gating jobs read the programs output.
test('build and check-programs gate on the programs output too', () => {
  for (const name of ['build', 'check-programs']) {
    const conditional = ciJob(name).match(/^    if: (.+)$/m)[1]
    assert.match(conditional, /needs\.changes\.outputs\.programs/, name)
    assert.match(conditional, /needs\.changes\.outputs\.code == 'true'/, name)
  }
  assert.match(ciText, /^      programs: \$\{\{ steps\.filter\.outputs\.programs \}\}$/m)
})

test('extension tests receive and reuse the build job\'s complete bundle', () => {
  for (const payload of ['target/osprey-pkg', 'target/package-workspace.wasm', 'target/workbench.wasm']) {
    assert.match(buildJob, new RegExp(`^ +${payload.replaceAll('.', '\\.')}\\s*$`, 'm'), payload)
  }
  const prerequisites = testsMk.match(/^_test_vscode_extension:(.*)$/m)?.[1] ?? ''
  assert.match(prerequisites, /\b_vsix_verify\b/)
  assert.doesNotMatch(prerequisites, /\b_vsix_bundle\b/)
  assert.ok(testRecipe.indexOf('$(MAKE) _vsix_bundle') >= 0)
  assert.ok(testRecipe.indexOf('$(MAKE) _vsix_bundle') < testRecipe.indexOf('$(MAKE) _test_vscode_extension'))
  for (const required of [/name: Restore bundled extension assets/, /chmod \+x[^\n]*target\/osprey-pkg/,
    /chmod \+x[^\n]*src\/toolchain\/vscode-extension\/bin\/linux-x64\/osprey-pkg/]) assert.match(extensionJob, required)
})

test('extension source fixtures have the WASI linker and sysroot before coverage', () => {
  const beforeCoverage = extensionJob.split('- name: Extension suite with V8 coverage')[0]
  assert.match(beforeCoverage, /apt-get install[^\n]*\blld\b/)
  assert.match(beforeCoverage, /wasi-sysroot-\$\{WASI_SDK_VERSION\}/)
  assert.match(beforeCoverage, /test -d .*lib\/wasm32-wasip1/)
  assert.match(beforeCoverage, /OSPREY_WASI_SYSROOT=.*>> "\$GITHUB_ENV"/)
  assert.match(buildJob, /compiler\/bin\/\*\.a/)
})

// The bundle stages src/osprey/workbench/assets/. and src/osprey/workbench/lessons/media/.
// Their *.png, *.gif and *.mp4 are LFS-stored, so a fetch that omits either path
// stages a pointer and the packaged extension ships bytes the Marketplace (and
// the client verifier) reject. The logo and every GIF the VSIX bundles from
// media/ are LFS-stored too, so each job fetches exactly the paths its bundle stages.
test('LFS fetches cover every workbench source asset the bundle stages', () => {
  const required = ['src/toolchain/website/src/assets/images/logo.png', 'src/toolchain/vscode-extension/media/**',
    'src/osprey/workbench/assets/**', 'src/osprey/workbench/lessons/media/**']
  for (const [name, file] of [['test-extension', CI], ['vsix', '.github/workflows/release.yml']]) {
    const include = job(name, file).match(/git lfs pull --include="([^"]+)"/)?.[1] ?? ''
    for (const path of required) assert.ok(include.split(',').includes(path), `${name}: LFS fetch omits ${path}`)
  }
})

test('the required macOS gate runs every Apple part, the native Workbench host among them', () => {
  const apple = ciJob('test-apple')
  for (const command of ['make -C src/toolchain/targets/ios/tests examples\n', 'make -C src/toolchain/targets/ios/tests inbox\n', 'make -C src/toolchain/targets/ios/tests tally-headless\n', 'make -C src/toolchain/targets/macos/tests examples tally-headless tally', 'make -C src/toolchain/targets/ios/tests bank',
    'make -C src/toolchain/targets/macos/tests _workbench_interactions', 'make -C src/toolchain/targets/macos/tests _workbench_memory']) {
    assert.ok(apple.includes(command), command)
  }
  // The Workbench part builds the compiler and both runtimes before it opens, exactly as its own host build did.
  assert.ok(apple.includes('make _runtime _runtime_macos'), 'make _runtime _runtime_macos')
  assert.ok(apple.includes('cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli'), 'cargo build osprey-cli')
  assert.match(apple, /^ {8}run: \$\{\{ matrix\.run \}\}$/m)
  assert.match(apple, /^ {10}OSPREY_BIN: \$\{\{ github\.workspace \}\}\/target\/release\/osprey$/m)
  assert.doesNotMatch(apple, /continue-on-error/)
  // The three iOS parts use macos-26 and its iOS SDK; Inbox and Tally each build their own host.
  assert.match(apple, /^    runs-on: \$\{\{ matrix\.runner \|\| 'macos-15' \}\}$/m)
  assert.match(apple, /^ {10}- part: iOS\n {12}runner: macos-26$/m)
  assert.match(apple, /^ {10}- part: iOS Inbox\n {12}runner: macos-26\n {12}minutes: 22\n {12}run: make -C src\/toolchain\/targets\/ios\/tests inbox$/m)
  assert.match(apple, /^ {10}- part: iOS Tally\n {12}runner: macos-26\n {12}minutes: 22\n {12}run: make -C src\/toolchain\/targets\/ios\/tests tally-headless$/m)
  assert.equal(apple.match(/^ {12}runner: /gm).length, 3, 'only the iOS parts leave macos-15')
  assert.match(apple, /^ {10}lfs: true$/m)
  assert.match(ciJob('ci'), /^ {6}- test-apple$/m)
})

test('the required macOS gate runs every Apple part, the native Workbench host among them (extra: the Workbench check drives the real SwiftUI host with no window)', () => {
  const recipe = readFileSync('src/toolchain/targets/macos/tests/Makefile', 'utf8')
  // It builds only the compiler the CI step already built, never the root `build` (the whole
  // workspace, KLEE, the Zed extension), which recompiled the compiler and cost the part its time.
  assert.match(recipe, /^_workbench_linked: _runtime _runtime_macos$/m)
  assert.match(recipe, /^workbench: _workbench_linked$/m)
  assert.match(recipe, /^_workbench_interactions: _workbench_linked$/m)
  assert.match(recipe, /^_workbench_memory: _workbench_linked$/m)
  const full = recipe.match(/^workbench:.*(?:\n\t.*)*/m)?.[0]
  assert.match(full, /--filter WorkbenchLayout$/m)
  assert.doesNotMatch(full, /--skip/)
  assert.match(recipe, /^\tcargo build --manifest-path src\/toolchain\/Cargo\.toml --release -p osprey-cli$/m)
  assert.match(recipe, /^\tOSPREY_BIN="\$\(CURDIR\)\/\$\(BIN\)" OSPREY_SWIFT_APP=src\/osprey\/workbench bash src\/toolchain\/hosts\/swift\/view\.sh --open-check$/m)
  // Every Swift test runs once: the Workbench suites but the memory soak, the soak in its own part,
  // and the rest under agent-headless.
  assert.match(recipe, /^WORKBENCH_SOAK = WorkbenchLayoutTests\/testActualApplicationImportDispatchAndCancellationMemoryPlateau\$\$$/m)
  assert.match(recipe, /^\tswift test --package-path src\/toolchain\/hosts\/swift --filter WorkbenchLayout --skip '\$\(WORKBENCH_SOAK\)'$/m)
  assert.match(recipe, /^\tswift test --package-path src\/toolchain\/hosts\/swift --filter '\$\(WORKBENCH_SOAK\)'$/m)
  assert.match(recipe, /^\tswift test --package-path src\/toolchain\/hosts\/swift --skip WorkbenchLayout$/m)
  // The gate's Workbench parts are the only places those recipes run, so a windowed host can never slip into CI.
  assert.match(ciText, /^ {14}make -C src\/toolchain\/targets\/macos\/tests _workbench_interactions$/m)
  assert.match(ciText, /^ {14}make -C src\/toolchain\/targets\/macos\/tests _workbench_memory$/m)
  assert.match(ciText, /- part: Workbench memory\n {12}minutes: 15\n/)
  assert.match(ciText, /^ {12}target\/orchestra\/diagnostics\/\*\.jsonl$/m)
  assert.doesNotMatch(ciText, /view\.sh(?!.*--open-check)/)
})

test('the Android emulator is the Pixel 7 the phone goldens were taken on', () => {
  const android = ciJob('test-android')
  assert.match(android, /^ {10}profile: pixel_7$/m)
  assert.ok(android.includes('make -C src/toolchain/targets/android/tests tally-headless'))
  // Goldens are named by width: a golden for any other phone width would mean CI runs another device.
  assert.deepEqual(readdirSync('src/osprey/examples/tally/goldens/android'), ['1080-ready.png'])
})

// [CAST-MOBILE-GPUI] iOS and Android casting are unfinished: no CI job may build, run or test them.
// Plan 0053's last step lifts this once their journeys are green.
test('unfinished mobile casting stays out of every CI path', () => {
  const unfinished = /casting\/(ios|android|mobile)|casting-(ios|android|mobile)|mobile_test|mobile_receiver|mobile-bridge-tests/
  for (const name of readdirSync('.github/workflows')) {
    assert.doesNotMatch(readFileSync(join('.github/workflows', name), 'utf8'), unfinished, name)
  }
  assert.doesNotMatch(readFileSync('scripts/mobile/test-mobile-tools.py', 'utf8'), /src\/osprey\/examples\/casting|src\/toolchain\/targets\/ios\/tests/)
  const adapters = readFileSync('src/toolchain/hosts/gpui/src/adapters.rs', 'utf8')
  assert.match(adapters, /feature = "mobile-bridge-tests"/)
  assert.doesNotMatch(adapters, /target_os = "android", test\)/)
})

test('the coverage gate only compares collected reports and never builds or tests', () => {
  const runs = [...coverageJob.matchAll(/^ {8}run: (.+)$/gm)].map((m) => m[1]).filter((r) => r !== '|')
  assert.deepEqual(runs.filter((r) => r.startsWith('make ')),
    ['make _coverage_check_rust', 'make _coverage_check_vscode_extension', 'make _coverage_check_packages', 'make _coverage_check_c_runtime'])
  assert.doesNotMatch(coverageJob, /cargo |swift |xcodebuild|rust-toolchain|typediagram|ios-test|run\.sh/)
  assert.match(coverageJob, /^    runs-on: ubuntu-latest$/m)
})

test('GPUI smoke and self-tests reuse the app libraries already compiled by the job', () => {
  const workflow = '.github/workflows/gpui.yml'
  for (const [host, preparation] of [['macos', /make _gpui-apps\n/], ['ubuntu', /make _gpui-smoke\n/]]) {
    const hostJob = job(host, workflow)
    assert.match(hostJob, preparation)
    const ready = hostJob.search(preparation)
    const selfTest = hostJob.indexOf('make -o _gpui-apps _gpui-self-test')
    assert.ok(selfTest > ready, `${host} self-test must reuse the earlier build`)
  }
  const planned = spawnSync('make', ['-n', '-o', '_gpui-apps', '_gpui-self-test', 'UNAME_S=Linux'], { encoding: 'utf8' })
  assert.equal(planned.status, 0, planned.stderr)
  assert.match(planned.stdout, /--self-test/)
  assert.doesNotMatch(planned.stdout, /--compile/)
})

test('local CI and hosted GPUI run the same DAW suite once with their built compiler', () => {
  const command = 'make -C src/toolchain/targets/tests synth APP_SUITE_PREBUILT=1'
  assert.equal(makeRecipe('ci').filter((line) => line === '$(MAKE) test').length, 1)
  assert.equal(testRecipe.filter((line) => line === command.replace(/^make/, '$(MAKE)')).length, 1)
  const hosted = job('ubuntu-suites', '.github/workflows/gpui.yml')
  const setup = hosted.indexOf('uses: ./.github/actions/setup-osprey-compiler')
  assert.ok(setup >= 0 && hosted.indexOf(command) > setup)
  assert.doesNotMatch(hosted, /osprey test src\/osprey\/examples\/daw\/test/)
  const planned = spawnSync('make', ['-n', ...command.split(' ').slice(1)], { encoding: 'utf8' })
  assert.equal(planned.status, 0, planned.stderr)
  assert.match(planned.stdout, /SUITES=src\/osprey\/examples\/daw\/test/)
  assert.match(planned.stdout, /target\/release\/osprey test \$suite \|\| exit 1/)
  assert.doesNotMatch(planned.stdout, /cargo build|clang |gcc /)
})

test('coverage identifies failed or skipped report producers before downloading artifacts', () => {
  const producers = coverageJob.match(/^    needs: \[changes, (.+)\]$/m)[1].split(', ')
  const preflight = coverageJob.split('      - uses: actions/checkout@v7')[0]
  const command = preflight.match(/^        run: \|\n([\s\S]+)$/m)[1]
  const results = Object.fromEntries(producers.map((producer) => [producer, { result: 'success' }]))
  const run = (overrides) => spawnSync('bash', ['-e', '-c', command], {
    env: { ...process.env, NEEDS: JSON.stringify({ ...results, ...overrides }) }, encoding: 'utf8',
  })
  assert.equal(run({}).status, 0)
  const failed = run({ 'test-c-runtime': { result: 'skipped' }, 'test-rust': { result: 'failure' },
    'test-registry-arc': { result: 'cancelled' } })
  assert.equal(failed.status, 1)
  assert.match(failed.stdout, /test-c-runtime: skipped/)
  assert.match(failed.stdout, /test-rust: failure/)
  assert.match(failed.stdout, /test-registry-arc: cancelled/)
})

const stagedBundleFixture = (change) => runStagedBundleFixture(extensionJob, change)

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
    // [PACKAGE-LOCAL-REGISTRY] The VSIX carries the registry server and never packages.
    const packaged = stagedBundleFixture((root, platform) => {
      mkdirSync(join(root, platform, 'registry/@osprey/testing'), { recursive: true })
      writeFileSync(join(root, platform, 'registry/@osprey/testing/osprey.toml'), 'name = "@osprey/testing"\n')
    })
    assert.notEqual(packaged.status, 0, 'a bundle carrying registry packages accepted')
    assert.match(packaged.stderr, /The VSIX bundles no packages/)
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

test('malformed detection input fails the gate instead of skipping required jobs', () => withTempDir('ci-scope', (temporary) => {
  const env = { ...process.env, GITHUB_OUTPUT: join(temporary, 'output') }
  const run = (input) => spawnSync(process.execPath, ['scripts/ci/ci-scope.mjs'], { env, input })
  assert.notEqual(run('docs/book/book.json').status, 0)
  const paths = Array.from({ length: 8000 }, (_, index) => `docs/book/manuscript/chapter ${index}.md`)
  assert.equal(run(`${paths.join('\0')}\0`).status, 0)
  assert.match(readFileSync(env.GITHUB_OUTPUT, 'utf8'), /^code=false\nwebsite=false\nbook=true\nbenchmarks=false\nprograms=false\n$/)
}))

function rustCoverageFixture(entries, covered = 100) {
  return withTempDir('rust-coverage', (root) => {
    for (const name of ['one', 'two']) {
      mkdirSync(join(root, 'src', 'toolchain', 'crates', name), { recursive: true })
      writeFileSync(join(root, 'src', 'toolchain', 'crates', name, 'Cargo.toml'), `[package]\nname = "${name}"\n`)
    }
    writeFileSync(join(root, 'thresholds.json'), JSON.stringify({ projects: { rust: entries } }))
    writeFileSync(join(root, 'lcov.info'), ['one', 'two'].map(name =>
      `SF:${root}/crates/${name}/src/lib.rs\nLF:100\nLH:${covered}\nend_of_record\n`).join(''))
    return spawnSync('make', ['-f', resolve('scripts/make/tests.mk'), '_coverage_check_rust',
      'COVERAGE_THRESHOLDS_FILE=thresholds.json', 'RUST_LCOV=lcov.info'], { cwd: root, encoding: 'utf8' })
  })
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
  for (const target of ['_test_packages', '_coverage_check_packages', '_test_python_app_ports', '_conformance-gc',
    '_conformance-arc', '_test_profiler', '_test_vscode_extension', '_coverage_check_vscode_extension', '_test_bench_tools']) {
    assert.equal(testRecipe.filter((line) => line === `$(MAKE) ${target}`).length, 1, target)
  }
  const check = makeRecipe('_coverage_check_packages')
  assert.equal(check.length, 1, 'checking collected coverage must not run unrelated suites')
  assert.match(check[0], /^python3 scripts\/packages\/package_coverage\.py /)
  assert.doesNotMatch(check[0], /--collect/)
})

test('the KLEE engine gates stay out of CI and make ci; the mock oracle and known-bug target stay in the tree', () => {
  // Owner decision 2026-10-26: the engine gates do not guarantee a green
  // signal while issue #306 is open (clean.osp never completes inside the
  // pinned budget), so CI runs none of them, and neither does `make ci`.
  // Verification is by hand:
  // make _test_hunt / make _test_hunt_clean / make _test_hunt_crashes.
  assert.ok(testRecipe.includes('$(MAKE) _test_hunt_mock'))
  assert.deepEqual(testRecipe.filter((command) => /_test_hunt(?!_mock)/.test(command)), [])
  assert.doesNotMatch(readFileSync('Makefile', 'utf8').match(/^ci:.*$/m)[0], /hunt/)
  assert.doesNotMatch(ciText, /brew install klee/)
  assert.doesNotMatch(ciText, /make _test_hunt(?!_mock)/)
  assert.doesNotMatch(ciText, /make _runtime_klee/)
  assert.doesNotMatch(ciText, /_test_hunt_crashes/)
  assert.match(ciJob('test-c-runtime'), /make _test_hunt_mock/)
  assert.match(testsMk, /^_test_hunt_clean:/m)
  const engine = readFileSync('src/toolchain/crates/osprey-cli/tests/hunt_engine.rs', 'utf8')
  assert.match(engine, /const KNOWN_BUGS: \[&str; 1\] = \["clean"\];/)
  for (const control of ['clean.osp', 'clean.expected']) assert.ok(readFileSync(`src/osprey/tests/hunt/${control}`).length > 0)
  assert.match(engine, /const BACKENDS: \[&str; 2\] = \["gc", "arc"\];/)
  assert.ok(readFileSync('src/osprey/tests/hunt/crash.expected').length > 0)
})

test('the Rust shards run every test: each name the rest skips, another shard selects', () => {
  assert.match(rustJob, /run: make _test_rust_shard RUST_SHARD=\$\{\{ matrix\.shard \}\}/)
  assert.match(rustJob, /shard: \[packages, workbench, rest\]/)
  assert.match(testsMk, /cargo llvm-cov --manifest-path src\/toolchain\/Cargo\.toml --workspace --profile ci --lcov/)
  assert.doesNotMatch(testsMk, /cargo llvm-cov[^\n]*(?:--exclude|--skip)/)
  assert.match(testsMk, /^RUST_SHARD_ARGS_packages := --test python_packages -- \$\(RUST_SHARD_PACKAGES\)$/m)
  assert.match(testsMk, /^RUST_SHARD_ARGS_workbench := --test generic_effect_arc_result -- \$\(RUST_SHARD_WORKBENCH\)$/m)
  assert.match(testsMk, /^RUST_SHARD_ARGS_rest := -- \$\(addprefix --skip ,\$\(RUST_SHARD_PACKAGES\) \$\(RUST_SHARD_WORKBENCH\)\)$/m)
})

test('the coverage gate reads the merge of every Rust shard', () => {
  const shards = rustJob.match(/shard: \[([^\]]+)\]/)[1].split(', ')
  assert.match(coverageJob, /pattern: coverage-rust-\*/)
  const merge = coverageJob.match(/python3 scripts\/ci\/merge_lcov\.py target\/coverage\/rust\/\{([^}]+)\}\/lcov\.info > target\/coverage\/lcov\.info/)
  assert.ok(merge, 'the merge must name every shard explicitly')
  assert.deepEqual(merge[1].split(','), shards.map(shard => `coverage-rust-${shard}`))
})

registerLintContracts(ciJob('analyse'))

test('package source coverage runs independently and blocks the required coverage gate', () => {
  const packages = ciJob('test-packages')
  assert.match(packages, /needs: \[changes, build\]/)
  assert.match(packages, /if:.*needs\.changes\.outputs\.code/)
  assert.match(packages, /run: make _test_packages/)
  assert.match(packages, /name: Upload package coverage\n\s+if: always\(\)/)
  assert.match(packages, /name: coverage-packages/)
  assert.match(packages, /if-no-files-found: error/)
  assert.match(coverageJob, /needs:.*\btest-packages\b/)
})

test('C runtime coverage consumes reports from a single instrumented test run', () => {
  const runtime = ciJob('test-c-runtime')
  assert.match(buildJob, /make -j2 _build_c_runtime_tests/)
  assert.match(buildJob, /name: c-runtime-tests/)
  assert.match(runtime, /name: c-runtime-tests/)
  assert.match(runtime, /make -j2 _test_c_runtime/)
  assert.doesNotMatch(runtime, /_build_c_runtime_tests|_coverage_check_c_runtime/)
  assert.match(runtime, /name: coverage-c-runtime/)
  assert.match(coverageJob, /needs:.*\btest-c-runtime\b/)
  assert.match(coverageJob, /name: coverage-c-runtime/)
  assert.match(coverageJob, /run: make _coverage_check_c_runtime/)
})

test('application ports have an independent complete backend gate', () => {
  const apps = ciJob('test-python-app-ports')
  assert.match(apps, /needs: \[changes, build\]/)
  assert.match(apps, /if:.*needs\.changes\.outputs\.code/)
  assert.match(apps, /run: make _test_python_app_ports/)
})

registerProofContracts(ciJob('test-proof'))

registerRegistryContracts()

test('every release publishes only after an unconditional dependency audit', () => {
  assert.match(job('audit'), /uses: \.\/\.github\/workflows\/ci-audit\.yml/)
  assert.doesNotMatch(job('audit'), /^    if:/m)
  for (const name of ['github-release', 'brew', 'scoop', 'vsix', 'publish-marketplace',
    'publish-openvsx', 'deploy-site', 'deploy-webcompiler']) {
    assert.match(job(name), /needs:.*\baudit\b/, `${name} must wait for the audit`)
  }
})

function fixtureGit(directory, ...args) {
  const result = spawnSync('git', ['-c', 'core.hooksPath=/dev/null', ...args], { cwd: directory, encoding: 'utf8' })
  assert.equal(result.status, 0, result.stderr)
  return result.stdout.trim()
}

function commitFixture(directory, subject) {
  fixtureGit(directory, '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', subject, '--allow-empty')
}

// A disposable directory per test: the body runs, then the directory is removed however it ends.
const withTempDir = (slug, body) => {
  const directory = mkdtempSync(join(tmpdir(), `osprey-${slug}-`))
  try { return body(directory) } finally { rmSync(directory, { recursive: true, force: true }) }
}

// A fresh repository with the CI-local files ignored and one initial commit of the given paths.
function initFixtureRepo(directory, ...paths) {
  fixtureGit(directory, 'init', '-q')
  writeFileSync(join(directory, '.gitignore'), 'target/\nci-step.sh\n')
  fixtureGit(directory, 'add', ...paths)
  commitFixture(directory, 'initial')
}

function runLocalCi(directory, audit) {
  writeFileSync(join(directory, 'ci-step.sh'), [
    'case "$1" in', `  audit) ${audit} ;;`, '  lint|test) printf "ran %s\\n" "$1" ;;', 'esac',
  ].join('\n'))
  return spawnSync('make', ['-j4', '-I', resolve('.'), '-f', resolve('Makefile'), 'MAKE=sh ./ci-step.sh', 'ci'],
    { cwd: directory, encoding: 'utf8' })
}

test('a failed local dependency audit invalidates a previous green push stamp', () => withTempDir('push-gate', (dir) => {
  initFixtureRepo(dir, '.gitignore')
  mkdirSync(join(dir, 'target'))
  const stamp = join(dir, 'target/ci-green')
  writeFileSync(stamp, 'previously tested commit')
  const result = runLocalCi(dir, 'echo fixture-audit-failure >&2; false')
  assert.notEqual(result.status, 0)
  assert.match(result.stderr, /fixture-audit-failure/)
  assert.doesNotMatch(result.stdout, /ran (lint|test)/)
  assert.throws(() => readFileSync(stamp), { code: 'ENOENT' })
}))

test('local CI stamps a clean commit and rejects changes before or during its run', () => withTempDir('ci-source', (directory) => {
  const stamp = join(directory, 'target/ci-green')
  const source = join(directory, 'source.osp')
  writeFileSync(source, 'fn main() = 0\n')
  initFixtureRepo(directory, '.')
  const run = (audit) => runLocalCi(directory, audit)
  const clean = run('echo ran audit')
  assert.equal(clean.status, 0, clean.stderr)
  assert.deepEqual(clean.stdout.match(/^ran \w+$/gm), ['ran audit', 'ran lint', 'ran test'])
  assert.equal(readFileSync(stamp, 'utf8').trim(), fixtureGit(directory, 'rev-parse', 'HEAD'))
  writeFileSync(source, 'fn main() = 1\n')
  const dirty = run('echo audit-ran')
  assert.notEqual(dirty.status, 0)
  assert.doesNotMatch(dirty.stdout, /audit-ran/)
  assert.throws(() => readFileSync(stamp), { code: 'ENOENT' })
  fixtureGit(directory, 'restore', 'source.osp')
  assert.notEqual(run('touch untracked.osp').status, 0)
  assert.throws(() => readFileSync(stamp), { code: 'ENOENT' })
  rmSync(join(directory, 'untracked.osp'))
  assert.notEqual(run('git -c user.name=Fixture -c user.email=fixture@example.invalid commit -qm later --allow-empty').status, 0)
  assert.throws(() => readFileSync(stamp), { code: 'ENOENT' })
}))

test('the push gate sends the tested commit if a teammate advances the branch during the CI check', () => withTempDir('push-race', (directory) => {
  const push = resolve('scripts/ci/push.sh')
  const git = (...args) => fixtureGit(directory, ...args)
  git('init', '-q', '-b', 'feature')
  git('init', '-q', '--bare', 'remote')
  git('remote', 'add', 'origin', './remote')
  for (const subject of ['tested', 'untested']) {
    commitFixture(directory, subject)
  }
  const tested = git('rev-parse', 'HEAD^')
  const untested = git('rev-parse', 'HEAD')
  git('update-ref', 'refs/heads/feature', tested)
  mkdirSync(join(directory, 'target'))
  writeFileSync(join(directory, 'target/ci-green'), `${tested}\n`)
  writeFileSync(join(directory, 'gh'), `#!/bin/sh\ngit update-ref refs/heads/feature ${untested} && echo 0\n`, { mode: 0o755 })
  const result = spawnSync('sh', [push], {
    cwd: directory, env: { ...process.env, PATH: `${directory}:${process.env.PATH}` }, encoding: 'utf8',
  })
  assert.equal(result.status, 0, result.stderr)
  assert.equal(git('--git-dir=remote', 'rev-parse', 'refs/heads/feature'), tested)
  assert.equal(git('rev-parse', 'HEAD'), untested)
}))

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
  const gate = ciJob('ci')
  assert.match(gate, /^ {4}name: CI\s*$/m)
  assert.match(gate, /if: \$\{\{ always\(\) \}\}/)
  const needs = [...gate.matchAll(/^ {6}- ([\w-]+)\s*$/gm)].map((m) => m[1])
  const jobs = ciText.split(/^jobs:\s*$/m)[1]
  const others = [...jobs.matchAll(/^ {2}([\w-]+):\s*$/gm)].map((m) => m[1]).filter((key) => key !== 'ci')
  assert.deepEqual([...needs].sort(), [...others].sort())
  for (const called of ['ci-windows.yml', 'gpui.yml', 'winui.yml']) {
    assert.ok(ciText.includes(`uses: ./.github/workflows/${called}`), called)
  }
  const beside = readdirSync('.github/workflows').filter((f) => f !== 'ci.yml' && /\.ya?ml$/.test(f))
    .filter((f) => targetsMainPullRequests(readFileSync(`.github/workflows/${f}`, 'utf8')))
  assert.deepEqual(beside, [])
  const guard = readFileSync('scripts/ci/verify-branch-protection.mjs', 'utf8')
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

test('the detect-ci-scope action exposes every scope key classify returns', () => {
  const action = readFileSync('.github/actions/detect-ci-scope/action.yml', 'utf8')
  for (const key of Object.keys(classify(['x']))) {
    assert.match(action, new RegExp(`^  ${key}:\\n[^\\n]*\\n    value: \\$\\{\\{ steps\\.scope\\.outputs\\.${key} \\}\\}$`, 'm'))
  }
})
