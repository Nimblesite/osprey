#!/usr/bin/env node
// The release pipeline is the one workflow nobody watches until it matters, and
// it had four separate ways to publish nothing and report success: win32 build
// and VSIX legs marked `continue-on-error`, runtime archives copied with `||
// true`, a token probe that turned a missing credential into a skipped publish,
// and `git commit || exit 0` swallowing a failed tap push. Each was defensible
// on its own; together they meant "green" did not mean "released".
//
// Removing them is not enough, because the failure mode that survives is a job
// that never runs: GitHub scores a skipped job as green, so a wrong `if:` or an
// unset scope output silently produces a release that shipped nothing. The
// `release-complete` job in release.yml is the backstop for that, and this
// check is the backstop for the backstop — it fails the PR that would let any
// of it back in.
//
// Like verify-node-deps-guard.mjs this is a script inside an already-required
// job, not a new required check, so the pinned context list in
// verify-branch-protection.mjs and the ruleset stay in agreement.
import { readFileSync, readdirSync } from 'node:fs'

const WORKFLOW_DIR = '.github/workflows'
const RELEASE_WORKFLOW = 'release.yml'
const GATE_JOB = 'release-complete'
const GATE_CONDITION = 'if: always()'

// A run step may not tolerate a failure. One does, for a reason that holds: it
// is followed by an explicit check that fails loudly on the bad case, so the
// tolerance buys a clear diagnostic rather than hiding anything.
//
// The set is compared exactly. An entry matching nothing fails too — a stale
// exemption is a hole with nobody looking at it.
const REVIEWED_TOLERANCES = [
  {
    file: 'release-change-detection.yml',
    fragment: 'prev="$(git describe',
    reason: 'first release has no previous tag: the empty result selects the publish-everything branch',
  },
]

const SWALLOWS = [
  { pattern: /continue-on-error/, name: 'continue-on-error' },
  { pattern: /\|\|\s*true\b/, name: '|| true' },
  { pattern: /\|\|\s*exit 0\b/, name: '|| exit 0' },
]

const isComment = (line) => line.trimStart().startsWith('#')

const workflowFiles = readdirSync(WORKFLOW_DIR).filter((name) => name.endsWith('.yml') || name.endsWith('.yaml'))

const readWorkflow = (file) => readFileSync(`${WORKFLOW_DIR}/${file}`, 'utf8').split('\n')

// Every tolerated line across the tree, as {file, line, text, swallow}.
const toleranceSites = workflowFiles.flatMap((file) =>
  readWorkflow(file)
    .map((text, index) => ({ file, line: index + 1, text }))
    .filter(({ text }) => !isComment(text))
    .flatMap((site) => SWALLOWS.filter(({ pattern }) => pattern.test(site.text)).map(({ name }) => ({ ...site, swallow: name }))),
)

const isReviewed = (site) =>
  REVIEWED_TOLERANCES.some((entry) => entry.file === site.file && site.text.includes(entry.fragment))

const unreviewed = toleranceSites
  .filter((site) => !isReviewed(site))
  .map((site) => `${site.file}:${site.line}: \`${site.swallow}\` — a failed step here reports success`)

const stale = REVIEWED_TOLERANCES.filter(
  (entry) => !toleranceSites.some((site) => site.file === entry.file && site.text.includes(entry.fragment)),
).map((entry) => `reviewed tolerance no longer present: ${entry.file} \`${entry.fragment}\` — delete the entry`)

// ---- The release backstop must name every job it is backstopping ------------

const releaseLines = readWorkflow(RELEASE_WORKFLOW)

const jobsStart = releaseLines.findIndex((line) => line === 'jobs:')

const jobNames = releaseLines
  .slice(jobsStart + 1)
  .filter((line) => !isComment(line))
  .map((line) => line.match(/^ {2}([A-Za-z][A-Za-z0-9_-]*):\s*$/))
  .filter((match) => match !== null)
  .map((match) => match[1])

// The gate job's own block: from its header to the next job at the same indent.
const gateStart = releaseLines.findIndex((line) => line === `  ${GATE_JOB}:`)
const gateRest = releaseLines.slice(gateStart + 1)
const gateEnd = gateRest.findIndex((line) => /^ {2}[A-Za-z]/.test(line))
const gateBlock = gateEnd === -1 ? gateRest : gateRest.slice(0, gateEnd)

const takeWhile = (items, keep) => {
  const stop = items.findIndex((item) => !keep(item))
  return stop === -1 ? items : items.slice(0, stop)
}

// `needs:` is either an inline flow sequence or a block list of `- name`.
const NEEDS_ITEM = /^ {6}- ([A-Za-z][A-Za-z0-9_-]*)\s*$/
const readNeeds = (block) => {
  const at = block.findIndex((line) => /^ {4}needs:/.test(line))
  if (at === -1) return []
  const inline = block[at].match(/needs:\s*\[(.*)\]/)
  if (inline) return inline[1].split(',').map((name) => name.trim()).filter(Boolean)
  return takeWhile(block.slice(at + 1), (line) => NEEDS_ITEM.test(line)).map((line) => line.match(NEEDS_ITEM)[1])
}

const backstopped = jobNames.filter((name) => name !== GATE_JOB)
const declared = readNeeds(gateBlock)
const unguarded = backstopped
  .filter((name) => !declared.includes(name))
  .map((name) => `${RELEASE_WORKFLOW}: job \`${name}\` is not in \`${GATE_JOB}\`'s needs — it can skip and the release still reports green`)
const phantom = declared
  .filter((name) => !backstopped.includes(name))
  .map((name) => `${RELEASE_WORKFLOW}: \`${GATE_JOB}\` needs \`${name}\`, which is not a job in this workflow`)

const conditionMissing = gateBlock.some((line) => line.trim() === GATE_CONDITION)
  ? []
  : [`${RELEASE_WORKFLOW}: \`${GATE_JOB}\` must be \`${GATE_CONDITION}\` or it is skipped by the failure it exists to catch`]

// ---- A gate that cannot run must not report success -------------------------

const brokenParser = []
if (jobsStart === -1) brokenParser.push(`no \`jobs:\` key in ${RELEASE_WORKFLOW} — the parser is broken, not the workflow`)
if (gateStart === -1) brokenParser.push(`no \`${GATE_JOB}\` job in ${RELEASE_WORKFLOW} — the release backstop is gone`)
if (jobNames.length < 8) brokenParser.push(`parsed only ${jobNames.length} jobs from ${RELEASE_WORKFLOW} — the parser is broken`)
if (workflowFiles.length < 4) brokenParser.push(`found only ${workflowFiles.length} workflow files — this check is scanning nothing`)

const failures = [...brokenParser, ...unreviewed, ...stale, ...unguarded, ...phantom, ...conditionMissing]

if (failures.length > 0) {
  console.error('release gate guard: FAIL')
  for (const failure of failures) console.error(`  - ${failure}`)
  console.error('\nA release that cannot publish must go red. Fix the workflow, or add a')
  console.error('reviewed entry to REVIEWED_TOLERANCES with the reason it is safe.')
  process.exit(1)
}

console.error(
  `release gate guard: OK (${workflowFiles.length} workflows scanned, ${backstopped.length} release jobs all backstopped by ${GATE_JOB})`,
)
