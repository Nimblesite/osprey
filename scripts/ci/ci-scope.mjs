import { appendFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const compilerRoots = /^(src\/toolchain\/crates|src\/toolchain\/compiler|src\/toolchain\/tree-sitter-osprey|src\/toolchain\/webcompiler|src\/toolchain\/vscode-extension|src\/osprey\/tests|src\/osprey\/examples|src\/osprey\/packages|scripts|\.github)\//
const content = (path) => /^(src\/toolchain\/website|docs\/book|src\/toolchain\/benchmarks)\//.test(path)
  || (path.endsWith('.md') && !compilerRoots.test(path))
const embeddedGrammar = 'src/toolchain/website/src/js/osprey-grammar.mjs'

export const classify = (files) => {
  if (!Array.isArray(files) || files.some((path) => typeof path !== 'string' || !path)) {
    throw new Error('Changed files must be an array of nonempty paths')
  }
  return {
    code: files.some((path) => path === embeddedGrammar || !content(path)),
    website: files.some((path) => path.startsWith('src/toolchain/website/')),
    book: files.some((path) => path.startsWith('docs/book/')),
    benchmarks: files.some((path) => path.startsWith('src/toolchain/benchmarks/')),
  }
}

if (resolve(process.argv[1] ?? '') === fileURLToPath(import.meta.url)) {
  const scope = classify(JSON.parse(process.env.CHANGED_FILES))
  const output = Object.entries(scope).map(([key, value]) => `${key}=${value}\n`).join('')
  appendFileSync(process.env.GITHUB_OUTPUT, output)
  process.stdout.write(output)
}
