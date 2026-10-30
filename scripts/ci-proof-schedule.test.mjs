import assert from 'node:assert/strict'
import { test } from 'node:test'
import { runKernelSchedule, runSchedule } from './ci-proof-fixture.mjs'

test('proof kernel schedules all three backends concurrently and retains every check', () => {
  const result = runKernelSchedule()
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.equal(result.calls.length, 8)
  for (const memory of ['gc', 'arc', 'wasm32']) {
    assert.ok(result.calls.some(call => call.source === 'examples/proof/kernel' && call.memory === memory))
    assert.ok(result.calls.some(call => call.source === 'examples/proof/kernel/tests' && call.memory === memory))
  }
  assert.equal(result.calls.filter(call => call.source === '.').length, 2)
})

for (const memory of ['gc', 'arc', 'wasm32']) {
  test(`proof scheduling propagates a ${memory} compiler failure`, () => {
    const result = runKernelSchedule({ FIXTURE_FAIL_BACKEND: memory })
    assert.notEqual(result.status, 0)
    assert.match(result.stdout + result.stderr, /Error 17/)
  })
}

test('proof scheduling rejects a successful process with a failed assertion summary', () => {
  const result = runKernelSchedule({ FIXTURE_BAD_TAP: '1' })
  assert.notEqual(result.status, 0)
  assert.match(result.stdout + result.stderr, /failed=1/)
})


test('proof CI freshly builds each backend once, checks both corpora and Unicode, then benchmarks', () => {
  const result = runSchedule('_test_proof_ci')
  assert.equal(result.status, 0, result.stdout + result.stderr)
  const batches = result.calls.filter(call => call.source === 'examples/proof/batch')
  assert.deepEqual(batches.map(call => call.memory).sort(), ['arc', 'gc', 'wasm32'])
  assert.equal(result.calls.length, 16)
  assert.equal(result.calls.at(-1).source, 'benchmark')
})

test('proof CI does not benchmark after a corpus failure', () => {
  const result = runSchedule('_test_proof_ci', { FIXTURE_FAIL_CORPUS: 'lean-core' })
  assert.notEqual(result.status, 0)
  assert.match(result.stdout + result.stderr, /Error 21/)
  assert.ok(!result.calls.some(call => call.source === 'benchmark'))
})


test('files named after proof targets cannot suppress acceptance checks', () => {
  const result = runSchedule('_test_proof_ci', { FIXTURE_STALE_TARGETS: '1' })
  assert.equal(result.status, 0, result.stdout + result.stderr)
  assert.ok(result.calls.some(call => call.source === 'disciplines'))
  assert.equal(result.calls.length, 16)
})
