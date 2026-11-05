#!/usr/bin/env node
// [REPO-ROOT-LAYOUT] The repository root holds only the entries listed here, and
// never a log or a stray file. Code belongs under src/; logs belong under target/.
import { spawnSync } from 'node:child_process'
import { readdirSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const ALLOWED_TRACKED = new Set([
  '.agents', '.cargo', '.claude', '.clinerules', '.config', '.cursor', '.opencode',
  '.devcontainer', '.dockerignore', '.gitattributes', '.github', '.gitignore',
  '.vscode', '.windsurf', 'AGENTS.md', 'CONTRIBUTING.md',
  'LICENSE', 'Makefile', 'README.md', 'SECURITY.md',   'docs', 'scripts', 'src',
  ])

/** Local, ignored files that tools keep in the root by design. */
export const ALLOWED_LOCAL_FILES = new Set(['.DS_Store', '.env', '.mcp.json'])

export function rootViolations({ tracked, files }) {
  const known = new Set(tracked)
  const violations = []
  for (const entry of known) {
    if (!ALLOWED_TRACKED.has(entry)) violations.push(`${entry}: not an allowed root entry; move it under src/ or docs/`)
  }
  for (const file of files) {
    if (/\.log(\.|$)/i.test(file)) violations.push(`${file}: log files are embargoed from the root; write them under target/`)
    else if (!known.has(file) && !ALLOWED_LOCAL_FILES.has(file)) violations.push(`${file}: stray file in the root; delete it or fix what writes it`)
  }
  return violations.sort()
}

function main() {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..')
  const listed = spawnSync('git', ['ls-files'], { cwd: root, encoding: 'utf8', maxBuffer: 1 << 28 })
  if (listed.status !== 0) throw new Error(`git ls-files failed: ${listed.stderr}`)
  const tracked = [...new Set(listed.stdout.split('\n').filter(Boolean).map((path) => path.split('/')[0]))]
  const files = readdirSync(root, { withFileTypes: true }).filter((entry) => !entry.isDirectory()).map((entry) => entry.name)
  const violations = rootViolations({ tracked, files })
  for (const violation of violations) console.error(`root layout: ${violation}`)
  process.exit(violations.length === 0 ? 0 : 1)
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main()
