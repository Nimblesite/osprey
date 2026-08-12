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
// Two traps are checked structurally, because both are why the gates were off:
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
// Run: node scripts/verify-branch-protection.mjs [--repo owner/name]

import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

const DEFAULT_REPO = 'Nimblesite/osprey'
const WORKFLOW_DIR = '.github/workflows'

// The gate list. Adding a required check means adding it here AND to the
// ruleset; this script fails until the two agree, in either direction.
const EXPECTED_CONTEXTS = [
  'Detect changed areas',
  'Test, Format, Build & Validate',
  'Rust Compiler (fmt, clippy, test, corpus)',
  'WebAssembly target (wasm32-wasip1)',
  'Website E2E (Playwright)',
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
// whether that job carries a job-level `if:`. Those names ARE the check-run
// contexts GitHub reports.
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
      current = { key: line.trim().replace(':', ''), name: null, hasIf: false }
      jobs.push(current)
      continue
    }
    if (!current || indentOf(line) !== 4) continue
    const name = line.match(/^\s{4}name:\s*(.+?)\s*$/)
    if (name) current.name = name[1].replace(/^['"]|['"]$/g, '')
    if (/^\s{4}if:/.test(line)) current.hasIf = true
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
      return { file: f, jobs: scanJobs(text), pathFiltered: hasPathFilter(text) }
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
const failures = [...rulesetFailures, ...checkContexts(contexts, readWorkflows())]

if (failures.length > 0) {
  console.error(`Branch protection on ${repo} is not intact:\n`)
  for (const f of failures) console.error(`  ✗ ${f}`)
  console.error('\nFix the ruleset at https://github.com/' + repo + '/settings/rules — not this script.')
  process.exit(1)
}

console.log(`Branch protection intact on ${repo}: ${onDefault.length} active ruleset(s), ${contexts.length} required checks.`)
for (const c of contexts) console.log(`  ✓ ${c}`)
