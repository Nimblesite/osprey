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
  'source = Path(__file__).resolve().parents[2] / "src/osprey/workbench/assets"',
  'destination = Path(sys.argv[1])',
  `files = ${JSON.stringify(Object.keys(proofPayloads))}`,
  'for name in files:',
  '    assert (destination / name).read_bytes() == (source / name).read_bytes(), name',
  'actual = {path.relative_to(destination).as_posix() for path in (destination / "proof-libraries").rglob("*") if path.is_file()}',
  'assert actual == {name for name in files if name.startswith("proof-libraries/")}',
].join('\n') + '\n'

function writeProofAssets(root, platform) {
  for (const [name, bytes] of Object.entries(proofPayloads)) {
    for (const directory of ['src/osprey/workbench/assets', `${platform}/workbench`]) {
      const file = join(root, directory, name)
      mkdirSync(dirname(file), { recursive: true })
      writeFileSync(file, bytes)
    }
  }
  mkdirSync(join(root, 'scripts/proof'), { recursive: true })
  writeFileSync(join(root, 'scripts/proof/workbench-proof-assets.py'), proofStager)
}

function restoreStagedBundle(root, restoreJob) {
  cpSync(join(root, 'src/toolchain/vscode-extension/bin'), join(root, 'extension/bin'), { recursive: true })
  const zipped = spawnSync('zip', ['-qr', 'src/toolchain/vscode-extension/osprey-fixture.vsix', 'extension'], { cwd: root, encoding: 'utf8' })
  assert.equal(zipped.status, 0, zipped.stderr)
  rmSync(join(root, 'extension'), { recursive: true })
  rmSync(join(root, 'src/toolchain/vscode-extension/bin'), { recursive: true })
  const block = restoreJob.split('- name: Restore bundled extension assets\n')[1] ?? ''
  const run = block.match(/^ {8}run: \|\n((?: {10}.*\n)+)/m)?.[1] ?? ''
  assert.notEqual(run, '', 'CI must restore assets from the built VSIX')
  const result = spawnSync('bash', ['-euo', 'pipefail', '-c', run.split('\n').map(line => line.slice(10)).join('\n')], { cwd: root, encoding: 'utf8' })
  assert.equal(result.status, 0, result.stdout + result.stderr)
}

export const nativeArchives = ['fiber_runtime', 'http_runtime'].flatMap(name => ['', '_gc', '_arc'].map(mode => `lib${name}${mode}.a`))
// [NATIVE-SHM-WASM] A valid module with exported memory and one empty function exported under every
// name a live shared-boundary module exports; packaging checks the names and never executes it.
const boundaryExports = ['_start', 'osprey_app_argument', 'osprey_app_open', 'osprey_app_event', 'osp_native_open',
  'osp_native_frame_commit', 'osp_native_frame_discard', 'osp_native_acquire', 'osp_native_child', 'osp_native_length',
  'osp_native_view', 'osp_native_release', 'malloc', 'free']
const leb = (n) => n < 128 ? [n] : [(n & 127) | 128, ...leb(n >> 7)]
const section = (id, body) => [id, ...leb(body.length), ...body]
const named = (text) => [text.length, ...Buffer.from(text)]
const exportEntries = [...boundaryExports.flatMap(name => [...named(name), 0, 0]), ...named('memory'), 2, 0]
const wasiCommand = Buffer.from([0, 97, 115, 109, 1, 0, 0, 0,
  ...section(1, [1, 96, 0, 0]), ...section(3, [1, 0]), ...section(5, [1, 0, 1]),
  ...section(7, [boundaryExports.length + 1, ...exportEntries]), ...section(10, [1, 2, 0, 11])])

function writeExecutablePayloads(root, platform) {
  const payloads = [['target/release/osprey', 'osprey'], ['target/osprey-pkg', 'osprey-pkg'],
    ['target/osprey-registry', 'osprey-registry'],
    ['target/package-workspace.wasm', 'package-workspace.wasm'], ['target/workbench.wasm', 'workbench.wasm'],
    ...nativeArchives.map(name => [`src/toolchain/compiler/bin/${name}`, name])]
  for (const dir of ['target/release', 'src/toolchain/compiler/bin', platform, 'src/toolchain/vscode-extension/scripts', 'src/toolchain/vscode-extension/out/client/src/osprey-ui']) mkdirSync(join(root, dir), { recursive: true })
  writeFileSync(join(root, 'src/toolchain/vscode-extension/scripts/verify-package-client.mjs'), readFileSync('src/toolchain/vscode-extension/scripts/verify-package-client.mjs'))
  for (const name of ['wasi', 'wasi-runner', 'wasi-worker']) writeFileSync(join(root, `src/toolchain/vscode-extension/out/client/src/osprey-ui/${name}.js`), 'export {};\n')
  for (const [source, name] of payloads) {
    const bytes = name.endsWith('.wasm') ? wasiCommand : '#!/bin/sh\nexit 97\n'
    for (const file of [source, `${platform}/${name}`]) writeFileSync(join(root, file), bytes, { mode: 0o755 })
  }
}

function writeWorkbenchAssets(root, platform) {
  for (const directory of ['src/osprey/workbench/lessons', `${platform}/workbench/lessons`]) {
    mkdirSync(join(root, directory), { recursive: true })
    writeFileSync(join(root, directory, 'catalogue.json'), JSON.stringify({ format: 1, lessons: ['and-swap'] }))
    writeFileSync(join(root, directory, 'and-swap.json'), JSON.stringify({ slug: 'and-swap', blocks: [] }))
  }
  mkdirSync(join(root, 'src/toolchain/website/src/assets/images'), { recursive: true })
  for (const file of ['src/toolchain/website/src/assets/images/logo.png', `${platform}/workbench/osprey-logo.png`]) writeFileSync(join(root, file), 'fixture logo')
  // The bundle rule copies src/osprey/workbench/assets/. as-is, so the study image
  // the client verifier requires comes from that directory.
  for (const directory of ['src/osprey/workbench/assets', `${platform}/workbench`]) mkdirSync(join(root, directory), { recursive: true })
  for (const file of ['src/osprey/workbench/assets/chalk-study.png', `${platform}/workbench/chalk-study.png`]) writeFileSync(join(root, file), 'fixture study image')
}

function writePresentationAssets(root, platform) {
  for (const directory of ['src/osprey/workbench/assets/fonts', `${platform}/workbench/fonts`]) {
    mkdirSync(join(root, directory), { recursive: true })
    for (const file of ['STIXTwoMath-Regular.ttf', 'STIX-Two-Math-LICENSE.txt', 'STIXTwoText.ttf', 'STIXTwoText-Italic.ttf', 'STIX-Two-Text-LICENSE.txt', 'Geist-LICENSE.txt']) writeFileSync(join(root, directory, file), 'fixture font')
  }
  for (const directory of ['src/osprey/workbench/assets', `${platform}/workbench`]) {
    for (const file of ['workbench.css', 'workbench-fonts.css']) writeFileSync(join(root, directory, file), '/* fixture style */')
  }
}

function writeSymbolAssets(root, platform) {
  const payloads = { 'symbols.json': JSON.stringify({ moon: 'icons/moon.svg' }),
    'icons/moon.svg': '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M1 1h1v1H1z"/></svg>',
    'icons/LICENSE.txt': 'fixture icon license\n' }
  for (const directory of ['src/osprey/workbench/assets', `${platform}/workbench`]) {
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
  const platform = `src/toolchain/vscode-extension/bin/${host}`
  try {
    writeBundlePayloads(root, platform)
    restoreStagedBundle(root, restoreJob)
    change(root, platform)
    return spawnSync('make', ['-f', resolve('scripts/make/extension.mk'), '_vsix_verify',
      'BIN=target/release/osprey', 'EXT_DIR=src/toolchain/vscode-extension', `EXT_PLATFORM=${host}`, 'RTB=src/toolchain/compiler/bin',
      `NATIVE_RUNTIME_ARCHIVES=${nativeArchives.map(name => `src/toolchain/compiler/bin/${name}`).join(' ')}`], { cwd: root, encoding: 'utf8' })
  } finally { rmSync(root, { recursive: true, force: true }) }
}

