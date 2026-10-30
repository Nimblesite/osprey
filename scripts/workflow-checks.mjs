// The check-run names a workflow reports on a pull request, read from its YAML.
// The `main` ruleset requires every one of them by name, so this list and the
// ruleset must stay equal; scripts/verify-branch-protection.mjs compares them.
//
// GitHub names a job's check after its `name:` (or its key when it has none).
// A job that calls a reusable workflow with `uses:` reports one check per job
// of the called workflow, named `<caller name> / <called job name>`. A gated
// `uses:` job reports under its own name when its `if:` skips it, so only a
// manual-dispatch gate is allowed there: on a pull request it always skips.

import { readFileSync } from 'node:fs'
import { join } from 'node:path'

const WORKFLOW_DIR = '.github/workflows'

const indentOf = (line) => line.length - line.trimStart().length
const unquote = (value) => value.replace(/^['"]|['"]$/g, '')

const jobField = (job, line) => {
  const field = line.match(/^\s{4}(name|if|uses):\s*(.+?)\s*$/)
  if (field) job[field[1]] = unquote(field[2])
}

// Every job in the `jobs:` block: its key and its `name:`, `if:` and `uses:`.
export const scanJobs = (source) => {
  const lines = source.replace(/\r\n/g, '\n').split('\n')
  const start = lines.findIndex((l) => /^jobs:\s*$/.test(l))
  const jobs = []
  for (const line of start < 0 ? [] : lines.slice(start + 1)) {
    if (line.trim() === '' || line.trimStart().startsWith('#')) continue
    if (indentOf(line) === 0) break
    const key = line.match(/^\s{2}([\w-]+):\s*$/)
    if (key) jobs.push({ key: key[1], name: key[1], if: '', uses: '' })
    else if (jobs.length > 0) jobField(jobs[jobs.length - 1], line)
  }
  return jobs
}

const calledFile = (uses) => uses.match(/^\.\/\.github\/workflows\/(.+\.ya?ml)$/)?.[1]

// The check names `file` reports, prefixed the way a caller's `uses:` job sees them.
export const workflowChecks = (file, read = (f) => readFileSync(join(WORKFLOW_DIR, f), 'utf8'), prefix = '') =>
  scanJobs(read(file)).flatMap((job) => {
    const name = `${prefix}${job.name}`
    const called = calledFile(job.uses)
    return called && !job.if ? workflowChecks(called, read, `${name} / `) : [name]
  })

// `uses:` jobs gated by anything but manual dispatch: their checks change with the gate.
export const gatedCalls = (file, read = (f) => readFileSync(join(WORKFLOW_DIR, f), 'utf8')) =>
  scanJobs(read(file)).flatMap((job) => {
    const called = calledFile(job.uses)
    if (!called) return []
    if (job.if) return /^(\$\{\{\s*)?github\.event_name == 'workflow_dispatch'(\s*\}\})?$/.test(job.if) ? [] : [`${file}: ${job.key}`]
    return gatedCalls(called, read)
  })
