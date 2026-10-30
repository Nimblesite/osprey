#!/usr/bin/env node
// Asserts that `main` is actually gated — that the branch rulesets are ENABLED,
// that they require exactly the one check named below, and that a workflow can
// really report it.
//
// This exists because both rulesets were found `enforcement: "disabled"` with a
// 212-file PR already merged past them. Nothing in the repo could have noticed:
// branch protection lives in GitHub's settings, not in the tree, so it drifts
// silently and by definition no test covers it. This is that test.
//
// The ruleset requires one check, "All checks green" (scripts/all-checks-green.mjs).
// It waits for every other workflow run and job on the pull request's head and
// fails if any did not pass or skip, so every check on a branch gates its merge
// and nothing here lists jobs. The ruleset once listed every job by name, and
// this script held it equal to that list; then two pull requests that each added
// a job could only merge after the other, each requiring a check that only the
// other produced. A list also left two traps the gate closes by construction: a
// job missing from it was advisory, and a job skipped because its unlisted
// upstream failed reported as passing.
//
// The traps still checked here, because each is a way the gate goes quiet:
//
//   Phantom context — ruleset 7726557 required a check named "CI". No job
//   reports that name (jobs report their `name:`, and the workflow's own name
//   is not a check run), so every PR would sit pending forever. A required
//   check that can never report is indistinguishable from a broken repo, and
//   the fix reached for under pressure is to switch the ruleset off.
//
//   Path-filtered required job — a job skipped by a workflow-level `on: paths:`
//   / `paths-ignore:` filter never reports at all, and a required check that
//   never reports blocks the merge forever.
//
//   A gate that does not always run — skipped reports as PASSING, so the gate
//   job must run on every pull request whatever else fails: `if: always()`.
//
// Run: node scripts/verify-branch-protection.mjs [--repo owner/name]

import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { targetsMainPullRequests } from './workflow-pr-target.mjs'

const DEFAULT_REPO = 'Nimblesite/osprey'
const WORKFLOW_DIR = '.github/workflows'

// The only required check: the job that runs scripts/all-checks-green.mjs.
const REQUIRED_CHECK = 'All checks green'

const argRepo = (argv) => {
  const i = argv.indexOf('--repo')
  return i >= 0 && argv[i + 1] ? argv[i + 1] : DEFAULT_REPO
}

const api = async (path, token) => {
  const headers = { accept: 'application/vnd.github+json', 'user-agent': 'osprey-ci' }
  if (token) headers.authorization = `Bearer ${token}`
  const res = await fetch(`https://api.github.com${path}`, { headers })
  if (!res.ok) throw new Error(`GET ${path} -> ${res.status} ${res.statusText}`)
  return res.json()
}

// --- workflow scanning (no YAML dependency; structure-specific by design) ----

const indentOf = (line) => line.length - line.trimStart().length

// Every `name:` at job level (4 spaces) inside the `jobs:` block, paired with
// the job-level `if:` expression that gates it. Those names ARE the check-run
// contexts GitHub reports; the `if:` text is what the upstream trap reads.
const scanJobs = (text) => {
  const lines = text.split('\n')
  const start = lines.findIndex((l) => /^jobs:\s*$/.test(l))
  if (start < 0) return []
  const jobs = []
  let current = null
  for (const line of lines.slice(start + 1)) {
    if (line.trim() === '' || line.trimStart().startsWith('#')) continue
    if (indentOf(line) === 0) break
    if (indentOf(line) === 2 && /^\s{2}[\w-]+:\s*$/.test(line)) {
      current = { key: line.trim().replace(':', ''), name: null, ifText: '' }
      jobs.push(current)
      continue
    }
    if (!current || indentOf(line) !== 4) continue
    const name = line.match(/^\s{4}name:\s*(.+?)\s*$/)
    if (name) current.name = name[1].replace(/^['"]|['"]$/g, '')
    const gate = line.match(/^\s{4}if:\s*(.+?)\s*$/)
    if (gate) current.ifText = gate[1]
  }
  return jobs
}

// A `paths:` / `paths-ignore:` key inside the `on:` block — the filter that
// leaves a required check pending forever.
const hasPathFilter = (text) => {
  const lines = text.split('\n')
  const start = lines.findIndex((l) => /^on:\s*$/.test(l))
  if (start < 0) return false
  for (const line of lines.slice(start + 1)) {
    if (indentOf(line) === 0 && line.trim() !== '') break
    if (/^\s+paths(-ignore)?:/.test(line)) return true
  }
  return false
}

const readWorkflows = () =>
  readdirSync(WORKFLOW_DIR)
    .filter((f) => f.endsWith('.yml') || f.endsWith('.yaml'))
    .map((f) => {
      const text = readFileSync(join(WORKFLOW_DIR, f), 'utf8')
      return { file: f, jobs: scanJobs(text), pathFiltered: hasPathFilter(text), pullRequest: targetsMainPullRequests(text) }
    })

// --- assertions -------------------------------------------------------------

const checkRulesets = (rulesets, detail) => {
  const failures = []
  const onDefault = rulesets.filter((r) =>
    (detail[r.id]?.conditions?.ref_name?.include ?? []).includes('~DEFAULT_BRANCH'),
  )
  if (onDefault.length === 0) failures.push('no ruleset targets the default branch — `main` is ungated')

  for (const r of onDefault) {
    const full = detail[r.id]
    if (full.enforcement !== 'active') {
      failures.push(`ruleset ${r.id} "${r.name}" is enforcement="${full.enforcement}" — it gates nothing`)
    }
    if ((full.bypass_actors ?? []).length > 0) {
      const who = full.bypass_actors.map((a) => a.actor_type).join(', ')
      failures.push(`ruleset ${r.id} "${r.name}" has bypass actors (${who}) — the gate is optional for them`)
    }
  }
  return { failures, onDefault }
}

const requiredContexts = (onDefault, detail) =>
  onDefault.flatMap((r) =>
    (detail[r.id].rules ?? [])
      .filter((rule) => rule.type === 'required_status_checks')
      .flatMap((rule) => rule.parameters.required_status_checks.map((c) => c.context)),
  )

// The ruleset names the gate and nothing else: any other name is a list again.
const checkRequired = (actual) =>
  actual.length === 1 && actual[0] === REQUIRED_CHECK
    ? []
    : [`ruleset must require exactly "${REQUIRED_CHECK}", not: ${actual.join(', ') || 'nothing'}`]

// The gate is a real job that every pull request starts and that always runs.
const checkGateJob = (workflows) => {
  const gate = workflows
    .filter((w) => w.pullRequest)
    .flatMap((w) => w.jobs.map((j) => ({ ...j, file: w.file, pathFiltered: w.pathFiltered })))
    .find((j) => j.name === REQUIRED_CHECK)
  if (!gate) return [`required check "${REQUIRED_CHECK}" matches no job name — it can never report, so every PR hangs pending`]
  return [
    ...(gate.pathFiltered
      ? [`"${REQUIRED_CHECK}" lives in ${gate.file}, which path-filters at the \`on:\` level — a filtered-out run never reports`]
      : []),
    ...(/^(\$\{\{\s*)?always\(\)(\s*\}\})?$/.test(gate.ifText)
      ? []
      : [`"${REQUIRED_CHECK}" must run on every PR (\`if: always()\`), not \`if: ${gate.ifText}\``]),
  ]
}

// --- main -------------------------------------------------------------------

const repo = argRepo(process.argv)
const token = process.env.GH_TOKEN || process.env.GITHUB_TOKEN
const summary = await api(`/repos/${repo}/rulesets`, token)
const detail = Object.fromEntries(
  await Promise.all(summary.map(async (r) => [r.id, await api(`/repos/${repo}/rulesets/${r.id}`, token)])),
)

const { failures: rulesetFailures, onDefault } = checkRulesets(summary, detail)
const contexts = requiredContexts(onDefault, detail)
const workflows = readWorkflows()
const failures = [
  ...rulesetFailures,
  ...checkRequired(contexts),
  ...checkGateJob(workflows),
]

if (failures.length > 0) {
  console.error(`Branch protection on ${repo} is not intact:\n`)
  for (const f of failures) console.error(`  ✗ ${f}`)
  console.error('\nFix the ruleset at https://github.com/' + repo + '/settings/rules — not this script.')
  process.exit(1)
}

console.log(`Branch protection intact on ${repo}: ${onDefault.length} active ruleset(s), ${contexts.length} required checks.`)
for (const c of contexts) console.log(`  ✓ ${c}`)
