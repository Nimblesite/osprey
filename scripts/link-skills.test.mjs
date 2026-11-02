import assert from 'node:assert/strict'
import { lstatSync, mkdirSync, mkdtempSync, readFileSync, readlinkSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs'
import { spawnSync } from 'node:child_process'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { test } from 'node:test'
import { linkSkills, problems } from './link-skills.mjs'

const LINKS = join('.claude', 'skills')

// A repository with two skills in .agents/skills and nothing in .claude/skills.
function repo() {
  const root = mkdtempSync(join(tmpdir(), 'link-skills-'))
  for (const name of ['fix-bug', 'spec-check']) {
    mkdirSync(join(root, '.agents', 'skills', name), { recursive: true })
    writeFileSync(join(root, '.agents', 'skills', name, 'SKILL.md'), `---\nname: ${name}\n---\n`)
  }
  writeFileSync(join(root, '.agents', 'skills', 'README.md'), 'not a skill\n')
  mkdirSync(join(root, LINKS), { recursive: true })
  return root
}

const skillThroughLink = (root, name) => readFileSync(join(root, LINKS, name, 'SKILL.md'), 'utf8')

test('a skill with no link under .claude/skills is reported', () => {
  const root = repo()
  assert.deepEqual(problems(root), ['.claude/skills/fix-bug is missing', '.claude/skills/spec-check is missing'])
  rmSync(root, { recursive: true })
})

test('linking makes every skill readable through .claude/skills, by a relative link', () => {
  const root = repo()
  assert.deepEqual(linkSkills(root), [])
  assert.deepEqual(problems(root), [])
  assert.match(skillThroughLink(root, 'fix-bug'), /name: fix-bug/)
  if (process.platform !== 'win32') assert.equal(readlinkSync(join(root, LINKS, 'fix-bug')), '../../.agents/skills/fix-bug')
  rmSync(root, { recursive: true })
})

test('the text file a checkout without symlinks leaves is replaced by a link', () => {
  const root = repo()
  writeFileSync(join(root, LINKS, 'fix-bug'), '../../.agents/skills/fix-bug')
  assert.deepEqual(problems(root), ['.claude/skills/fix-bug is a file, not a link', '.claude/skills/spec-check is missing'])
  assert.deepEqual(linkSkills(root), [])
  assert.match(skillThroughLink(root, 'fix-bug'), /name: fix-bug/)
  rmSync(root, { recursive: true })
})

test('a link to the wrong place is pointed at the skill', () => {
  const root = repo()
  symlinkSync(resolve(root, '.agents', 'skills', 'spec-check'), join(root, LINKS, 'fix-bug'), 'dir')
  assert.match(problems(root)[0], /^\.claude\/skills\/fix-bug is a link to /)
  assert.deepEqual(linkSkills(root), [])
  assert.match(skillThroughLink(root, 'fix-bug'), /name: fix-bug/)
  rmSync(root, { recursive: true })
})

test('a skill only Claude can see is reported and never deleted', () => {
  const root = repo()
  mkdirSync(join(root, LINKS, 'claude-only'))
  writeFileSync(join(root, LINKS, 'claude-only', 'SKILL.md'), '---\nname: claude-only\n---\n')
  const left = linkSkills(root)
  assert.deepEqual(left, ['.claude/skills/claude-only is not in .agents/skills: move it there'])
  assert.ok(lstatSync(join(root, LINKS, 'claude-only', 'SKILL.md')).isFile())
  rmSync(root, { recursive: true })
})

test('a real directory where a link belongs is reported and never deleted', () => {
  const root = repo()
  mkdirSync(join(root, LINKS, 'fix-bug'))
  writeFileSync(join(root, LINKS, 'fix-bug', 'SKILL.md'), 'a copy\n')
  assert.deepEqual(linkSkills(root), ['.claude/skills/fix-bug is a directory, not a link'])
  assert.equal(readFileSync(join(root, LINKS, 'fix-bug', 'SKILL.md'), 'utf8'), 'a copy\n')
  rmSync(root, { recursive: true })
})

test('a link left behind by a removed skill is removed', () => {
  const root = repo()
  assert.deepEqual(linkSkills(root), [])
  rmSync(join(root, '.agents', 'skills', 'spec-check'), { recursive: true })
  assert.deepEqual(problems(root), ['.claude/skills/spec-check is not in .agents/skills: move it there'])
  assert.deepEqual(linkSkills(root), [])
  assert.throws(() => lstatSync(join(root, LINKS, 'spec-check')), { code: 'ENOENT' })
  rmSync(root, { recursive: true })
})

// A commit whose links this clone checked out as text files, as Git does with core.symlinks=false.
function stubbedClone() {
  const root = repo()
  const git = (...args) => spawnSync('git', args, { cwd: root, encoding: 'utf8' }).stdout.trim()
  git('init', '-q')
  git('config', 'core.symlinks', 'false')
  git('config', 'core.autocrlf', 'false')
  git('config', 'user.name', 'test')
  git('config', 'user.email', 'test@example.com')
  mkdirSync(join(root, 'other'))
  writeFileSync(join(root, 'other', 'target'), 'bytes\n')
  const stubs = { '.claude/skills/fix-bug': '../../.agents/skills/fix-bug', '.claude/skills/spec-check': '../../.agents/skills/spec-check', 'other/link': 'target' }
  git('add', '.agents', 'other/target')
  for (const [path, target] of Object.entries(stubs)) {
    writeFileSync(join(root, path), target)
    git('update-index', '--add', '--cacheinfo', `120000,${git('hash-object', '-w', path)},${path}`)
  }
  git('commit', '-q', '-m', 'links')
  return { root, git }
}

const onWindows = { skip: process.platform !== 'win32' }

test('a clone that can make symlinks is switched to them, and Git sees no change', onWindows, () => {
  const { root, git } = stubbedClone()
  assert.deepEqual(linkSkills(root), [])
  assert.match(skillThroughLink(root, 'fix-bug'), /name: fix-bug/)
  assert.equal(git('config', '--bool', 'core.symlinks'), 'true')
  assert.equal(git('status', '--porcelain'), '')
  assert.equal(git('ls-files', '-v').split('\n').filter((line) => line.startsWith('S')).length, 0)
  assert.equal(readlinkSync(join(root, 'other', 'link')), 'target')
  assert.equal(git('ls-files', '-s', '.claude/skills/fix-bug').split(' ')[0], '120000')
  rmSync(root, { recursive: true })
})

test('a link file someone edited is left as it is when the clone is switched', onWindows, () => {
  const { root, git } = stubbedClone()
  writeFileSync(join(root, 'other', 'link'), 'edited by hand')
  assert.deepEqual(linkSkills(root), [])
  assert.equal(readFileSync(join(root, 'other', 'link'), 'utf8'), 'edited by hand')
  assert.notEqual(git('status', '--porcelain'), '')
  rmSync(root, { recursive: true })
})

test('this repository links every skill it has', () => {
  const root = realpathSync(join(dirname(fileURLToPath(import.meta.url)), '..'))
  assert.deepEqual(problems(root), [])
})
