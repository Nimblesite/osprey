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
// The ruleset requires every check ci.yml reports, by name: each job in ci.yml
// and each job of the workflows it calls (scripts/workflow-checks.mjs reads the
// names). There is no aggregate job, so any one failing or cancelled check
// blocks the merge. This script holds the ruleset equal to that list: a job
// missing from the ruleset gates nothing, and a ruleset name no job reports
// leaves every PR pending forever.
//
// The traps still checked here:
//
//   Path-filtered workflow — a workflow skipped by an `on: paths:` /
//   `paths-ignore:` filter never reports at all, and a required check that
//   never reports blocks the merge forever.
//
//   A workflow beside ci.yml — one that runs on a pull request by itself is
//   not in the list this script derives, so its checks would gate nothing.
//
//   A conditional `uses:` job — when its `if:` skips it, it reports under its
//   own name instead of its called jobs' names, so the required list would
//   change with the condition. Only a manual-dispatch gate is allowed.
//
// Run: node scripts/verify-branch-protection.mjs [--repo owner/name]

import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { targetsMainPullRequests } from './workflow-pr-target.mjs'
import { gatedCalls, workflowChecks } from './workflow-checks.mjs'

const DEFAULT_REPO = 'Nimblesite/osprey'
const WORKFLOW_DIR = '.github/workflows'

// The workflow whose checks the ruleset requires.
const ROOT_WORKFLOW = 'ci.yml'

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

// --- workflow scanning ------------------------------------------------------

const indentOf = (line) => line.length - line.trimStart().length

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
      return { file: f, pathFiltered: hasPathFilter(text), pullRequest: targetsMainPullRequests(text) }
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

// The ruleset requires exactly the checks ci.yml reports, each once.
const checkRequired = (actual, expected) => {
  const missing = expected.filter((name) => !actual.includes(name))
  const phantom = actual.filter((name) => !expected.includes(name))
  const doubled = actual.filter((name, i) => actual.indexOf(name) !== i)
  return [
    ...missing.map((name) => `"${name}" is reported by ${ROOT_WORKFLOW} but not required — add it to the ruleset`),
    ...phantom.map((name) => `"${name}" is required but no job reports it — every PR hangs pending`),
    ...doubled.map((name) => `"${name}" is required twice`),
  ]
}

// ci.yml is not path-filtered, calls no gated workflow, and no other workflow
// runs on a pull request by itself.
const checkWorkflows = (workflows) => [
  ...(workflows.find((w) => w.file === ROOT_WORKFLOW)?.pathFiltered
    ? [`${ROOT_WORKFLOW} path-filters at the \`on:\` level — a filtered-out run never reports`]
    : []),
  ...gatedCalls(ROOT_WORKFLOW).map((job) => `${job} calls a workflow under an \`if:\` — its checks change with the condition`),
  ...workflows
    .filter((w) => w.pullRequest && w.file !== ROOT_WORKFLOW)
    .map((w) => `${w.file} runs on pull requests by itself — call it from ${ROOT_WORKFLOW} so the ruleset requires its checks`),
]

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
  ...checkRequired(contexts, workflowChecks(ROOT_WORKFLOW)),
  ...checkWorkflows(workflows),
]

if (failures.length > 0) {
  console.error(`Branch protection on ${repo} is not intact:\n`)
  for (const f of failures) console.error(`  ✗ ${f}`)
  console.error('\nFix the ruleset at https://github.com/' + repo + '/settings/rules — not this script.')
  process.exit(1)
}

console.log(`Branch protection intact on ${repo}: ${onDefault.length} active ruleset(s), ${contexts.length} required checks.`)
for (const c of contexts) console.log(`  ✓ ${c}`)
