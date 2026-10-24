import assert from 'node:assert/strict'
import { readFileSync, mkdtempSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawnSync } from 'node:child_process'
import { test } from 'node:test'
import { classify } from './ci-scope.mjs'

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
const job = (name) => workflowJobs('.github/workflows/release.yml').find((block) => block.startsWith(`  ${name}:`)) ?? ''

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
