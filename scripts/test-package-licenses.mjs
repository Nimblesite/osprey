#!/usr/bin/env node
import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { fileURLToPath } from 'node:url'

const PACKAGES = fileURLToPath(new URL('../packages/', import.meta.url))
const CHECKER = fileURLToPath(new URL('./verify-package-licenses.mjs', import.meta.url))
const verify = (root) => spawnSync(process.execPath, [join(root, 'scripts/verify-package-licenses.mjs')], { encoding: 'utf8' })

const inFixture = (check) => {
  const root = mkdtempSync(join(tmpdir(), 'osprey-license-test-'))
  try {
    mkdirSync(join(root, 'scripts'))
    cpSync(CHECKER, join(root, 'scripts/verify-package-licenses.mjs'))
    cpSync(PACKAGES, join(root, 'packages'), { recursive: true, filter: (path) => !path.includes('/.deslop') })
    check(root)
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

test('reviewed packages retain their MIT grants and scoped material notices', () => inFixture((root) => {
  const result = verify(root)
  assert.equal(result.status, 0, result.stderr)
}))

test('required original attribution cannot be removed while retaining generic license text', () => inFixture((root) => {
  const path = join(root, 'packages/scipy/NOTICE.md')
  const original = readFileSync(path, 'utf8')
  const credit = '(C) Copyright John Maddock 2006.'
  assert.ok(original.includes(credit))
  writeFileSync(path, original.replaceAll(credit, ''))
  assert.notEqual(verify(root).status, 0, 'Missing original copyright must fail the license gate')
}))

test('an API reference does not authorize an unreviewed upstream license copy', () => inFixture((root) => {
  const dir = join(root, 'packages/einops/LICENSES')
  mkdirSync(dir)
  writeFileSync(join(dir, 'unrequired.txt'), 'Unreviewed upstream license text')
  assert.notEqual(verify(root).status, 0)
}))
