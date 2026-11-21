// Holds the preflight to the workflows it reads: every check CI runs is in the local plan, run
// whole with its directory, matrix values filled in, coverage gates last, on the host CI gives it.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { jobHost, matrixRows, plan, runCheck, skipReason, steps, workflowFiles } from './preflight.mjs'

test('a preflight selector must run a check to report green', () => {
  const directory = mkdtempSync(join(tmpdir(), 'osprey-preflight-selector-'))
  try {
    mkdirSync(join(directory, '.github/workflows'), { recursive: true })
    writeFileSync(join(directory, '.github/workflows/ci.yml'), 'jobs:\n')
    mkdirSync(join(directory, 'scripts/make'), { recursive: true })
    writeFileSync(join(directory, 'Makefile'), '')
    const script = fileURLToPath(new URL('./preflight.mjs', import.meta.url))
    const result = spawnSync(process.execPath, [script, '--only=Missing check'], { cwd: directory, encoding: 'utf8' })
    assert.equal(result.status, 1)
    assert.match(result.stderr, /No CI checks match/)
    const otherHost = process.platform === 'win32' ? 'macos-15' : 'windows-latest'
    writeFileSync(join(directory, '.github/workflows/ci.yml'), [
      'jobs:', '  foreign:', `    runs-on: ${otherHost}`, '    steps:',
      '      - name: Native replay', '        run: ./check',
    ].join('\n'))
    const skipped = spawnSync(process.execPath, [script, '--only=Native replay'], { cwd: directory, encoding: 'utf8' })
    assert.equal(skipped.status, 1)
    assert.match(skipped.stdout, /SKIP.*Native replay/)
    assert.match(skipped.stderr, /None of the selected CI checks ran/)
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})

test('local preflight keeps failing commands and mixed output in chronological order', () => {
  const output = mkdtempSync(join(tmpdir(), 'osprey-preflight-'))
  try {
    const host = { darwin: 'macos', win32: 'windows' }[process.platform] ?? 'linux'
    const command = 'echo first; echo diagnostic >&2; echo last; exit 7'
    const result = runCheck({ command, host, env: {}, directory: '.', job: 'fixture', name: 'mixed output' }, 0, output)
    assert.equal(result.status, 'FAIL')
    assert.equal(readFileSync(result.log, 'utf8'), `$ ${command}\nfirst\ndiagnostic\nlast\n`)
  } finally {
    rmSync(output, { recursive: true, force: true })
  }
})

test('preflight keeps distinct environments and hosts for the same command', () => {
  const directory = mkdtempSync(join(tmpdir(), 'osprey-preflight-environment-'))
  try {
    const file = join(directory, 'ci.yml')
    const job = (name, host, mode) => [
      `  ${name}:`, `    runs-on: ${host}`, '    env:', `      MODE: ${mode}`,
      '    steps:', '      - name: Compile', '        env:', '          FORMAT: json',
      '        run: ./check', '',
    ].join('\n')
    writeFileSync(file, 'env:\n  FORMAT: text\n  COLOR: never\njobs:\n'
      + job('gc', 'ubuntu-latest', 'gc') + job('arc', 'ubuntu-latest', 'arc')
      + job('windows', 'windows-latest', 'gc') + job('repeat', 'ubuntu-latest', 'gc'))
    const checks = plan([file])
    assert.deepEqual(checks.map(({ host, env }) => [host, env]), [
      ['linux', { MODE: 'gc', FORMAT: 'json', COLOR: 'never' }],
      ['linux', { MODE: 'arc', FORMAT: 'json', COLOR: 'never' }],
      ['windows', { MODE: 'gc', FORMAT: 'json', COLOR: 'never' }],
    ])
  } finally {
    rmSync(directory, { recursive: true, force: true })
  }
})

const checks = plan()
const commands = checks.map((check) => check.command)
const find = (pattern) => checks.find((check) => pattern.test(check.command))

test('the preflight reads ci.yml and every workflow it calls', () => {
  assert.deepEqual(workflowFiles(), ['.github/workflows/ci.yml', '.github/workflows/ci-windows.yml',
    '.github/workflows/gpui.yml', '.github/workflows/winui.yml', '.github/workflows/ci-audit.yml'])
  assert.ok(commands.includes('make audit'), 'the PR preflight runs the release audit')
})

test('every make check in the PR workflows is planned, inside the step that runs it', () => {
  for (const file of workflowFiles()) {
    for (const [, line] of readFileSync(file, 'utf8').matchAll(/^\s+(?:- )?(?:run: )?(?:xvfb-run -a )?(make [^$\n]+)$/gm)) {
      if (/_test_rust_shard|-install\b/.test(line)) continue // one local run; tool installs are runner setup
      assert.ok(commands.some((command) => command.split('\n').some((text) => text.trim() === line.trim())), `${file}: ${line}`)
    }
  }
  assert.ok(commands.every((command) => !command.includes('${{ matrix')), 'matrix values are filled in')
  const contexts = checks.map(({ directory, command, host, env, ciOnly }) =>
    JSON.stringify([directory, command, host, ciOnly, Object.entries(env).sort()]))
  assert.equal(new Set(contexts).size, checks.length, 'each check runs once in each context')
})

test('checks that are not make targets run too: static analysis, browsers, the emulator and the containers', () => {
  for (const [pattern, directory] of [[/^cargo fmt .* --check$/, '.'], [/^cargo clippy .*-D warnings$/, '.'], [/^npm run lint$/, './src/toolchain/vscode-extension'],
    [/^npm test$/, './src/toolchain/website'], [/^npx playwright test$/, './src/osprey/examples/talon/playwright-e2e-tests'], [/^deslop \./, '.']]) {
    assert.equal(find(pattern)?.directory, directory, String(pattern))
  }
  const android = find(/targets\/android\/tests inbox/)
  assert.ok(android?.command.includes('tally-headless') && android.command.includes(':app:lintDebug'), 'the emulator action script runs whole')
  assert.ok(find(/cd src\/toolchain\/webcompiler/)?.command.includes('test.sh'), 'the web compiler container test runs')
  const orchestra = find(/pocket_orchestra\/test --memory=arc/)
  assert.ok(orchestra?.command.includes('pocket-orchestra-headless'), 'a matrix row runs its whole script')
})

test('runner plumbing is left out and runner-only checks are named as skipped', () => {
  assert.ok(!commands.some((command) => /apt-get|GITHUB_ENV|^chmod \+x target\/|^tar -xf target\//m.test(command)), 'no installs, runner env or artifact restores')
  assert.ok(!commands.some((command) => /if: failure\(\)/.test(command)))
  const coveragePreflight = checks.find(({ job, name }) => job === 'ci.yml:coverage' && name.startsWith('Require successful coverage producers'))
  assert.ok(coveragePreflight)
  assert.equal(skipReason(coveragePreflight), 'reads values only a runner has')
  assert.equal(skipReason(find(/to_entries\[\]/)), 'reads values only a runner has', 'the required check reads the needs context')
  assert.equal(skipReason(find(/benchmarks\/instruction_gate\.py/), 'macos'), 'needs a linux host')
  assert.equal(skipReason(find(/benchmarks\/instruction_gate\.py/), 'linux'), undefined)
})

test('the coverage gates run last, after every test that feeds them', () => {
  const gates = ['make _coverage_check_rust', 'make _coverage_check_vscode_extension', 'make _coverage_check_packages', 'make _coverage_check_c_runtime']
  assert.deepEqual(commands.slice(-gates.length), gates)
  for (const feeder of ['make _test_rust', 'make _test_vscode_extension', 'make _test_packages', 'make -j2 _test_c_runtime']) {
    const at = commands.indexOf(feeder)
    assert.ok(at >= 0 && at < commands.length - gates.length, feeder)
  }
  assert.ok(!commands.some((command) => /_test_rust_shard|merge_lcov/.test(command)), 'the shards are one local run')
})

test('each check keeps the host CI gives it', () => {
  assert.equal(find(/^make -C src\/toolchain\/targets\/ios\/tests examples$/)?.host, 'macos')
  assert.equal(find(/^make _test_registry REGISTRY_MEMORY=arc$/)?.host, 'linux')
  assert.equal(skipReason(find(/^make _test_registry REGISTRY_MEMORY=arc$/), 'macos'), undefined, 'a Mac runs the Linux checks')
  const windows = checks.filter((check) => /^(ci-windows|winui)\.yml/.test(check.job))
  assert.ok(windows.length > 5 && windows.every((check) => skipReason(check, 'macos')), 'a Mac skips every Windows check, naming why')
  assert.equal(skipReason(windows.find((check) => /dotnet test/.test(check.command)), 'macos'), 'needs a windows host')
})

test('matrix rows, steps and runners are read from the workflow text', () => {
  const block = [
    '  demo:', "    runs-on: ${{ matrix.runner || 'macos-15' }}", '    strategy:', '      matrix:', '        include:',
    '          - part: one', '            runner: windows-latest', '            run: make one',
    '          - part: two', '            run: |', '              make two', '              ./check two',
    '    steps:', '      - name: Run', '        env:', '          MODE: fast', '        run: ${{ matrix.run }}',
    '      - name: Later', '        if: failure()', '        working-directory: ./sub', '        run: |', '          make after', '          echo done',
    '      - uses: some/action@v1', '        with:', '          script: |', '            make scripted',
  ].join('\n')
  const rows = matrixRows(block)
  assert.deepEqual(rows, [{ part: 'one', runner: 'windows-latest', run: 'make one' }, { part: 'two', run: 'make two\n./check two' }])
  assert.deepEqual([jobHost(block, rows[0]), jobHost(block, rows[1])], ['windows', 'macos'])
  assert.deepEqual(steps(block).map((step) => [step.run, step.condition, step.directory, step.env]), [
    ['${{ matrix.run }}', '', '.', { MODE: 'fast' }], ['make after\necho done', 'failure()', './sub', {}], ['make scripted', '', '.', {}]])
  assert.deepEqual(matrixRows('    strategy:\n      matrix:\n        shard: [a, b]\n        memory: [gc, arc]\n    steps:\n'),
    [{ shard: 'a', memory: 'gc' }, { shard: 'a', memory: 'arc' }, { shard: 'b', memory: 'gc' }, { shard: 'b', memory: 'arc' }])
})
