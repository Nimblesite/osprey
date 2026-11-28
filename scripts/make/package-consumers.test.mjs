// [PKG-CONSUMERS-BUILD] A package's check also compiles every project built on its source, so a change
// to the package that breaks a consumer (bb930140d: a new synth Processor left the daw's exhaustive
// match short) fails the package's own target, not only CI's every-program sweep.
import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { existsSync } from 'node:fs'
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
  assert.ok(synth.includes('src/osprey/examples/pocket_orchestra/kernel'), `the orchestra kernel consumes synth: ${synth}`)
  assert.ok(!synth.some((dir) => dir.startsWith('src/osprey/packages/synth')), 'the package is not its own consumer')
  assert.deepEqual(synth, [...new Set(synth)].sort(), 'each consumer once, in order')
})

test('the projects of a laid split repository consume the package too', { skip: !existsSync('target/downstream/perception/.git') && 'Perception is not laid: run make downstream' }, () => {
  // [REPO-SPLIT-CANARY] Perception studio, test and render projects consume json, laid by `make downstream`.
  const json = consumers('json')
  for (const dir of ['src/studio', 'src/studio/test', 'src/studio/render']) {
    assert.ok(json.includes(dir), `${dir} consumes json: ${json}`)
  }
})
