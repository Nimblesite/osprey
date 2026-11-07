import { appendFileSync, readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const compilerRoots = /^(src\/toolchain\/crates|src\/toolchain\/compiler|src\/toolchain\/tree-sitter-osprey|src\/toolchain\/webcompiler|src\/toolchain\/vscode-extension|src\/osprey\/tests|src\/osprey\/examples|src\/osprey\/packages|scripts|\.github)\//
const content = (path) => /^(src\/toolchain\/website|docs\/book|src\/toolchain\/benchmarks)\//.test(path)
  || (path.endsWith('.md') && !compilerRoots.test(path))
const embeddedGrammar = 'src/toolchain/website/src/js/osprey-grammar.mjs'
const program = (path) => /\.(osp|ospml|ospo|py)$/.test(path) || path.split('/').pop() === 'osprey.toml'

export const classify = (files) => {
  if (!Array.isArray(files) || files.some((path) => typeof path !== 'string' || !path)) {
    throw new Error('Changed files must be an array of nonempty paths')
  }
  return {
    code: files.some((path) => path === embeddedGrammar || !content(path)),
    website: files.some((path) => path.startsWith('src/toolchain/website/')),
    book: files.some((path) => path.startsWith('docs/book/')),
    benchmarks: files.some((path) => path.startsWith('src/toolchain/benchmarks/')),
    programs: files.some(program),
  }
}

if (resolve(process.argv[1] ?? '') === fileURLToPath(import.meta.url)) {
  const all = process.argv[2] === '--all'
  const input = all ? '' : readFileSync(0, 'utf8')
  if (input && !input.endsWith('\0')) throw new Error('Changed paths must be NUL-terminated')
  const scope = classify(input.split('\0').slice(0, -1))
  const output = Object.entries(scope).map(([key, value]) => `${key}=${all || value}\n`).join('')
  appendFileSync(process.env.GITHUB_OUTPUT, output)
  process.stdout.write(output)
}
