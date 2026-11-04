import { appendFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const compilerRoots = /^(src\/crates|src\/compiler|src\/tree-sitter-osprey|src\/webcompiler|src\/vscode-extension|tests|src\/examples|src\/packages|scripts|\.github)\//
const content = (path) => /^(src\/website|Book|src\/benchmarks)\//.test(path)
  || (path.endsWith('.md') && !compilerRoots.test(path))
const embeddedGrammar = 'src/website/src/js/osprey-grammar.mjs'

export const classify = (files) => {
  if (!Array.isArray(files) || files.some((path) => typeof path !== 'string' || !path)) {
    throw new Error('Changed files must be an array of nonempty paths')
  }
  return {
    code: files.some((path) => path === embeddedGrammar || !content(path)),
    website: files.some((path) => path.startsWith('src/website/')),
    book: files.some((path) => path.startsWith('Book/')),
    benchmarks: files.some((path) => path.startsWith('src/benchmarks/')),
  }
}

if (resolve(process.argv[1] ?? '') === fileURLToPath(import.meta.url)) {
  const scope = classify(JSON.parse(process.env.CHANGED_FILES))
  const output = Object.entries(scope).map(([key, value]) => `${key}=${value}\n`).join('')
  appendFileSync(process.env.GITHUB_OUTPUT, output)
  process.stdout.write(output)
}
