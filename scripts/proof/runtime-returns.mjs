// Backward slice of pointer returns in Clang's LLVM for ownership list operations.
// Unknown pointer origins remain explicit: they invalidate a freshness theorem.
import { readFileSync } from 'node:fs'

export function runtimeReturns(file, allocator, boundary = '.allocated') {
  const ir = readFileSync(file, 'utf8')
  const functions = new Map([...ir.matchAll(/^define [^\n@]*@([\w.$-]+)\([^\n]*\{\n([\s\S]*?)^\}/gm)]
    .map(([, name, body]) => [name, body]))
  const summaries = new Map()
  function origin(name, callers = []) {
    if (name === allocator) {
      if (functions.has(name)) throw Error('expected external ownership allocator')
      return boundary
    }
    if (summaries.has(name)) return summaries.get(name)
    if (!functions.has(name) || callers.includes(name)) return '.other'
    const body = functions.get(name)
    const defs = new Map([...body.matchAll(/^\s*(%[\w.-]+) = (.+)$/gm)].map(([, reg, rhs]) => [reg, rhs]))
    function value(reg, seen = []) {
      if (reg === 'null') return '.nullResult'
      const rhs = defs.get(reg)
      if (!rhs || seen.includes(reg)) return '.other'
      const call = /^(?:(?:tail|musttail|notail) )?call [^@]*\b(?:ptr|i8\*) @([\w.$-]+)\(/.exec(rhs)
      if (call) return origin(call[1], [...callers, name])
      if (!rhs.startsWith('phi ptr ')) return '.other'
      const inputs = rhs.slice(8).split(/\s*,\s*(?=\[)/)
      return alternatives(inputs.map(input => {
        const pair = /^\[\s*([^, ]+)\s*,\s*%[\w.-]+\s*\]$/.exec(input)
        return pair ? value(pair[1], [...seen, reg]) : '.other'
      }))
    }
    const returns = [...body.matchAll(/^\s*ret (.+)$/gm)].map(([, ret]) =>
      value(/^(?:ptr|i8\*) (%[\w.$-]+|null)$/.exec(ret)?.[1]))
    const summary = alternatives(returns)
    summaries.set(name, summary)
    return summary
  }
  return name => {
    if (!functions.has(name)) throw Error('missing compiled runtime function: ' + name)
    return origin(name)
  }
}
function alternatives(values) {
  if (!values.length) return '.other'
  return values.reduce((left, right) => '(.fork ' + left + ' ' + right + ')')
}
