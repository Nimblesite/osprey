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

function unary(ir, name) {
  const attributes = '(?:(?:noundef|nofree|readnone|nocapture|captures\\(address_is_null\\)) )*'
  const header = '^define [^@\\n]*void @' + name + '\\(ptr ' + attributes + '(%[\\w.-]+)\\)[^\\n]*\\{\\n([\\s\\S]*?)^\\}'
  const match = new RegExp(header, 'm').exec(ir)
  if (!match) throw Error('missing unary free function: ' + name)
  return { arg: match[1], lines: match[2].replace(/;[^\n]*/g, '').split('\n').map(line => line.trim()).filter(Boolean) }
}

// Reflect the compiled free entry and its null test. Only an immediate void
// return is inert. Every other successor is conservatively effectful.
export function nullDrop(file) {
  const ir = readFileSync(file, 'utf8')
  const wrapper = unary(ir, 'osp_ownership_free')
  const call = /^(?:tail )?call (?:fastcc )?void @tracked_drop\(ptr (?:noundef )?(%[\w.-]+)\)$/.exec(wrapper.lines[1])
  if (wrapper.lines.length !== 3 || wrapper.lines[0] !== 'entry:' || wrapper.lines[2] !== 'ret void' || call?.[1] !== wrapper.arg)
    throw Error('unsupported ownership free entry')
  const { arg, lines } = unary(ir, 'tracked_drop')
  const test = /^(%[\w.-]+) = icmp (eq|ne) ptr (%[\w.-]+), null$/.exec(lines[1])
  const branch = /^br i1 (%[\w.-]+), label %([\w.-]+), label %([\w.-]+)$/.exec(lines[2])
  if (lines[0] !== 'entry:' || !test || !branch || test[3] !== arg || branch[1] !== test[1])
    throw Error('unsupported tracked drop guard')
  const inert = label => {
    const at = lines.indexOf(label + ':')
    if (at < 0) throw Error('missing drop successor ' + label)
    return lines[at + 1] === 'ret void'
  }
  return 'if (isNull == ' + (test[2] === 'eq') + ') then ' + inert(branch[2]) + ' else ' + inert(branch[3])
}
