// The three `_lint` commands no other workflow runs, and the guard that keeps
// them reachable. `_lint` (scripts/make/tests.mk) has twelve steps; the
// required analysis job re-implements them inline, and these three were the
// only ones absent from every workflow — so a licence-bundle or dead-code
// regression could not fail a merge. The caller passes the `build` job text,
// so this file needs no workflow parser of its own.
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { readFileSync, readdirSync, mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'

const LINT_COMMANDS = ['node scripts/ci/verify-no-dead-code.mjs',
  'node scripts/packages/verify-package-licenses.mjs', 'node --test scripts/packages/test-package-licenses.mjs']

test('Zed builds in phase one and tests the cached compiler once in phase two', () => {
  const workflow = readFileSync('.github/workflows/ci.yml', 'utf8')
  const build = workflow.split('\n  build:')[1].split('\n  analyse:')[0]
  const extension = workflow.split('\n  test-extension:')[1].split('\n  test-wasm:')[0]
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
  const workflow = readFileSync('.github/workflows/ci.yml', 'utf8')
  const wasm = workflow.split('\n  test-wasm:')[1].split('\n  test-website:')[0]
  assert.doesNotMatch(wasm, /setup-osprey-compiler|libc6-dbg|lldb/)
  assert.match(wasm, /apt-get install -y clang llvm lld wabt zsh/)
  assert.match(wasm, /timeout-minutes: 6/)
  assert.match(wasm, /name: osprey-build/)
  assert.match(wasm, /make _runtime_wasm/)
  assert.match(wasm, /wasm-validate .*hello\.wasm/)
  assert.match(wasm, /for flavor in osp ospml/)
  assert.match(wasm, /wasm-browser-smoke\.mjs/)
  assert.match(wasm, /make _test_wasm_goldens/)
})

test('Only the Linux profiler suite installs glibc debug symbols', () => {
  const setup = readFileSync('.github/actions/setup-osprey-compiler/action.yml', 'utf8')
  const workflow = readFileSync('.github/workflows/ci.yml', 'utf8')
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
  const workflow = readFileSync('.github/workflows/ci.yml', 'utf8')
  const analyse = workflow.split('\n  analyse:')[1].split('\n  check-programs:')[0]
  assert.match(analyse, /uses: \.\/\.github\/actions\/setup-osprey-compiler\n        with:\n          build: 'false'/)
})

test('Website E2E reuses the current build and can still run when it is skipped', () => {
  const workflow = readFileSync('.github/workflows/ci.yml', 'utf8')
  const website = workflow.split('\n  test-website:')[1].split('\n  test-integration:')[0]
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

function deadCodeFixture(product, tests) {
  const root = mkdtempSync(join(tmpdir(), 'osprey-dead-code-'))
  try {
    mkdirSync(join(root, 'scripts/ci'), { recursive: true })
    mkdirSync(join(root, 'crates/fixture/src'), { recursive: true })
    writeFileSync(join(root, 'scripts/ci/check.mjs'), readFileSync('scripts/ci/verify-no-dead-code.mjs'))
    writeFileSync(join(root, 'crates/fixture/src/main.rs'), product)
    writeFileSync(join(root, 'crates/fixture/src/tests.rs'), tests)
    return spawnSync(process.execPath, [join(root, 'scripts/ci/check.mjs')], { encoding: 'utf8' })
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
