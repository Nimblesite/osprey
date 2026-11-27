import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { lowered, THRESHOLDS } from './verify-threshold-ratchet.mjs'

const gate = (rust, c) => ({ projects: { rust, c_runtime: c } })

test('a lowered threshold anywhere in the file fails, and raises or additions pass', () => {
  const base = gate({ one: { threshold: 97 }, two: { threshold: 95 } }, { fiber: { threshold: 90 } })
  assert.deepEqual(lowered(base, base), [])
  assert.deepEqual(lowered(base, gate({ one: { threshold: 98 }, two: { threshold: 95 }, three: { threshold: 95 } },
    { fiber: { threshold: 91 } })), [], 'raising or adding a threshold is the ratchet working')
  assert.deepEqual(lowered(base, gate({ one: { threshold: 96 }, two: { threshold: 95 } }, { fiber: { threshold: 89.5 } })),
    ['projects.rust.one.threshold: 97 -> 96', 'projects.c_runtime.fiber.threshold: 90 -> 89.5'],
    'every lowered number is named with its path, before and after')
  assert.deepEqual(lowered(base, gate({ one: { threshold: 97 } }, { fiber: { threshold: 90 } })), [],
    'a removed key belongs to the completeness checks, not this one')
})

test('the committed thresholds parse and have numbers to ratchet', () => {
  const current = JSON.parse(readFileSync(THRESHOLDS, 'utf8'))
  assert.deepEqual(lowered(current, current), [])
  assert.ok(JSON.stringify(current).match(/"threshold":\d/g).length > 50, 'the ratchet reads the real file')
})
