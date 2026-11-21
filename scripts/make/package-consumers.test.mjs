// [PKG-CONSUMERS-BUILD] A package's check also compiles every project built on its source, so a change
// to the package that breaks a consumer (bb930140d: a new synth Processor left the daw's exhaustive
// match short) fails the package's own target, not only CI's every-program sweep.
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { test } from 'node:test'

const make = (...args) => execFileSync('make', ['-n', ...args], { encoding: 'utf8' })
const consumers = (name) =>
  execFileSync('node', ['scripts/make/package-consumers.mjs', '--list', name], { encoding: 'utf8' }).trim().split('\n')

test('the synth package target compiles every consumer of the synth source', () => {
  const plan = make('-C', 'src/osprey/packages', 'synth')
  assert.match(plan, /package_coverage\.py --collect .*--package synth/, 'the suite still runs')
  assert.match(plan, /package-consumers\.mjs .*synth/, 'then every consumer compiles')
})

test('consumers are the projects whose source roots include the package source, never the package', () => {
  const synth = consumers('synth')
  for (const dir of ['src/osprey/examples/daw', 'src/osprey/examples/daw/test', 'src/osprey/examples/daw/render', 'src/osprey/examples/pocket_orchestra/kernel']) {
    assert.ok(synth.includes(dir), `${dir} consumes synth: ${synth}`)
  }
  assert.ok(!synth.some((dir) => dir.startsWith('src/osprey/packages/synth')), 'the package is not its own consumer')
  assert.deepEqual(synth, [...new Set(synth)].sort(), 'each consumer once, in order')
  assert.ok(consumers('json').includes('src/osprey/examples/daw'), 'the daw also consumes json')
})
