import assert from 'node:assert/strict'
import { test } from 'node:test'
import { rootViolations } from './root-layout.mjs'

const clean = { tracked: ['Makefile', 'src', 'docs', 'android', 'ios', 'llms', 'macos', 'tests', 'web', 'windows'], files: ['Makefile', '.env', '.DS_Store', '.mcp.json'] }

test('a clean root has no violations', () => {
  assert.deepEqual(rootViolations(clean), [])
  assert.deepEqual(rootViolations({ tracked: [], files: [] }), [])
})

test('logs are embargoed whatever their name, even when tracked', () => {
  for (const name of ['build.log', 'BUILD.LOG', 'npm-debug.log', 'osprey.log.1', 'test.log.gz']) {
    const [violation, ...rest] = rootViolations({ tracked: [], files: [name] })
    assert.match(violation, /log files are embargoed/, name)
    assert.deepEqual(rest, [], name)
  }
  assert.ok(rootViolations({ tracked: ['build.log'], files: ['build.log'] }).some((line) => /embargoed/.test(line)))
})

test('names that merely contain log are not logs', () => {
  assert.deepEqual(rootViolations({ tracked: ['Makefile'], files: ['Makefile', '.env'] }), [])
  for (const name of ['catalog', 'blog.txt', 'logfile']) {
    const [violation] = rootViolations({ tracked: [], files: [name] })
    assert.match(violation, /stray file/, name)
  }
})

test('stray files are refused, whatever wrote them', () => {
  for (const name of ['osprey_transformer_1_2_raw.bin', 'osprey_http_state_levels.db', 'tests.cpuprofile', 'nohup.out', 'core']) {
    const violations = rootViolations({ tracked: [], files: [name] })
    assert.equal(violations.length, 1, name)
    assert.match(violations[0], /stray file/, name)
  }
})

test('a new tracked root entry must be added to the allowlist on purpose', () => {
  for (const name of ['new-compiler', 'crates2', 'notes.md']) {
    const violations = rootViolations({ tracked: [name], files: [] })
    assert.equal(violations.length, 1, name)
    assert.match(violations[0], /not an allowed root entry/, name)
  }
})

test('every violation is reported, sorted', () => {
  assert.deepEqual(
    rootViolations({ tracked: ['zzz'], files: ['b.log', 'a.bin'] }).map((line) => line.split(':')[0]),
    ['a.bin', 'b.log', 'zzz'],
  )
})
