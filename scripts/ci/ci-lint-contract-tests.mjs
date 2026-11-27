// The three `_lint` commands no other workflow runs, and the guard that keeps
// them reachable. `_lint` (scripts/make/tests.mk) has twelve steps; the
// required analysis job re-implements them inline, and these three were the
// only ones absent from every workflow — so a licence-bundle or dead-code
// regression could not fail a merge. The caller passes the `build` job text,
// so this file needs no workflow parser of its own.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync, readdirSync, mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { job, workflowText } from './ci-workflow-text.mjs'
import { withTempDir } from './ci-bundle-fixture.mjs'

const ciFile = '.github/workflows/ci.yml'
const workflow = workflowText(ciFile)

const LINT_COMMANDS = ['node scripts/ci/verify-no-dead-code.mjs',
  'node scripts/packages/verify-package-licenses.mjs', 'node --test scripts/packages/test-package-licenses.mjs']

test('Zed builds in phase one and tests the cached compiler once in phase two', () => {
  const build = job('build', ciFile)
  const extension = job('test-extension', ciFile)
  assert.match(build, /rustup target add wasm32-wasip2/)
  assert.match(build, /make -C src\/toolchain\/zed-extension build/)
  assert.match(build, /target\/wasm32-wasip2\/debug\/osprey_zed\.wasm/)
  assert.doesNotMatch(build, /zed-extension (?:test|stdio-test)/)
  assert.equal(workflow.match(/make -C src\/toolchain\/zed-extension stdio-test/g)?.length, 1)
  assert.match(extension, /name: osprey-build/)
  assert.match(extension, /make -C src\/toolchain\/zed-extension stdio-test/)
  assert.doesNotMatch(extension, /zed-extension (?:build|test)\b/)
})

test('WASM tests use only their toolchain and retain every acceptance step', () => {
  const wasm = job('test-wasm', ciFile)
  assert.doesNotMatch(wasm, /setup-osprey-compiler|libc6-dbg|lldb/)
  assert.match(wasm, /apt-get install -y clang llvm lld wabt zsh/)
  assert.match(wasm, /timeout-minutes: 6/)
  assert.match(wasm, /name: osprey-build/)
  assert.match(wasm, /make _runtime_wasm/)
  assert.match(wasm, /wasm-validate .*hello\.wasm/)
  assert.match(wasm, /for flavor in osp ospml/)
  assert.match(wasm, /wasm-browser-smoke\.mjs/)
  assert.match(wasm, /make _test_wasm_goldens/)
  const doctests = job('test-wasm-doctests', ciFile)
  assert.match(doctests, /timeout-minutes: 6/)
  assert.match(doctests, /\*install-wasm-toolchain/)
  assert.match(doctests, /\*download-osprey-build/)
  assert.match(doctests, /\*build-wasm-runtime/)
  assert.match(doctests, /make _test_wasm_doctests/)
  assert.match(workflow, /- test-wasm-doctests\n/)
})

test('Only the Linux profiler suite installs glibc debug symbols', () => {
  const setup = readFileSync('.github/actions/setup-osprey-compiler/action.yml', 'utf8')
  assert.doesNotMatch(setup, /(?:apt_install|apt-get install)[^\n]*libc6-dbg/)
  assert.match(workflow, /- name: Profiler suite\n        run: \|\n[\s\S]*?install -y libc6-dbg\n          make _test_profiler/)
})

// The staged GPUI dependency sources come from pinned archives; a cached copy from an older
// inventory fails dependencies.py's integrity check, so the cached target never carries them.
test('the compiler setup restores no staged GPUI dependency sources', () => {
  const setup = readFileSync('.github/actions/setup-osprey-compiler/action.yml', 'utf8')
  const cache = setup.indexOf('- name: Cache cargo registry + target')
  const drop = setup.indexOf('- name: Drop cached GPUI dependency sources')
  assert.ok(cache >= 0 && drop > cache, 'the drop step follows the cargo cache restore')
  assert.match(setup.slice(drop), /^ {6}shell: bash\n {6}run: rm -rf target\/gpui-dependencies$/m)
  for (const name of readdirSync('.github/workflows')) {
    const workflow = readFileSync(join('.github/workflows', name), 'utf8')
    if (/_gpui-|gpui\/run\.sh/.test(workflow)) assert.match(workflow, /uses: \.\/\.github\/actions\/setup-osprey-compiler/, name)
  }
})

test('analysis checks the workspace without rebuilding the release compiler', () => {
  const analyse = job('analyse', ciFile)
  assert.match(analyse, /uses: \.\/\.github\/actions\/setup-osprey-compiler\n        with:\n          build: 'false'/)
})

test('Website E2E reuses the current build and can still run when it is skipped', () => {
  const website = job('test-website', ciFile)
  assert.match(website, /^    needs: build$/m)
  assert.match(website, /^    if: \$\{\{ !cancelled\(\) \}\}$/m)
  assert.match(website, /Download compiler artifacts\n        if: \$\{\{ needs\.build\.result == 'success' \}\}[\s\S]*?name: osprey-build/)
  assert.match(website, /run: npm test\n[\s\S]*?WEBSITE_PREBUILT_COMPILER: \$\{\{ needs\.build\.result == 'success' && '1' \|\| '' \}\}/)
  const makefile = readFileSync('src/toolchain/website/Makefile', 'utf8')
  assert.match(makefile, /ifeq \(\$\(WEBSITE_PREBUILT_COMPILER\),1\)\n\ttest -x "\$\(BIN\)"\nelse\n\tcargo build .*--release -p osprey-cli\nendif/)
})

// A job-shaped block with the contract's own name and detector gate, so an
// evasion fails on the guard and not on an incidental mismatch.
const JOB_HEADER = '  build:\n    name: Build, Format & Analyse\n    needs: changes\n' +
  "    if: ${{ needs.changes.outputs.code == 'true' }}\n    runs-on: ubuntu-latest\n" +
  '    timeout-minutes: 30\n    steps:\n'

const EVASIONS = [
  `      # run: ${LINT_COMMANDS[0]}\n`,
  `      - if: \${{ needs.changes.outputs.code == 'true' }}\n        run: ${LINT_COMMANDS[0]}\n`,
  `      - name: Dead-code gate intact\n        continue-on-error: true\n        run: ${LINT_COMMANDS[0]}\n`,
  '      - name: Dead-code gate intact\n        run: node scripts/ci/verify-node-deps-guard.mjs\n',
  `      - name: Dead-code gate intact\n        run: |\n          run: ${LINT_COMMANDS[0]}\n`,
]

// The step carrying `run: <command>` as its own raw eight-space run line, if
// any. Membership is checked before trimming, and a step that opens a block
// scalar is refused outright, so text inside a `run: |` body is never a run
// line however it is indented; comments and other steps cannot match either.
const OPENS_BLOCK = /^ {6}(?:- )?run: [|>]/
const stepRunning = (build, command) =>
  build.split(/(?=^ {6}- )/m).map((step) => step.split('\n'))
    .find((lines) => lines.includes(`        run: ${command}`) && !lines.some((line) => OPENS_BLOCK.test(line)))

// That step must also carry no `if:` in any spelling (`if:` indented under the
// dash or written on the dash line) and no `continue-on-error`, which would
// swallow the failure instead of failing CI.
const assertUngated = (build, command) => {
  const step = stepRunning(build, command)
  assert.ok(step, `${command} must run as its own run: line`)
  const lines = (step ?? []).map((line) => line.trim())
  assert.ok(!lines.some((line) => /^(?:-\s*)?if:/.test(line)), `${command} must not be individually skipped`)
  assert.ok(!lines.some((line) => line.includes('continue-on-error')), `${command} must not swallow a failure`)
}

const assertBuildRunsLint = (build) => {
  assert.match(build, /^    name: Format & Analyse$/m)
  assert.match(build, /^    if: \$\{\{ needs\.changes\.outputs\.code == 'true' \}\}$/m)
  assert.doesNotMatch(build, /continue-on-error/)
  for (const command of LINT_COMMANDS) assertUngated(build, command)
}

const assertGuardRejectsEvasions = () => {
  for (const evasion of EVASIONS) {
    assert.throws(() => assertUngated(JOB_HEADER + evasion, LINT_COMMANDS[0]), evasion)
  }
}

export const registerLintContracts = (build) => {
  test('the analyse job runs the lint commands no other workflow covers', () => assertBuildRunsLint(build))
  test('the run-line guard rejects a commented, gated, suppressed or missing lint command', assertGuardRejectsEvasions)
}

function deadCodeFixture(product, tests, { nested = false, testFile = 'tests.rs', library = '', descendants = {} } = {}) {
  return withTempDir('dead-code', (root) => {
    mkdirSync(join(root, 'scripts/ci'), { recursive: true })
    const source = join(root, 'src/toolchain/crates/fixture/src')
    mkdirSync(source, { recursive: true })
    writeFileSync(join(root, 'scripts/ci/check.mjs'), readFileSync('scripts/ci/verify-no-dead-code.mjs'))
    const testDir = nested ? join(source, 'env') : source
    mkdirSync(testDir, { recursive: true })
    writeFileSync(join(source, nested ? 'env.rs' : 'main.rs'), product)
    writeFileSync(join(testDir, testFile), tests)
    if (library) writeFileSync(join(source, 'lib.rs'), library)
    for (const [name, contents] of Object.entries(descendants)) {
      const file = join(testDir, name)
      mkdirSync(dirname(file), { recursive: true })
      writeFileSync(file, contents)
    }
    return spawnSync(process.execPath, [join(root, 'scripts/ci/check.mjs')], { encoding: 'utf8' })
  })
}

function assertOrphanWithoutHelper(result, helperName) {
  assert.equal(result.status, 1)
  assert.match(result.stderr, /fn orphan/)
  assert.doesNotMatch(result.stderr, helperName)
}

test('dead-code gate distinguishes test module helpers from unused product code', () => {
  for (const nested of [false, true]) {
    const declaration = nested ? '#[expect(\n    clippy::indexing_slicing\n)]\npub(crate) mod tests;' : 'mod tests;'
    const helper = deadCodeFixture(`pub fn active() {}\n#[cfg(test)]\n${declaration}\nfn main() { active(); }`,
      'pub(crate) fn assembled_program() {}', { nested })
    assert.equal(helper.status, 0, helper.stderr)
  }
  const dead = deadCodeFixture('pub fn orphan() {}\n#[cfg(test)]\nmod tests;\nfn main() {}',
    '#[test]\nfn regression() { crate::orphan(); }')
  assert.equal(dead.status, 1)
  assert.match(dead.stderr, /src\/toolchain\/crates\/fixture\/src\/main.rs:1  fn orphan/)
  for (const declaration of ['#[cfg(test)] mod witness;', '#[cfg(test)]\n#[path = "witness.rs"]\nmod verification;']) {
    const named = deadCodeFixture(`pub fn orphan() {}\n${declaration}\nfn main() {}`,
      'pub fn regression() { crate::orphan(); }', { testFile: 'witness.rs' })
    assertOrphanWithoutHelper(named, /fn regression/)
  }
  const shared = deadCodeFixture('#[cfg(test)] mod witness;\nfn main() {}',
    'pub fn active() {}', { testFile: 'witness.rs', library: 'mod witness;\nfn use_shared() { witness::active(); }' })
  assert.equal(shared.status, 0, shared.stderr)
  const productModule = deadCodeFixture('mod tests;\nfn main() {}', 'pub fn unused_product() {}')
  assert.equal(productModule.status, 1)
  assert.match(productModule.stderr, /fn unused_product/)
  const comment = deadCodeFixture('// #[cfg(test)]\nmod tests;\nfn main() {}', 'pub fn unused_product() {}')
  assert.equal(comment.status, 1)
  const live = deadCodeFixture('pub fn active() {}\nfn main() { active(); }', '')
  assert.equal(live.status, 0, live.stderr)
  assert.match(live.stdout, /1 public items/)
  const missing = deadCodeFixture('fn main() {}', '')
  assert.equal(missing.status, 1)
  assert.match(missing.stderr, /no public Rust declarations/)
})

test('dead-code gate follows test modules through descendants and keeps shared product routes', () => {
  const entry = 'pub fn orphan() {}\n#[cfg(test)] mod witness;\nfn main() {}'
  for (const { shared, gated } of [{}, { shared: true }, { shared: true, gated: true }]) {
    const guard = gated ? '#[cfg(test)] ' : ''
    const result = deadCodeFixture(entry, 'pub mod input;', {
      testFile: 'witness.rs',
      library: shared ? `pub fn orphan() {}\nmod witness;\n${guard}fn run() { witness::input::leaf::fixture(); }` : '',
      descendants: {
        'witness/input.rs': guard + 'pub mod leaf;',
        'witness/input/leaf.rs': 'pub fn fixture() { crate::orphan(); }',
      },
    })
    if (shared && !gated) {
      assert.equal(result.status, 0, result.stderr)
      assert.match(result.stdout, /2 public items/)
    } else {
      assertOrphanWithoutHelper(result, /fn fixture/)
    }
  }
})

test('dead-code gate discards comments and keeps runtime symbols embedded in strings', () => {
  const source = [
    'pub fn orphan() {}',
    '/// orphan()',
    '/* Outer /* inner */',
    'pub fn phantom() {}',
    'orphan */',
    'pub fn native_route() {}',
    'fn llvm() -> &\'static str { r##"// @native_route()"## }',
    'pub fn ffi_route() {}',
    '#[cfg(test)] fn check() { orphan(); }',
  ].join('\n')
  const bridge = ['// orphan \\', 'orphan', 'const char *name = "/* ffi_route */";'].join('\n')
  const result = deadCodeFixture(source, '', { descendants: { 'bridge.c': bridge } })
  assertOrphanWithoutHelper(result, /fn (phantom|native_route|ffi_route)/)
  assert.match(result.stderr, /FAIL: 1 public item/)
})


test('a test-only field cannot hide a later product type reference', () => {
  for (const probe of ['probe: bool,', 'probe: std::collections::HashMap<String,\n        Vec<(usize, usize)>>, // only tests']) {
    const product = `pub type Live = usize;\nstruct Holder {\n    #[cfg(test)]\n    ${probe}\n    payload: Live,\n}\nfn main() {}`
    const result = deadCodeFixture(product, '')
    assert.equal(result.status, 0, result.stderr)
  }
})

const deadCodeMaskControls = [
  ['standalone test attributes cannot keep unused product functions alive',
    'pub fn only_attribute_test() {}\n#[test]\nfn witness() { only_attribute_test(); }\nfn main() {}',
    /fn only_attribute_test/],
  ['split function headers cannot expose where-clause test calls as product references',
    'pub fn only_split_test() {}\n#[cfg(test)]\nfn\n\n witness<T>()\nwhere\n    T: Copy,\n{ only_split_test(); }\nfn main() {}',
    /fn only_split_test/],
  ['same-line test attributes cannot expose where-clause test calls as product references',
    'pub fn only_inline_test() {}\n#[cfg(test)] fn witness<T>()\nwhere\n    T: Copy,\n{ only_inline_test(); }\nfn main() {}',
    /fn only_inline_test/],
  ['test function where-clause commas cannot expose test calls as product references',
    'pub fn only_where_test() {}\n#[cfg(test)]\nfn witness<T>()\nwhere\n    T: Copy,\n{ only_where_test(); }\nfn main() {}',
    /fn only_where_test/],
  ['a test-only field cannot hide a later unused product declaration',
    'struct Holder {\n    #[cfg(test)]\n    probe: bool,\n}\npub fn orphan_after_test_field() {}\nfn main() {}',
    /fn orphan_after_test_field/],
  ['multiline test functions still cannot keep unused product code alive',
    'pub fn only_test() {}\n#[cfg(test)]\nfn witness(\n    callback: fn(usize, usize) -> usize,\n) { only_test(); }\nfn main() {}',
    /fn only_test/],
]
for (const [name, product, missing] of deadCodeMaskControls) {
  test(name, () => {
    const result = deadCodeFixture(product, '')
    assert.equal(result.status, 1, result.stdout)
    assert.match(result.stderr, missing)
  })
}

test('test-only function-pointer tuple fields keep later product types visible', () => {
  const product = 'pub type Live = usize;\nstruct Holder(\n    #[cfg(test)]\n    fn\n    (usize) -> usize,\n    Live,\n);\nfn main() {}'
  const result = deadCodeFixture(product, '')
  assert.equal(result.status, 0, result.stderr)
})
