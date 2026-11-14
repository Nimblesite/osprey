// Holds the preflight to the workflows it reads: every make check CI runs is in the local plan,
// matrix values are filled in, the coverage gates run last, and checks keep the host CI gives them.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { jobHost, matrixRows, plan, runCommands, workflowFiles } from './preflight.mjs'

const checks = plan()
const commands = checks.map((check) => check.command)

test('the preflight reads ci.yml and every workflow it calls', () => {
  assert.deepEqual(workflowFiles(), ['.github/workflows/ci.yml', '.github/workflows/ci-windows.yml',
    '.github/workflows/gpui.yml', '.github/workflows/winui.yml'])
})

test('every make check in the PR workflows is planned, with no unexpanded matrix value', () => {
  for (const file of workflowFiles()) {
    for (const [, line] of readFileSync(file, 'utf8').matchAll(/^\s+(?:- )?run: (?:xvfb-run -a )?(make [^$\n]+)$/gm)) {
      assert.ok(commands.includes(line.trim()), `${file}: ${line} is in the preflight`)
    }
  }
  assert.ok(commands.every((command) => !command.includes('${{')), commands.find((command) => command.includes('${{')))
  assert.equal(new Set(commands).size, commands.length, 'each check runs once')
})

test('the coverage gates run last, after every test that feeds them', () => {
  const gates = ['make _coverage_check_rust', 'make _coverage_check_vscode_extension', 'make _coverage_check_packages', 'make _coverage_check_c_runtime']
  assert.deepEqual(commands.slice(-gates.length), gates)
  for (const feeder of ['make _test_rust', 'make _test_vscode_extension', 'make _test_packages', 'make -j2 _test_c_runtime']) {
    assert.ok(commands.indexOf(feeder) >= 0 && commands.indexOf(feeder) < commands.length - gates.length, feeder)
  }
})

test('sharded CI checks run as their one local equivalent', () => {
  assert.ok(!commands.some((command) => command.includes('_test_rust_shard')), 'the shards merge into one report locally')
  assert.equal(commands.filter((command) => command === 'make _test_rust').length, 1)
  assert.ok(commands.includes('make _test_vscode_extension'), 'xvfb-run is a Linux runner detail')
})

test('every Apple matrix row is planned whole on macOS, Windows checks on Windows', () => {
  const host = (command) => checks.find((check) => check.command === command)?.host
  for (const command of ['make -C src/toolchain/targets/ios/tests examples', 'make -C src/toolchain/targets/ios/tests inbox tally-headless',
    'target/release/osprey test src/osprey/examples/pocket_orchestra/test --memory=arc', 'make -C src/toolchain/targets/macos/tests workbench']) {
    assert.equal(host(command), 'macos', command)
  }
  assert.equal(host('make _test_registry REGISTRY_MEMORY=arc'), 'linux')
  assert.ok(checks.filter((check) => check.job.includes('winui.yml')).every((check) => check.host === 'windows'))
})

test('matrix rows, run blocks and runners are read from the workflow text', () => {
  const block = [
    '  demo:', "    runs-on: ${{ matrix.runner || 'macos-15' }}", '    strategy:', '      matrix:', '        include:',
    '          - part: one', '            runner: windows-latest', '            run: make one',
    '          - part: two', '            run: |', '              make two', '              ./check two',
    '    steps:', '      - name: Run', '        run: ${{ matrix.run }}', '      - run: |', '          make after', '          echo done',
  ].join('\n')
  const rows = matrixRows(block)
  assert.deepEqual(rows, [{ part: 'one', runner: 'windows-latest', run: 'make one' }, { part: 'two', run: 'make two\n./check two' }])
  assert.deepEqual([jobHost(block, rows[0]), jobHost(block, rows[1])], ['windows', 'macos'])
  assert.deepEqual(runCommands(block), ['${{ matrix.run }}', 'make after', 'echo done'])
  assert.deepEqual(matrixRows('      matrix:\n        shard: [a, b]\n        memory: [gc, arc]\n'),
    [{ shard: 'a', memory: 'gc' }, { shard: 'a', memory: 'arc' }, { shard: 'b', memory: 'gc' }, { shard: 'b', memory: 'arc' }])
})
