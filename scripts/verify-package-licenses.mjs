#!/usr/bin/env node
// Retain the reviewed license bundle when package source or metadata changes.
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = fileURLToPath(new URL('../packages/', import.meta.url))
const AUDIT = JSON.parse(readFileSync(join(ROOT, 'license-audit.json'), 'utf8'))
const HOLDER = 'NIMBLESITE PTY LTD, Australia'
const MARKER = '# osprey: flavor=python'
const CHANGE_NOTICE = '# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.'
const read = (path) => readFileSync(path, 'utf8')
const digest = (text) => createHash('sha256').update(text).digest('hex')
const requireThat = (ok, message) => { if (!ok) throw new Error(message) }
const filesIn = (dir) => readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
  entry.isDirectory() ? filesIn(join(dir, entry.name)) : [join(dir, entry.name)])

const packageNames = readdirSync(ROOT, { withFileTypes: true })
  .filter((entry) => entry.isDirectory() && !entry.name.startsWith('.'))
  .map((entry) => entry.name).sort()
requireThat(JSON.stringify(packageNames) === JSON.stringify(Object.keys(AUDIT.packages).sort()),
  'Every package needs an entry in packages/license-audit.json')
requireThat(AUDIT.copyright_holder === HOLDER, 'Unexpected Osprey copyright holder')

const verifySource = (dir, manifest) => {
  const sources = manifest.match(/^python_sources = \[(.*)\]$/m)?.[1] ?? ''
  for (const path of [...sources.matchAll(/"([^"]+)"/g)].map((match) => match[1])) {
    const source = read(join(dir, path))
    requireThat(source.startsWith(`${MARKER}\n${CHANGE_NOTICE}\n`), `${dir}/${path}: missing port change notice`)
  }
}

const verifyUpstream = (dir, upstream) => {
  requireThat(upstream.file.startsWith('LICENSES/') && !upstream.file.includes('..'), 'Invalid upstream license path')
  const text = read(join(dir, upstream.file))
  requireThat(text.length > 20 && digest(text) === upstream.sha256, `${dir}/${upstream.file}: license text changed`)
  requireThat(upstream.repository.startsWith('https://github.com/') && upstream.source.startsWith('https://'),
    `${dir}: missing upstream provenance`)
}

const verifyPackage = (name) => {
  const dir = join(ROOT, name)
  const entry = AUDIT.packages[name]
  const manifest = read(join(dir, 'osprey.toml'))
  const license = read(join(dir, 'LICENSE'))
  const notice = read(join(dir, 'NOTICE.md'))
  requireThat(manifest.match(/^license = "([^"]+)"$/m)?.[1] === entry.license, `${name}: unreviewed license expression`)
  requireThat(license.includes(HOLDER) && digest(license) === entry.original_license_sha256, `${name}: MIT license changed`)
  requireThat(notice.includes(entry.license) && notice.includes(HOLDER), `${name}: attribution or license summary missing`)
  for (const upstream of entry.upstreams) verifyUpstream(dir, upstream)
  const expected = [...new Set(entry.upstreams.map((upstream) => join(dir, upstream.file)))].sort()
  const actual = expected.length ? filesIn(join(dir, 'LICENSES')).sort() : []
  requireThat(JSON.stringify(actual) === JSON.stringify(expected), `${name}: unreviewed or missing license files`)
  verifySource(dir, manifest)
}

for (const name of packageNames) verifyPackage(name)
console.log(`Package licenses: ${packageNames.length} reviewed packages; MIT holder, upstream texts, SPDX metadata and change notices verified.`)
