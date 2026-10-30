import { test } from 'node:test'
import assert from 'node:assert/strict'
import { latestRuns, verdict } from './all-checks-green.mjs'

const run = (id, workflowId, name, status, conclusion, createdAt = '2026-10-30T00:00:00Z') =>
  ({ id, workflow_id: workflowId, name, status, conclusion, created_at: createdAt })
const job = (name, status, conclusion) => ({ name, status, conclusion })
const gate = 'All checks green'

test('a re-run supersedes the earlier run of its workflow and the gate ignores its own run', () => {
  const runs = [
    run(1, 10, 'CI', 'in_progress', null),
    run(2, 20, 'GPUI host', 'completed', 'failure', '2026-10-30T00:00:00Z'),
    run(3, 20, 'GPUI host', 'completed', 'success', '2026-10-30T01:00:00Z'),
  ]
  assert.deepEqual(latestRuns(runs, 1).map((r) => r.id), [3])
})

test('every other workflow and job must finish before the gate decides', () => {
  const seen = verdict({
    runs: [run(1, 10, 'CI', 'in_progress', null), run(2, 20, 'WinUI host', 'queued', null)],
    jobs: [job(gate, 'in_progress', null), job('Build, Format & Analyse', 'in_progress', null)],
    ownRunId: 1,
    gateJob: gate,
  })
  assert.deepEqual(seen, { pending: ['workflow "WinUI host"', 'job "Build, Format & Analyse"'], failed: [] })
})

test('success, skipped and neutral pass; anything else fails the merge', () => {
  const seen = verdict({
    runs: [run(2, 20, 'GPUI host', 'completed', 'cancelled'), run(3, 30, 'CI (Windows)', 'completed', 'success')],
    jobs: [
      job('Detect changed areas', 'completed', 'success'),
      job('Tests: Website E2E (Playwright)', 'completed', 'skipped'),
      job('Tests: proof kernel', 'completed', 'neutral'),
      job('Coverage thresholds', 'completed', 'failure'),
      job('Tests: WebAssembly', 'completed', 'timed_out'),
    ],
    ownRunId: 1,
    gateJob: gate,
  })
  assert.deepEqual(seen.pending, [])
  assert.deepEqual(seen.failed, [
    'workflow "GPUI host": cancelled',
    'job "Coverage thresholds": failure',
    'job "Tests: WebAssembly": timed_out',
  ])
})

test('a commit whose checks all passed or skipped is green', () => {
  const seen = verdict({
    runs: [run(2, 20, 'GPUI host', 'completed', 'success')],
    jobs: [job(gate, 'in_progress', null), job('Build, Format & Analyse', 'completed', 'success')],
    ownRunId: 1,
    gateJob: gate,
  })
  assert.deepEqual(seen, { pending: [], failed: [] })
})
