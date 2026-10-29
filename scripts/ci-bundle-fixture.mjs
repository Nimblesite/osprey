import assert from 'node:assert/strict'
import { readFileSync, mkdtempSync, rmSync, mkdirSync, writeFileSync, cpSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'

const proofPayloads = {
  'proof-libraries.json': JSON.stringify({ libraries: [{ title: 'Fixture proof', path: 'proof-libraries/fixture.ndjson' }] }),
  'proof-libraries/fixture.ndjson': 'fixture proof export\n',
  'proof-libraries/LICENSE.lean4': 'fixture Lean license\n',
  'proof-libraries/LICENSE.openai-math': 'fixture OpenAI license\n',
}
export const proofAssets = Object.keys(proofPayloads).map(name => `workbench/${name}`)

// This fixture isolates Make's staged-payload contract. The real proof stager
// has its own corruption, inventory, license and symlink regressions.
const proofStager = [
  'from pathlib import Path', 'import sys',
  'assert sys.argv[2:] == ["--check"]',
  'source = Path(__file__).resolve().parents[1] / "apps/workbench/assets"',
  'destination = Path(sys.argv[1])',
  `files = ${JSON.stringify(Object.keys(proofPayloads))}`,
  'for name in files:',
  '    assert (destination / name).read_bytes() == (source / name).read_bytes(), name',
  'actual = {path.relative_to(destination).as_posix() for path in (destination / "proof-libraries").rglob("*") if path.is_file()}',
  'assert actual == {name for name in files if name.startswith("proof-libraries/")}',
].join('\n') + '\n'

function writeProofAssets(root, platform) {
  for (const [name, bytes] of Object.entries(proofPayloads)) {
    for (const directory of ['apps/workbench/assets', `${platform}/workbench`]) {
      const file = join(root, directory, name)
      mkdirSync(dirname(file), { recursive: true })
      writeFileSync(file, bytes)
    }
  }
  mkdirSync(join(root, 'scripts'), { recursive: true })
  writeFileSync(join(root, 'scripts/workbench-proof-assets.py'), proofStager)
}

function restoreStagedBundle(root, restoreJob) {
  cpSync(join(root, 'vscode-extension/bin'), join(root, 'extension/bin'), { recursive: true })
  const zipped = spawnSync('zip', ['-qr', 'vscode-extension/osprey-fixture.vsix', 'extension'], { cwd: root, encoding: 'utf8' })
  assert.equal(zipped.status, 0, zipped.stderr)
  rmSync(join(root, 'extension'), { recursive: true })
  rmSync(join(root, 'vscode-extension/bin'), { recursive: true })
  const block = restoreJob.split('- name: Restore bundled extension assets\n')[1] ?? ''
  const run = block.match(/^ {8}run: \|\n((?: {10}.*\n)+)/m)?.[1] ?? ''
  assert.notEqual(run, '', 'CI must restore assets from the built VSIX')
  const result = spawnSync('bash', ['-euo', 'pipefail', '-c', run.split('\n').map(line => line.slice(10)).join('\n')], { cwd: root, encoding: 'utf8' })
  assert.equal(result.status, 0, result.stdout + result.stderr)
}

export const nativeArchives = ['fiber_runtime', 'http_runtime'].flatMap(name => ['', '_gc', '_arc'].map(mode => `lib${name}${mode}.a`))
// A valid command with exported memory and an empty _start; packaging never executes it.
const wasiCommand = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0, 1, 4, 1, 96, 0, 0, 3, 2, 1, 0,
  5, 3, 1, 0, 1, 7, 19, 2, 6, 95, 115, 116, 97, 114, 116, 0, 0, 6, 109, 101, 109, 111, 114, 121, 2, 0, 10, 4, 1, 2, 0, 11])

function writeExecutablePayloads(root, platform) {
  const payloads = [['target/release/osprey', 'osprey'], ['target/osprey-pkg', 'osprey-pkg'],
    ['target/package-workspace.wasm', 'package-workspace.wasm'], ['target/workbench.wasm', 'workbench.wasm'],
    ...nativeArchives.map(name => [`compiler/bin/${name}`, name])]
  for (const dir of ['target/release', 'compiler/bin', platform, 'vscode-extension/scripts']) mkdirSync(join(root, dir), { recursive: true })
  writeFileSync(join(root, 'vscode-extension/scripts/verify-package-client.mjs'), readFileSync('vscode-extension/scripts/verify-package-client.mjs'))
  for (const [source, name] of payloads) {
    const bytes = name.endsWith('.wasm') ? wasiCommand : '#!/bin/sh\nexit 97\n'
    for (const file of [source, `${platform}/${name}`]) writeFileSync(join(root, file), bytes, { mode: 0o755 })
  }
}

function writeWorkbenchAssets(root, platform) {
  for (const directory of ['apps/workbench/lessons', `${platform}/workbench/lessons`]) {
    mkdirSync(join(root, directory), { recursive: true })
    writeFileSync(join(root, directory, 'catalogue.json'), JSON.stringify({ format: 1, lessons: ['and-swap'] }))
    writeFileSync(join(root, directory, 'and-swap.json'), JSON.stringify({ slug: 'and-swap', blocks: [] }))
  }
  mkdirSync(join(root, 'website/src/assets/images'), { recursive: true })
  for (const file of ['website/src/assets/images/logo.png', `${platform}/workbench/osprey-logo.png`]) writeFileSync(join(root, file), 'fixture logo')
  // The bundle rule copies apps/workbench/assets/. as-is, so the study image
  // the client verifier requires comes from that directory.
  for (const directory of ['apps/workbench/assets', `${platform}/workbench`]) mkdirSync(join(root, directory), { recursive: true })
  for (const file of ['apps/workbench/assets/chalk-study.png', `${platform}/workbench/chalk-study.png`]) writeFileSync(join(root, file), 'fixture study image')
}

function writePresentationAssets(root, platform) {
  for (const directory of ['apps/workbench/assets/fonts', `${platform}/workbench/fonts`]) {
    mkdirSync(join(root, directory), { recursive: true })
    for (const file of ['STIXTwoMath-Regular.ttf', 'STIX-Two-Math-LICENSE.txt', 'STIXTwoText.ttf', 'STIXTwoText-Italic.ttf', 'STIX-Two-Text-LICENSE.txt', 'Geist-LICENSE.txt']) writeFileSync(join(root, directory, file), 'fixture font')
  }
  for (const directory of ['apps/workbench/assets', `${platform}/workbench`]) {
    for (const file of ['workbench.css', 'workbench-fonts.css']) writeFileSync(join(root, directory, file), '/* fixture style */')
  }
}

function writeSymbolAssets(root, platform) {
  const payloads = { 'symbols.json': JSON.stringify({ moon: 'icons/moon.svg' }),
    'icons/moon.svg': '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M1 1h1v1H1z"/></svg>',
    'icons/LICENSE.txt': 'fixture icon license\n' }
  for (const directory of ['apps/workbench/assets', `${platform}/workbench`]) {
    mkdirSync(join(root, directory, 'icons'), { recursive: true })
    for (const [name, bytes] of Object.entries(payloads)) writeFileSync(join(root, directory, name), bytes)
  }
}

function writeBundlePayloads(root, platform) {
  writeExecutablePayloads(root, platform)
  writeWorkbenchAssets(root, platform)
  writePresentationAssets(root, platform)
  writeProofAssets(root, platform)
  writeSymbolAssets(root, platform)
}

export function runStagedBundleFixture(restoreJob, change = () => {}) {
  const root = mkdtempSync(join(tmpdir(), 'osprey-staged-bundle-'))
  const host = `${process.platform === 'win32' ? 'win32' : process.platform === 'darwin' ? 'darwin' : 'linux'}-${process.arch === 'arm64' ? 'arm64' : 'x64'}`
  const platform = `vscode-extension/bin/${host}`
  try {
    writeBundlePayloads(root, platform)
    restoreStagedBundle(root, restoreJob)
    change(root, platform)
    return spawnSync('make', ['-f', resolve('scripts/make/extension.mk'), '_vsix_verify',
      'BIN=target/release/osprey', 'EXT_DIR=vscode-extension', `EXT_PLATFORM=${host}`, 'RTB=compiler/bin',
      `NATIVE_RUNTIME_ARCHIVES=${nativeArchives.map(name => `compiler/bin/${name}`).join(' ')}`], { cwd: root, encoding: 'utf8' })
  } finally { rmSync(root, { recursive: true, force: true }) }
}

