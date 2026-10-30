#!/usr/bin/env node
// The one required check. The `main` ruleset requires only the job that runs
// this script; it waits for every other workflow run on the pull request's head
// commit, and every other job in its own run, then fails if any of them neither
// succeeded nor skipped. Every check on the branch therefore gates the merge:
// a new job or workflow blocks from the commit that adds it, with no list to
// keep in step here or in the ruleset, so two pull requests that each add a job
// can never require a check only the other produces.
//
// A skipped job passes, as branch protection treats it; one skipped because its
// upstream failed still blocks, because the upstream's failure is seen here.
// A workflow that a path filter never starts has no run and is not waited for.
//
// Run in CI: GH_TOKEN, GITHUB_REPOSITORY, GITHUB_RUN_ID, HEAD_SHA, GATE_JOB.

import { pathToFileURL } from 'node:url'

const PASSING = new Set(['success', 'skipped', 'neutral'])
const POLL_SECONDS = 30

// The latest run of each workflow, without this gate's own run: a re-run or a
// second push of the same commit supersedes the earlier run of that workflow.
export const latestRuns = (runs, ownRunId) => {
  const latest = new Map()
  for (const run of runs) {
    if (run.id === ownRunId) continue
    const seen = latest.get(run.workflow_id)
    if (!seen || run.created_at > seen.created_at) latest.set(run.workflow_id, run)
  }
  return [...latest.values()]
}

// What is still running and what failed, among the other workflow runs and
// this run's other jobs.
export const verdict = ({ runs, jobs, ownRunId, gateJob }) => {
  const checks = [
    ...latestRuns(runs, ownRunId).map((r) => ({ name: `workflow "${r.name}"`, status: r.status, conclusion: r.conclusion })),
    ...jobs.filter((j) => j.name !== gateJob).map((j) => ({ name: `job "${j.name}"`, status: j.status, conclusion: j.conclusion })),
  ]
  return {
    pending: checks.filter((c) => c.status !== 'completed').map((c) => c.name),
    failed: checks.filter((c) => c.status === 'completed' && !PASSING.has(c.conclusion)).map((c) => `${c.name}: ${c.conclusion}`),
  }
}

const api = async (path) => {
  const headers = { accept: 'application/vnd.github+json', authorization: `Bearer ${process.env.GH_TOKEN}`, 'user-agent': 'osprey-ci' }
  const res = await fetch(`https://api.github.com/repos/${process.env.GITHUB_REPOSITORY}${path}`, { headers })
  if (!res.ok) throw new Error(`GET ${path} -> ${res.status} ${res.statusText}`)
  return res.json()
}

const observe = async () => {
  const ownRunId = Number(process.env.GITHUB_RUN_ID)
  const runs = await api(`/actions/runs?head_sha=${process.env.HEAD_SHA}&event=pull_request&per_page=100`)
  const jobs = await api(`/actions/runs/${ownRunId}/jobs?filter=latest&per_page=100`)
  return verdict({ runs: runs.workflow_runs, jobs: jobs.jobs, ownRunId, gateJob: process.env.GATE_JOB })
}

const main = async () => {
  for (;;) {
    const seen = await observe().catch((error) => ({ error }))
    if (seen.error) console.log(`Retrying after ${seen.error.message}`)
    const { pending = ['the GitHub API'], failed = [] } = seen
    if (failed.length > 0) {
      console.error(`Not every check passed:\n${failed.map((f) => `  ✗ ${f}`).join('\n')}`)
      process.exitCode = 1
      return
    }
    if (pending.length === 0) {
      console.log('Every check on this commit passed or skipped.')
      return
    }
    console.log(`Waiting on ${pending.length}: ${pending.join(', ')}`)
    await new Promise((resolve) => setTimeout(resolve, POLL_SECONDS * 1000))
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) await main()
