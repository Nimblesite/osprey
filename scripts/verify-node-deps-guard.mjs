#!/usr/bin/env node
// Every Makefile recipe that runs a tool out of a node subproject needs that
// project's node_modules, and for a long time nothing installed one: `make
// build` on a tree that had never run `make setup` reached `tsc -b` and died
// with "tsc: command not found". The install is now a make prerequisite (the
// `%/node_modules/.package-lock.json` pattern rule), but a prerequisite is only
// as good as the next person's memory — the next `cd website && npm run x`
// added without one reintroduces the same failure, and it reproduces only on a
// machine that has never installed that project. This check makes the omission
// fail the build instead.
//
// It is deliberately a script, not a new CI job: it runs inside `make lint` and
// inside the already-required "Build, Format & Analyse" job, so it blocks a
// merge without changing the pinned required-check list.
import { readFileSync } from 'node:fs'

const MAKEFILE = 'Makefile'

// Subproject directory -> the make variable holding its install stamp. A recipe
// running a node tool in one of these must list that variable as a prerequisite.
const GUARDED = new Map([
  ['vscode-extension', 'EXT_NODE_DEPS'],
  ['website', 'WEBSITE_NODE_DEPS'],
  ['webcompiler', 'WEBCOMPILER_NODE_DEPS'],
  ['examples/projects/modules/web', 'BANK_WEB_NODE_DEPS'],
  ['examples/projects/modules/e2e', 'BANK_E2E_NODE_DEPS'],
])

// Make variables that resolve to a guarded directory.
const DIR_VARS = new Map([['$(EXT_DIR)', 'vscode-extension']])

// The install rule itself runs npm without a stamp prerequisite — it *is* the
// stamp. Exempting it by name keeps the rule from having to depend on itself.
const INSTALL_RULE = '%/node_modules/.package-lock.json'

const NODE_TOOL = /^(npm|npx|yarn|pnpm|vsce|tsc)\b|(^|[/\s])node_modules\/\.bin\//

const resolveDir = (raw) => DIR_VARS.get(raw) ?? raw.replace(/^\.\//, '')

// Fold `\`-continued physical lines into one logical line so that a compound
// `cd x && npm ... && ./node_modules/.bin/y` is analysed as a single command.
const foldContinuations = (text) =>
  text.split('\n').reduce((acc, line) => {
    const pending = acc.length > 0 && acc[acc.length - 1].endsWith('\\')
    if (pending) acc[acc.length - 1] = acc[acc.length - 1].slice(0, -1).trimEnd() + ' ' + line.trim()
    else acc.push(line)
    return acc
  }, [])

const isTargetLine = (line) =>
  !line.startsWith('\t') && !line.startsWith('#') && line.trim() !== '' && /^[^=]*?:(?!=)/.test(line)

const parseTarget = (line) => {
  const colon = line.indexOf(':')
  return { names: line.slice(0, colon).trim(), prereqs: line.slice(colon + 1).split('#')[0].trim() }
}

// Group the Makefile into { names, prereqs, recipe[] } rules.
const parseRules = (lines) =>
  lines.reduce((rules, line) => {
    if (isTargetLine(line)) rules.push({ ...parseTarget(line), recipe: [] })
    else if (line.startsWith('\t') && rules.length > 0) rules[rules.length - 1].recipe.push(line.slice(1))
    return rules
  }, [])

const stripPrefix = (segment) => segment.trim().replace(/^[-@+]+\s*/, '')

// Which guarded directory does this command segment touch, given the directory
// a preceding `cd` in the same compound command put us in?
const dirTouched = (segment, cwd) => {
  const cmd = stripPrefix(segment)
  const prefix = cmd.match(/^npm\s+--prefix\s+(\S+)/)
  if (prefix) return resolveDir(prefix[1])
  if (cmd.startsWith('echo') || cmd.startsWith('cd ')) return null
  return NODE_TOOL.test(cmd) ? cwd : null
}

// Walk one logical recipe line, tracking `cd` across `&&`/`;` segments.
const dirsUsedBy = (logicalLine) =>
  logicalLine.split(/&&|;/).reduce(
    (state, segment) => {
      const cd = stripPrefix(segment).match(/^cd\s+(\S+)/)
      if (cd) return { ...state, cwd: resolveDir(cd[1]) }
      const dir = dirTouched(segment, state.cwd)
      if (dir !== null && GUARDED.has(dir)) state.dirs.add(dir)
      return state
    },
    { cwd: null, dirs: new Set() },
  ).dirs

const violationsFor = (rule) =>
  [...new Set(rule.recipe.flatMap((line) => [...dirsUsedBy(line)]))]
    .filter((dir) => !rule.prereqs.includes(`$(${GUARDED.get(dir)})`))
    .map((dir) => `${rule.names}: runs a node tool in ${dir}/ but does not require $(${GUARDED.get(dir)})`)

const rules = parseRules(foldContinuations(readFileSync(MAKEFILE, 'utf8'))).filter(
  (rule) => rule.names !== INSTALL_RULE,
)

// A gate that cannot run must not report success. If the parser stops finding
// rules, or stops finding any guarded npm usage at all, it has been defeated by
// a Makefile restructure and is no longer checking anything.
const guardedUses = rules.filter((rule) => violationsFor(rule).length > 0 || rule.recipe.some((l) => dirsUsedBy(l).size > 0))
const failures = rules.flatMap(violationsFor)

if (rules.length < 20) failures.push(`parsed only ${rules.length} rules from ${MAKEFILE} — the parser is broken, not the Makefile`)
if (guardedUses.length === 0) failures.push(`found no node-tool recipes at all in ${MAKEFILE} — this check is no longer checking anything`)

if (failures.length > 0) {
  console.error('node dependency guard: FAIL')
  for (const failure of failures) console.error(`  - ${failure}`)
  console.error('\nAdd the stamp variable to the target\'s prerequisites so make installs before it runs.')
  process.exit(1)
}

console.error(`node dependency guard: OK (${guardedUses.length} node-tool recipes, all guarded)`)
