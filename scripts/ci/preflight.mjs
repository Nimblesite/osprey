// Every check the PR workflows run, run locally before a push.
// The plan is read from .github/workflows/ci.yml and the workflows it calls, so a check added to
// CI is in the preflight the moment it lands. Each `run:` step (and the emulator action's
// `script:`) is one check, run whole with its working directory and environment. Every check runs
// even after one fails, so one red can never hide the next, and the coverage gates run last.
import { spawnSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { workflowJobs } from './ci-workflow-text.mjs'

const CI = '.github/workflows/ci.yml'
const OUT = 'target/preflight'

// CI splits the Rust tests into shards whose reports a later step merges; locally one
// workspace run writes the merged report the coverage gate reads.
const LOCAL_EQUIVALENTS = [[/^make _test_rust_shard RUST_SHARD=\S+$/, 'make _test_rust'], [/merge_lcov\.py/, '']]

// Runner setup, not checks: tool installs, runner environment, and restoring the build job's
// artifacts (a developer's tree already holds its tools and what the build produced).
const PLUMBING = /\b(apt-get|brew install|choco|udevadm|pacman|winget)\b|GITHUB_(ENV|PATH|OUTPUT|STEP_SUMMARY)|^\s*sudo |^chmod \+x target\/|^tar -xf target\//
const SETUP_STEP = /^(Restore|Unpack|Install|Set ?up) /i

// Checks whose tools exist only on Linux; a Mac runs every other Linux check.
const LINUX_ONLY = /benchmarks\/instruction_gate\.py/

/** The workflows ci.yml calls, beside ci.yml itself. */
export const workflowFiles = (text = readFileSync(CI, 'utf8')) =>
  [CI, ...[...text.matchAll(/uses: (\.\/\.github\/workflows\/[\w.-]+\.ya?ml)/g)].map((match) => match[1].slice(2))]

/** The lines nested under the line at `start`, without their common indent. */
function nested(lines, start) {
  const indent = lines.slice(start).find((line) => line.trim())?.match(/^ */)[0].length ?? 0
  const body = []
  for (let index = start; index < lines.length && (lines[index].trim() === '' || lines[index].startsWith(' '.repeat(indent))); index += 1) {
    body.push(lines[index].slice(indent))
  }
  while (body.length && !body.at(-1).trim()) body.pop()
  return body
}

/** One `key: value` of a mapping, a `|` or `>` block read whole; keys are matched at `indent`. */
function field(lines, key, indent) {
  const at = lines.findIndex((line) => line.startsWith(`${' '.repeat(indent)}${key}:`))
  if (at < 0) return undefined
  const value = lines[at].slice(indent + key.length + 1).trim()
  if (/^\|-?$/.test(value)) return nested(lines, at + 1).join('\n')
  if (/^>-?$/.test(value)) return nested(lines, at + 1).join(' ')
  return value.replace(/^(['"])(.*)\1$/, '$2')
}

/** A simple `env:` mapping at `indent`. */
function envOf(lines, indent) {
  const at = lines.findIndex((line) => line.startsWith(`${' '.repeat(indent)}env:`))
  if (at < 0) return {}
  return Object.fromEntries(nested(lines, at + 1).map((line) => line.match(/^(\w+): ?(.*)$/)).filter(Boolean)
    .map(([, key, value]) => [key, value.replace(/^(['"])(.*)\1$/, '$2')]))
}

const rowOf = (lines) => Object.fromEntries(lines.map((line) => line.match(/^ {12}(\w+):/)?.[1]).filter(Boolean)
  .map((key) => [key, field(lines, key, 12)]))

/** The values of each matrix combination: inline lists cross, `include` entries are taken whole. */
export function matrixRows(block) {
  const head = block.split(/^ {4}steps:$/m)[0]
  const inline = [...head.matchAll(/^ {8}(\w+): \[([^\]]*)\]$/gm)]
    .map(([, key, values]) => values.split(',').map((value) => ({ [key]: value.trim() })))
  const included = head.split(/^ {10}- /m).slice(1).map((entry) => rowOf(`            ${entry}`.split('\n')))
    .filter((row) => Object.keys(row).length)
  return included.length ? included : inline.reduce((rows, options) => rows.flatMap((row) => options.map((option) => ({ ...row, ...option }))), [{}])
}

/** The command of a step: its `run:`, or the `script:` an action runs. */
function commandOf(lines) {
  const withAt = lines.findIndex((line) => line.startsWith('        with:'))
  return field(lines, 'run', 8) ?? (withAt < 0 ? undefined : field(nested(lines, withAt + 1), 'script', 0))
}

/** Each step of a job: its command, condition, directory and env. */
export function steps(block) {
  const body = block.split(/^ {4}steps:$/m)[1] ?? ''
  return body.split(/^ {6}- /m).slice(1).map((text) => `        ${text}`.split('\n')).map((lines) => ({
    name: field(lines, 'name', 8) ?? '',
    run: commandOf(lines),
    condition: field(lines, 'if', 8) ?? '',
    directory: field(lines, 'working-directory', 8) ?? '.',
    env: envOf(lines, 8),
  })).filter((step) => step.run)
}

/** Whether a step runs for this matrix row: failure diagnostics, fallbacks and other rows' steps do not. */
function applies(condition, row) {
  if (/failure\(\)|!= 'success'/.test(condition)) return false
  const part = condition.match(/matrix\.(\w+) == '([^']+)'/)
  return !part || row[part[1]] === part[2]
}

/** Fills in what a local run knows; anything left in `${{ }}` exists only on a runner. */
function substitute(text, row, env) {
  return text.replace(/\$\{\{\s*matrix\.(\w+)(?:\s*\|\|\s*'([^']*)')?\s*\}\}/g, (_, key, fallback) => row[key] ?? fallback ?? '')
    .replace(/\$\{\{\s*env\.(\w+)\s*\}\}/g, (_, key) => env[key] ?? '')
    .replace(/\$\{\{\s*github\.workspace\s*\}\}/g, process.cwd())
}

/** The host family a job runs on: linux, macos or windows. */
export function jobHost(block, row = {}) {
  const runsOn = substitute(block.match(/^ {4}runs-on: (.*)$/m)?.[1] ?? '', row, {})
  return /windows/.test(runsOn) ? 'windows' : /macos/.test(runsOn) ? 'macos' : 'linux'
}

/** The command without its plumbing lines (and their `\\` continuations); empty when nothing is checked. */
function withoutPlumbing(command) {
  const kept = []
  let dropping = false
  for (const line of command.split('\n')) {
    dropping = dropping || PLUMBING.test(line)
    if (!dropping) kept.push(line)
    dropping = dropping && /\\\s*$/.test(line)
  }
  return kept.some((line) => line.trim() && !/^(set -|echo )/.test(line.trim())) ? kept.join('\n') : ''
}

const localCommand = (command) => LOCAL_EQUIVALENTS.reduce((text, [pattern, local]) => (pattern.test(text) ? local : text),
  command.replace(/^xvfb-run -a /gm, '').trim())

/** One planned check, or nothing for runner plumbing. */
function check(origin, step, row, workflowEnv) {
  const command = withoutPlumbing(localCommand(substitute(step.run, row, workflowEnv)))
  if (!command || SETUP_STEP.test(step.name)) return []
  const values = Object.entries(step.env).map(([key, value]) => [key, substitute(value, row, workflowEnv)])
  const runnerOnly = values.filter(([key, value]) => value.includes('${{') && new RegExp(`\\$\\{?${key}\\b`).test(command))
  const env = Object.fromEntries(values.filter(([, value]) => !value.includes('${{')))
  const ciOnly = command.includes('${{') || runnerOnly.length > 0
  return [{ ...origin, name: substitute(step.name, row, workflowEnv), command, directory: step.directory, env, ciOnly }]
}

/** Each check of one job, expanded per matrix row. */
function jobChecks(file, block, workflowEnv) {
  const job = block.match(/^ {2}([\w-]+):/)?.[1] ?? '?'
  return matrixRows(block).flatMap((row) => steps(block).filter((step) => applies(step.condition, row))
    .flatMap((step) => check({ job: `${file.replace('.github/workflows/', '')}:${job}`, host: jobHost(block, row) }, step, row, workflowEnv)))
}

/** The preflight: CI's checks in workflow order, each command once, coverage gates last. */
export function plan(files = workflowFiles()) {
  const seen = new Set()
  const key = (step) => `${step.directory}\n${step.command}`
  const checks = files.flatMap((file) => {
    const workflowEnv = envOf(readFileSync(file, 'utf8').split('\n'), 0)
    return workflowJobs(file).slice(1).flatMap((block) => jobChecks(file, block, workflowEnv))
  }).filter((step) => !seen.has(key(step)) && seen.add(key(step)))
  const gate = (step) => /_coverage_check/.test(step.command)
  return [...checks.filter((step) => !gate(step)), ...checks.filter(gate)]
}

const HOST = process.platform === 'darwin' ? 'macos' : process.platform === 'win32' ? 'windows' : 'linux'

/** Why a check cannot run here, or undefined when it can. */
export function skipReason(step, host = HOST) {
  if (step.ciOnly) return 'reads values only a runner has'
  if (LINUX_ONLY.test(step.command) && host !== 'linux') return 'needs a linux host'
  return step.host === host || (step.host === 'linux' && host === 'macos') ? undefined : `needs a ${step.host} host`
}

function runCheck(step, index) {
  const reason = skipReason(step)
  if (reason) return { ...step, status: 'SKIP', log: reason }
  const log = `${OUT}/${String(index).padStart(3, '0')}.log`
  const started = Date.now()
  const env = { ...process.env, RUNNER_TEMP: `${process.cwd()}/${OUT}/runner-temp`, ...step.env }
  const result = spawnSync('bash', ['-eo', 'pipefail', '-c', step.command], { cwd: step.directory, env, stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 1 << 30 })
  writeFileSync(log, `$ ${step.command}\n${result.stdout ?? ''}${result.stderr ?? ''}`)
  const status = result.status === 0 ? 'PASS' : 'FAIL'
  console.log(`${status} ${step.job} ${step.name} (${Math.round((Date.now() - started) / 1000)}s) ${log}`)
  return { ...step, status, log }
}

export const firstLine = (command) => command.split('\n').find((line) => line.trim() && !/^set -/.test(line))?.trim() ?? command
const summaryLine = (result) => `| ${result.status} | ${result.job} | ${result.name} | \`${firstLine(result.command).replaceAll('|', '\\|')}\` | ${result.log} |`

function main(args) {
  const only = args.find((arg) => arg.startsWith('--only='))?.slice(7)
  const checks = plan().filter((step) => !only || new RegExp(only).test(`${step.job} ${step.name} ${step.command}`))
  if (args.includes('--list')) {
    for (const step of checks) console.log(`${skipReason(step) ? 'SKIP' : 'RUN '}\t${step.job}\t${step.name}\t${firstLine(step.command)}`)
    return 0
  }
  mkdirSync(`${OUT}/runner-temp`, { recursive: true })
  const results = checks.map(runCheck)
  const table = ['| Status | CI job | Step | Command | Log |', '| --- | --- | --- | --- | --- |', ...results.map(summaryLine)].join('\n')
  writeFileSync(`${OUT}/summary.md`, `${table}\n`)
  console.log(`\n${table}`)
  return results.some((result) => result.status === 'FAIL') ? 1 : 0
}

if (import.meta.url === `file://${process.argv[1]}`) process.exit(main(process.argv.slice(2)))
