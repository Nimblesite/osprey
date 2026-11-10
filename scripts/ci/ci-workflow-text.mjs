// Reading workflows and Makefiles as text for the CI contract tests: one job's
// block of a workflow, and the commands of one make target across the root
// Makefile and the scripts/make fragments it includes.
import { readFileSync, readdirSync } from 'node:fs'

export const workflowJobs = (file) => readFileSync(file, 'utf8').split(/(?=^  [\w-]+:\s*$)/m)
export const job = (name, file = '.github/workflows/release.yml') => workflowJobs(file).find((block) => block.startsWith(`  ${name}:`)) ?? ''

// The root Makefile and the fragments it includes, where private steps live.
export const ROOT_MAKEFILES = ['Makefile', ...readdirSync('scripts/make').filter((f) => f.endsWith('.mk')).map((f) => `scripts/make/${f}`)]

export const makeRecipe = (name, files = ROOT_MAKEFILES) => {
  const commands = []
  let active = false
  for (const line of [files].flat().flatMap((file) => [...readFileSync(file, 'utf8').split('\n'), '.end-of-file:'])) {
    if (line.startsWith(`${name}:`)) { active = true; continue }
    if (active && /^[^\s#][^:]*:/.test(line)) break
    if (active && line.startsWith('\t')) commands.push(line.trim())
  }
  return commands
}
