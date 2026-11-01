// One copy of each skill, for every agent. The skill lives in
// .agents/skills/<name>, the cross-client location (agentskills.io), and
// .claude/skills/<name> is a relative link to it, because Claude Code reads
// only .claude/skills. Git stores the links; a Windows clone made without
// symlink support has core.symlinks=false and leaves each one as a text file
// holding its target. Running this script turns them back into links. Where
// Windows allows symlinks (Developer Mode) it also sets core.symlinks=true and
// restores the clone's other links, so the clone then behaves as on macOS and
// Linux; where it refuses them the links are junctions Git is told to ignore.
// `--check` only reports. Usage: node link-skills.mjs [--check] [root]
import { existsSync, lstatSync, mkdirSync, readdirSync, readlinkSync, realpathSync, rmSync, symlinkSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const SOURCE = '.agents/skills'
const LINKS = '.claude/skills'

const entries = (dir) => (existsSync(dir) ? readdirSync(dir, { withFileTypes: true }) : [])
const lstatOf = (path) => (existsSync(path) || isLink(path) ? lstatSync(path) : undefined)

function isLink(path) {
  try { return lstatSync(path).isSymbolicLink() } catch { return false }
}

// The skills: directories under .agents/skills holding a SKILL.md.
export function skills(root) {
  const dir = join(root, SOURCE)
  return entries(dir).filter((entry) => existsSync(join(dir, entry.name, 'SKILL.md'))).map((entry) => entry.name).sort()
}

function resolvesTo(link, target) {
  try { return realpathSync(link) === realpathSync(target) } catch { return false }
}

// What is wrong with .claude/skills/<name>, or undefined when it links to the skill.
function fault(root, name) {
  const link = join(root, LINKS, name)
  const stat = lstatOf(link)
  if (!stat) return 'missing'
  if (!stat.isSymbolicLink()) return stat.isDirectory() ? 'a directory, not a link' : 'a file, not a link'
  return resolvesTo(link, join(root, SOURCE, name)) ? undefined : `a link to ${readlinkSync(link)}`
}

// Entries under .claude/skills that no skill in .agents/skills accounts for.
const strays = (root, names) => entries(join(root, LINKS)).map((entry) => entry.name).filter((name) => !names.includes(name)).sort()

export function problems(root) {
  const names = skills(root)
  const faults = names.flatMap((name) => { const why = fault(root, name); return why ? [`${LINKS}/${name} is ${why}`] : [] })
  return [...faults, ...strays(root, names).map((name) => `${LINKS}/${name} is not in ${SOURCE}: move it there`)]
}

// Relative and with forward slashes, so the same link works from any clone on any OS.
// Returns whether it is a symlink, which is what Git stores, rather than a junction.
function link(root, name) {
  const path = join(root, LINKS, name)
  try { symlinkSync(`../../${SOURCE}/${name}`, path, 'dir') } catch (error) {
    if (process.platform !== 'win32' || error.code !== 'EPERM') throw error
    symlinkSync(resolve(root, SOURCE, name), path, 'junction')
    return false
  }
  return true
}

const git = (root, ...args) => spawnSync('git', args, { cwd: root, encoding: 'utf8' })
const lines = (result) => result.stdout?.split(/\r?\n/).filter(Boolean) ?? []

// The links Git tracks that this clone checked out as text files, as they were committed.
function untouchedStubs(root) {
  const tracked = lines(git(root, 'ls-files', '-s')).filter((line) => line.startsWith('120000 ')).map((line) => line.split('\t')[1])
  const edited = lines(git(root, 'diff', '--name-only'))
  return tracked.filter((path) => lstatOf(join(root, path))?.isFile() && !edited.includes(path))
}

// A clone with core.symlinks=false expects a text file where each link now is. With symlinks Git
// can be told the truth, and its other links restored; with junctions it can only be kept quiet.
function tellGit(root, names, symlinks) {
  if (process.platform !== 'win32' || names.length === 0) return
  if (git(root, 'config', '--bool', 'core.symlinks').stdout?.trim() === 'true') return
  const paths = names.map((name) => `${LINKS}/${name}`)
  if (!symlinks) {
    git(root, 'update-index', '--skip-worktree', '--', ...paths)
    return
  }
  const stubs = untouchedStubs(root)
  git(root, 'config', 'core.symlinks', 'true')
  git(root, 'update-index', '--no-skip-worktree', '--', ...paths)
  if (stubs.length > 0) git(root, 'checkout', '--', ...stubs)
  git(root, 'update-index', '--refresh')
}

// Makes every link, removes links to skills that are gone, and returns what it must not touch:
// a real directory under .claude/skills is somebody's work, so it is reported, never deleted.
export function linkSkills(root) {
  const names = skills(root)
  const stubs = names.filter((name) => fault(root, name) === 'a file, not a link')
  const fixable = names.filter((name) => fault(root, name) && fault(root, name) !== 'a directory, not a link')
  const stale = strays(root, names).filter((name) => isLink(join(root, LINKS, name)))
  for (const name of [...fixable, ...stale]) rmSync(join(root, LINKS, name), { force: true })
  mkdirSync(join(root, LINKS), { recursive: true })
  const symlinks = fixable.map((name) => link(root, name)).every(Boolean)
  tellGit(root, stubs, symlinks)
  return problems(root)
}

function main(args) {
  const check = args.includes('--check')
  const root = resolve(args.find((arg) => arg !== '--check') ?? '.')
  const left = check ? problems(root) : linkSkills(root)
  for (const problem of left) console.error(`link-skills: ${problem}`)
  if (left.length > 0 && check) console.error('link-skills: run `node scripts/link-skills.mjs` to fix the links')
  return left.length === 0 ? 0 : 1
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) process.exitCode = main(process.argv.slice(2))
