import { appendFileSync, copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const helper = fileURLToPath(import.meta.url)
const pause = () => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 20)
const shellQuote = value => `'${value.replaceAll("'", "'\\''")}'`

function executable(root, name, mode) {
  const path = join(root, name)
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, `#!/bin/sh\nexec ${shellQuote(process.execPath)} ${shellQuote(helper)} ${mode} "$@"\n`, { mode: 0o755 })
}

function prepare(root) {
  for (const source of ['kernel', 'export_check']) {
    const destination = join(root, 'examples/proof', source)
    mkdirSync(destination, { recursive: true })
    copyFileSync(`examples/proof/${source}/expectedoutput`, join(destination, 'expectedoutput'))
  }
  executable(root, 'target/osprey', 'compiler')
  executable(root, 'bin/node', 'wasm')
  executable(root, 'bin/python3', 'corpus')
  executable(root, 'bin/zsh', 'benchmark')
  const includes = ['tests.mk', 'proof.mk'].filter(name => existsSync(`scripts/make/${name}`))
  writeFileSync(join(root, 'Makefile'), 'SHELL := /bin/bash\nBIN := target/osprey\nMKDIR := mkdir -p\n' +
    includes.map(name => `include ${resolve('scripts/make', name)}\n`).join('') +
    '_test_disciplines: ; @python3 fixture-disciplines\n')
}

function staleTargets(root) {
  const targets = ['_test_proof_ci', '_test_proof_kernel', '_test_disciplines',
    '_test_proof_openai', '_test_proof_lean_core', '_test_proof_unicode',
    '_test_proof_batch_gc', '_test_proof_batch_arc', '_test_proof_batch_wasm']
  for (const name of targets) writeFileSync(join(root, name), '')
}

export function runKernelSchedule(environment = {}) {
  return runSchedule('_test_proof_kernel', environment)
}

export function runSchedule(target, environment = {}) {
  const root = mkdtempSync(join(tmpdir(), 'osprey-proof-schedule-'))
  try {
    prepare(root)
    if (environment.FIXTURE_STALE_TARGETS) staleTargets(root)
    const result = spawnSync('make', ['-j3', target], {
      cwd: root, encoding: 'utf8', timeout: 15000,
      env: { ...process.env, ...environment, FIXTURE_ROOT: root, PATH: `${root}/bin:${process.env.PATH}` },
    })
    return { ...result, calls: existsSync(join(root, 'calls')) ? readFileSync(join(root, 'calls'), 'utf8').trim().split('\n').map(JSON.parse) : [] }
  } finally { rmSync(root, { recursive: true, force: true }) }
}

export function benchmarkSetup(compiler) {
  const source = readFileSync('benchmarks/proof/run.sh', 'utf8')
  const boundary = source.indexOf('\nhave()')
  if (boundary < 0) throw new Error('proof benchmark setup boundary moved')
  const script = source.slice(0, boundary)
  const env = { ...process.env }
  delete env.OSPREY_BIN
  if (compiler !== undefined) env.OSPREY_BIN = compiler
  return spawnSync('zsh', ['-c', script + '\nprint -rn -- "$OSP"', resolve('benchmarks/proof/run.sh')], { encoding: 'utf8', env })
}

function barrier(root, memory) {
  writeFileSync(join(root, `started-${memory}`), '')
  const deadline = Date.now() + 2000
  while (!['gc', 'arc', 'wasm32'].every(backend => existsSync(join(root, `started-${backend}`)))) {
    if (Date.now() > deadline) throw new Error('kernel backends did not start concurrently')
    pause()
  }
}

function output(root, source) {
  if (source.endsWith('/tests')) return `passed=400 failed=${process.env.FIXTURE_BAD_TAP ? 1 : 0}\n`
  return readFileSync(join(root, 'examples/proof', source === '.' ? 'export_check' : 'kernel', 'expectedoutput'), 'utf8')
}

function compiler(args) {
  const root = process.env.FIXTURE_ROOT
  const source = args[0]
  const memory = args.includes('--target=wasm32') ? 'wasm32' : args.includes('--memory=arc') ? 'arc' : 'gc'
  appendFileSync(join(root, 'calls'), JSON.stringify({ source, memory }) + '\n')
  if (source === 'examples/proof/kernel') barrier(root, memory)
  if (process.env.FIXTURE_FAIL_BACKEND === memory) process.exit(17)
  if (!args.includes('--compile')) return process.stdout.write(output(root, source))
  writeFileSync(args[args.indexOf('-o') + 1], JSON.stringify({ source }))
}

function wasm(args) {
  const binary = args[1]
  const text = output(process.env.FIXTURE_ROOT, JSON.parse(readFileSync(binary, 'utf8')).source)
  writeFileSync(binary.replace(/\.wasm$/, '.wasm.stdout.txt'), text)
  if (args[2] && text !== readFileSync(args[2], 'utf8')) process.exit(18)
  process.stdout.write(text)
}

if (process.argv[2] === 'compiler') compiler(process.argv.slice(3))
if (process.argv[2] === 'wasm') wasm(process.argv.slice(3))

function corpus(args) {
  const root = process.env.FIXTURE_ROOT
  const source = args[0] === 'fixture-disciplines' ? 'disciplines' : args.includes('lean-core') ? 'lean-core' : args[0].includes('literals') ? 'unicode' : 'openai'
  if (source !== 'disciplines') for (const suffix of ['gc', 'arc', 'wasm']) {
    if (!existsSync(join(root, 'target/proof/openai/check-batch.' + suffix))) process.exit(19)
  }
  if (['openai', 'lean-core'].includes(source) && !args.includes('--reuse-binaries')) process.exit(20)
  appendFileSync(join(root, 'calls'), JSON.stringify({ source }) + '\n')
  if (process.env.FIXTURE_FAIL_CORPUS === source) process.exit(21)
}

function benchmark() {
  const root = process.env.FIXTURE_ROOT
  const calls = readFileSync(join(root, 'calls'), 'utf8').trim().split('\n').map(JSON.parse)
  const compiler = process.env.OSPREY_BIN
  if (process.env.FIXTURE_ASSERT_COMPILER && (!compiler || !existsSync(compiler) || realpathSync(compiler) !== realpathSync(join(root, 'target/osprey')))) process.exit(24)
  for (const source of ['disciplines', 'openai', 'lean-core', 'unicode']) {
    if (!calls.some(call => call.source === source)) process.exit(22)
  }
  if (calls.length !== 15) process.exit(23)
  appendFileSync(join(root, 'calls'), JSON.stringify({ source: 'benchmark' }) + '\n')
}

if (process.argv[2] === 'corpus') corpus(process.argv.slice(3))
if (process.argv[2] === 'benchmark') benchmark()
