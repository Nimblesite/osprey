#!/usr/bin/env node
// Asserts that `main` is actually gated — that the branch rulesets are ENABLED,
// that they require the checks listed here, and that each required check is one
// a workflow can really report.
//
// This exists because both rulesets were found `enforcement: "disabled"` with a
// 212-file PR already merged past them. Nothing in the repo could have noticed:
// branch protection lives in GitHub's settings, not in the tree, so it drifts
// silently and by definition no test covers it. This is that test.
//
// Four traps are checked structurally, because each is a way the gates go quiet:
//
//   Phantom context — ruleset 7726557 required a check named "CI". No job
//   reports that name (jobs report their `name:`, and the workflow's own name
//   is not a check run), so every PR would sit pending forever. A required
//   check that can never report is indistinguishable from a broken repo, and
//   the fix reached for under pressure is to switch the ruleset off.
//
//   Path-filtered required job — a job skipped by a workflow-level `on: paths:`
//   / `paths-ignore:` filter never reports at all, and a required check that
//   never reports blocks the merge forever. A job skipped by a job-level `if:`
//   reports "skipped", which counts as PASSING. Required jobs must therefore
//   skip via `if:`, never via path filtering.
//
//   Advisory tier — a job that runs on every PR but is absent from the ruleset
//   burns runner minutes and blocks nothing. EXPECTED_CONTEXTS said "every job
//   is here" while nothing compared the two, so the next job added would have
//   been advisory by omission and this script would still have printed intact.
//
//   Unrequired upstream — a job gated on `needs.X.outputs` SKIPS when X fails,
//   and a skipped check reports as PASSING. If X is not itself required, one
//   failure in X silently vanishes every job downstream of it. That is exactly
//   why "Detect changed areas (Windows)" is in the list below.
//
// Run: node scripts/verify-branch-protection.mjs [--repo owner/name]

import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

const DEFAULT_REPO = 'Nimblesite/osprey'
const WORKFLOW_DIR = '.github/workflows'

// The gate list. Adding a required check means adding it here AND to the
// ruleset; this script fails until the two agree, in either direction.
// Every job in the four-stage pipeline is here, because every job blocks.
// There is no advisory tier: a suite worth running is worth failing the merge,
// and one marked "not required" has been deleted in all but name.
const EXPECTED_CONTEXTS = [
  'Detect changed areas',
  'Build, Format & Analyse',
  'Tests: Rust workspace (coverage)',
  'Tests: language corpus (default)',
  'Tests: language corpus (gc)',
  'Tests: language corpus (arc)',
  'Tests: C runtime (coverage)',
  'Tests: VS Code extension (coverage)',
  'Tests: WebAssembly target (wasm32-wasip1)',
  'Tests: Website E2E (Playwright)',
  'Tests: integration (bank, profiler, web compiler)',
  'Coverage thresholds',
  // Required because `windows-core` skips on its output, and a skipped check
  // reports as PASSING. Left unrequired, a failure in this job silently
  // vanished the entire Windows gate rather than blocking on it.
  'Detect changed areas (Windows)',
  'Windows Core Build & Smoke Test',
]

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

// A `pull_request:` key inside the `on:` block. Only these workflows report
// check runs on a PR, so only their jobs can be required — or advisory.
const onPullRequest = (text) => {
  const lines = text.split('\n')
  const start = lines.findIndex((l) => /^on:\s*$/.test(l))
  if (start < 0) return false
  for (const line of lines.slice(start + 1)) {
    if (indentOf(line) === 0 && line.trim() !== '') break
    if (/^\s{2}pull_request:/.test(line)) return true
  }
  return false
}

const readWorkflows = () =>
  readdirSync(WORKFLOW_DIR)
    .filter((f) => f.endsWith('.yml') || f.endsWith('.yaml'))
    .map((f) => {
      const text = readFileSync(join(WORKFLOW_DIR, f), 'utf8')
      return { file: f, jobs: scanJobs(text), pathFiltered: hasPathFilter(text), pullRequest: onPullRequest(text) }
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

// Every job that reports on a PR must be required. A job that runs and cannot
// fail the merge is the advisory tier this repo does not have.
const checkNoAdvisoryTier = (workflows) =>
  workflows
    .filter((w) => w.pullRequest)
    .flatMap((w) => w.jobs.map((j) => ({ ...j, file: w.file })))
    .filter((j) => j.name && !EXPECTED_CONTEXTS.includes(j.name))
    .map(
      (j) =>
        `job "${j.name}" (${j.file}) runs on every PR but is absent from EXPECTED_CONTEXTS — ` +
        'it burns runner minutes and blocks nothing; require it or delete the job',
    )

// A job gated on `needs.X.outputs` skips when X fails, and skipped counts as
// PASSING — so X must block the merge too, or its failure vanishes downstream.
const checkConditionalUpstreams = (workflows) =>
  workflows
    .filter((w) => w.pullRequest)
    .flatMap((w) =>
      w.jobs.flatMap((j) =>
        [...j.ifText.matchAll(/needs\.([\w-]+)\.outputs/g)]
          .map((m) => w.jobs.find((u) => u.key === m[1]))
          .filter((up) => up && !EXPECTED_CONTEXTS.includes(up.name))
          .map(
            (up) =>
              `job "${j.name}" (${w.file}) is gated on \`needs.${up.key}.outputs\`, but "${up.name}" ` +
              'is not required — a failure there makes this job skip, and a skipped check reports as PASSING',
          ),
      ),
    )

const checkContexts = (actual, workflows) => {
  const failures = []
  const missing = EXPECTED_CONTEXTS.filter((c) => !actual.includes(c))
  const extra = actual.filter((c) => !EXPECTED_CONTEXTS.includes(c))
  if (missing.length) failures.push(`ruleset does not require: ${missing.join(', ')}`)
  if (extra.length) failures.push(`ruleset requires checks absent from EXPECTED_CONTEXTS: ${extra.join(', ')}`)

  const allJobs = workflows.flatMap((w) => w.jobs.map((j) => ({ ...j, file: w.file, pathFiltered: w.pathFiltered })))
  for (const context of actual) {
    const job = allJobs.find((j) => j.name === context)
    if (!job) {
      failures.push(`required check "${context}" matches no job name — it can never report, so every PR hangs pending`)
      continue
    }
    if (job.pathFiltered) {
      failures.push(
        `required check "${context}" lives in ${job.file}, which path-filters at the \`on:\` level — ` +
          'a filtered-out run never reports and blocks the merge forever; skip with a job-level `if:` instead',
      )
    }
  }
  return failures
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
  ...checkContexts(contexts, workflows),
  ...checkNoAdvisoryTier(workflows),
  ...checkConditionalUpstreams(workflows),
]

if (failures.length > 0) {
  console.error(`Branch protection on ${repo} is not intact:\n`)
  for (const f of failures) console.error(`  ✗ ${f}`)
  console.error('\nFix the ruleset at https://github.com/' + repo + '/settings/rules — not this script.')
  process.exit(1)
}

console.log(`Branch protection intact on ${repo}: ${onDefault.length} active ruleset(s), ${contexts.length} required checks.`)
for (const c of contexts) console.log(`  ✓ ${c}`)
