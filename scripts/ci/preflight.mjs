// Every check the PR workflow runs, run locally before a push.
// The plan is read from .github/workflows/ci.yml and the workflows it calls, so a check added to
// CI is in the preflight the moment it lands. Every check runs even after one fails, so one red
// can never hide the next, and the coverage gates run last against what the tests just produced.
import { spawnSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { workflowJobs } from './ci-workflow-text.mjs'

const CI = '.github/workflows/ci.yml'
const OUT = 'target/preflight'

// CI splits the Rust tests into shards whose reports a later step merges; locally one
// workspace run writes the same report the coverage gate reads.
const LOCAL_EQUIVALENTS = [[/^make _test_rust_shard RUST_SHARD=\S+$/, 'make _test_rust']]

/** The workflows ci.yml calls, beside ci.yml itself. */
export const workflowFiles = (text = readFileSync(CI, 'utf8')) =>
  [CI, ...[...text.matchAll(/uses: (\.\/\.github\/workflows\/[\w.-]+\.ya?ml)/g)].map((match) => match[1].slice(2))]

/** The values of each matrix combination: inline lists expand, `include` entries are taken whole. */
export function matrixRows(block) {
  const inline = [...block.matchAll(/^ {8}(\w+): \[([^\]]*)\]$/gm)]
    .map(([, key, values]) => values.split(',').map((value) => ({ [key]: value.trim() })))
  const included = block.split(/^ {10}- /m).slice(1).map(includeEntry).filter((row) => Object.keys(row).length)
  const crossed = inline.reduce((rows, options) => rows.flatMap((row) => options.map((option) => ({ ...row, ...option }))), [{}])
  return included.length ? included : crossed
}

function includeEntry(entry) {
  const row = {}
  const lines = entry.replace(/^\s*(?=\w)/, '            ').split('\n')
  for (let index = 0; index < lines.length; index += 1) {
    const match = lines[index].match(/^ {12}(\w+): ?(.*)$/)
    if (!match) continue
    const block = match[2] === '|' ? blockLines(lines, index + 1, 14) : undefined
    row[match[1]] = block ? block.join('\n') : match[2].replace(/^(['"])(.*)\1$/, '$2')
  }
  return row
}

function blockLines(lines, start, indent) {
  const body = []
  for (let index = start; index < lines.length && (lines[index].startsWith(' '.repeat(indent)) || lines[index].trim() === ''); index += 1) {
    if (lines[index].trim()) body.push(lines[index].slice(indent))
  }
  return body
}

/** Every `run:` command of a job's steps, multi-line blocks included. */
export function runCommands(block) {
  const lines = block.slice(Math.max(block.search(/^ {4}steps:$/m), 0)).split('\n')
  return lines.flatMap((line, index) => {
    const match = line.match(/^\s+(?:- )?run: ?(.*)$/)
    if (!match) return []
    const indent = lines.slice(index + 1).find((next) => next.trim())?.match(/^ */)[0].length ?? 0
    return match[1] === '|' ? blockLines(lines, index + 1, indent) : [match[1]]
  })
}

const expand = (command, row) => command.replace(/\$\{\{ matrix\.(\w+) \}\}/g, (_, key) => row[key] ?? '')

/** The host family a job runs on: linux, macos or windows. */
export function jobHost(block, row = {}) {
  const runsOn = expand(block.match(/^ {4}runs-on: (.*)$/m)?.[1] ?? '', row)
  const runner = runsOn.match(/'([^']+)'/)?.[1] ?? runsOn
  const resolved = /\|\|/.test(runsOn) && row.runner ? row.runner : runner
  return /windows/.test(resolved) ? 'windows' : /macos/.test(resolved) ? 'macos' : 'linux'
}

const localCommand = (command) => {
  const plain = command.replace(/^xvfb-run -a /, '').trim()
  return LOCAL_EQUIVALENTS.reduce((text, [pattern, local]) => (pattern.test(text) ? local : text), plain)
}

/** The checks one step runs: every make line, and every line of a matrix row's own `run`. */
const stepChecks = (command, row) => expand(command, row).split('\n')
  .filter((line) => line.trim() && (/\$\{\{ matrix\.run \}\}/.test(command) || /(^|\s)make\s/.test(line)))

/** Each check of one job, expanded per matrix row, with the host it needs. */
function jobChecks(file, block) {
  const name = block.match(/^ {2}([\w-]+):/)?.[1] ?? '?'
  return matrixRows(block).flatMap((row) => runCommands(block)
    .flatMap((command) => stepChecks(command, row))
    .map((command) => ({ job: `${file}:${name}`, host: jobHost(block, row), command: localCommand(command) })))
}

/** The preflight: CI's make checks in workflow order, deduplicated, coverage gates last. */
export function plan(files = workflowFiles()) {
  const seen = new Set()
  const checks = files.flatMap((file) => workflowJobs(file).slice(1).flatMap((block) => jobChecks(file, block)))
    .filter((check) => !seen.has(check.command) && seen.add(check.command))
  const gate = (check) => /_coverage_check/.test(check.command)
  return [...checks.filter((check) => !gate(check)), ...checks.filter(gate)]
}

const HOST = process.platform === 'darwin' ? 'macos' : process.platform === 'win32' ? 'windows' : 'linux'
const runnable = (check) => check.host === HOST || (check.host === 'linux' && HOST === 'macos')

function runCheck(check, index) {
  const log = `${OUT}/${String(index).padStart(2, '0')}-${check.command.replace(/[^\w=-]+/g, '_').slice(0, 80)}.log`
  if (!runnable(check)) return { ...check, status: 'SKIP', log: `needs a ${check.host} host` }
  const started = Date.now()
  const result = spawnSync('bash', ['-c', check.command], { stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 1 << 30 })
  writeFileSync(log, `${result.stdout ?? ''}${result.stderr ?? ''}`)
  const seconds = Math.round((Date.now() - started) / 1000)
  console.log(`${result.status === 0 ? 'PASS' : 'FAIL'} ${check.command} (${seconds}s)`)
  return { ...check, status: result.status === 0 ? 'PASS' : 'FAIL', log, seconds }
}

const summaryLine = (result) => `| ${result.status} | \`${result.command}\` | ${result.job} | ${result.log} |`

function main(args) {
  const only = args.find((arg) => arg.startsWith('--only='))?.slice(7)
  const checks = plan().filter((check) => !only || new RegExp(only).test(check.command))
  if (args.includes('--list')) { for (const check of checks) console.log(`${check.host}\t${check.command}\t${check.job}`); return 0 }
  mkdirSync(OUT, { recursive: true })
  const results = checks.map(runCheck)
  const table = ['| Status | Check | CI job | Log |', '| --- | --- | --- | --- |', ...results.map(summaryLine)].join('\n')
  writeFileSync(`${OUT}/summary.md`, `${table}\n`)
  console.log(`\n${table}`)
  return results.some((result) => result.status === 'FAIL') ? 1 : 0
}

if (import.meta.url === `file://${process.argv[1]}`) process.exit(main(process.argv.slice(2)))
