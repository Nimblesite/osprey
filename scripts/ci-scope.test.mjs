import assert from 'node:assert/strict'
import { readFileSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'
import { classify } from './ci-scope.mjs'
import { targetsMainPullRequests } from './workflow-pr-target.mjs'

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

const makeRecipe = (name) => {
  const commands = []
  let active = false
  for (const line of readFileSync('Makefile', 'utf8').split('\n')) {
    if (line.startsWith(`${name}:`)) { active = true; continue }
    if (active && /^[^\s#][^:]*:/.test(line)) break
    if (active && line.startsWith('\t')) commands.push(line.trim())
  }
  return commands
}

test('package coverage preserves the complete test recipe and checks without rerunning suites', () => {
  const commands = makeRecipe('test')
  for (const target of ['_test_packages', '_coverage_check_packages', '_test_goldens', '_conformance-gc',
    '_conformance-arc', '_test_profiler', '_test_vscode_extension', '_coverage_check_vscode_extension', '_test_bench_tools']) {
    assert.equal(commands.filter((line) => line === `$(MAKE) ${target}`).length, 1, target)
  }
  const check = makeRecipe('_coverage_check_packages')
  assert.equal(check.length, 1, 'checking collected coverage must not run unrelated suites')
  assert.match(check[0], /^python3 scripts\/package_coverage\.py /)
  assert.doesNotMatch(check[0], /--collect/)
})

test('required coverage runs the real Hunt expectations and default-budget regression', () => {
  const coverage = job('coverage', '.github/workflows/ci.yml')
  assert.match(coverage, /make _runtime_klee\s+make _test_hunt\s+python3 scripts\/test-hunt-default-budget\.py\s+make _test_hunt_crashes/)
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

test('every release publishes only after an unconditional dependency audit', () => {
  assert.match(job('audit'), /uses: \.\/\.github\/workflows\/ci-audit\.yml/)
  assert.doesNotMatch(job('audit'), /^    if:/m)
  for (const name of ['github-release', 'brew', 'scoop', 'vsix', 'publish-marketplace',
    'publish-openvsx', 'deploy-site', 'deploy-webcompiler']) {
    assert.match(job(name), /needs:.*\baudit\b/, `${name} must wait for the audit`)
  }
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
})
