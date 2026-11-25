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

// `type` is every parameter's type, or a list of one type per parameter.
export function voidBody(ir, name, arity = 1, type = 'ptr') {
  const attributes = '(?:(?:range\\(i64 0, 4294967296\\)|noundef|nofree|readonly|readnone|nocapture|captures\\((?:none|address_is_null)\\)) )*'
  const types = Array.isArray(type) ? type : Array(arity).fill(type)
  const params = types.map(each => each + ' ' + attributes + '(%[\\w.-]+)').join(', ')
  const header = '^define [^@\\n]*void @' + name + '\\(' + params + '\\)[^\\n]*\\{\\n([\\s\\S]*?)^\\}'
  const match = new RegExp(header, 'm').exec(ir)
  if (!match) throw Error('unsupported signature: ' + name)
  const lines = match.at(-1).replace(/;[^\n]*/g, '').split('\n').map(line => line.trim()).filter(Boolean)
  return { args: match.slice(1, -1), lines }
}

export const wordSlot = /^(%[\w.]+) = getelementptr inbounds (?:nuw )?(?:\[(\d+) x i8\]|i64), ptr (%[\w.]+), i64 (%[\w.]+)$/

export function blocks(lines) {
  const result = new Map()
  let block
  for (const line of lines) {
    if (/^[\w.-]+:$/.test(line)) {
      const name = line.slice(0, -1)
      if (result.has(name)) throw Error('duplicate LLVM block: ' + name)
      block = []
      result.set(name, block)
    } else block.push(line)
  }
  return result
}

export function pointerCfg(ir, name) {
  const fn = new RegExp('^define [^\\n@]*ptr @' + name + '\\((.*)\\)[^\\n{]*\\{\\n([\\s\\S]*?)^\\}', 'm').exec(ir)
  if (!fn) throw Error('missing compiled function: ' + name)
  const args = [...fn[1].matchAll(/%[\w.]+/g)].map(m => m[0])
  const lines = fn[2].replace(/;[^\n]*|, !tbaa !\d+| #\d+(?=\n)/g, '').split('\n').map(s => s.trim()).filter(Boolean)
  return { args, lines, cfg: blocks(lines) }
}

export function runtimeCfg(ir, name, type = 'ptr') {
  const { args: [arg], lines } = voidBody(ir, name, 1, type)
  const cfg = blocks(lines.map(line => line.replace(/, !(?:tbaa|llvm\.loop) !\d+/g, '')))
  return { arg, cfg, entry: cfg.get('entry') ?? [] }
}

export function operand(regs, reg) {
  if (!regs.has(reg)) throw Error('unknown LLVM operand: ' + reg)
  return regs.get(reg)
}

export function selectedPhi(rhs, regs, predecessor) {
  const m = /^phi (?:i8\*|i64) \[ (%[\w$]+), %(L\d+) \], \[ (%[\w$]+), %(L\d+) \]$/.exec(rhs)
  if (!m || m[2] === m[4]) throw Error('invalid ownership phi: ' + rhs)
  return operand(regs, operand(new Map([[m[2], m[1]], [m[4], m[3]]]), predecessor))
}

// Consume every instruction on the selected, acyclic path, including its
// actual incoming edge at each join. Callers supply path-specific semantics.
export function tracePointer(cfg, instruction, bit, returned) {
  const seen = new Set()
  let label = 'entry', previous
  while (!seen.has(label)) {
    seen.add(label)
    const lines = operand(cfg, label)
    for (const line of lines.slice(0, -1)) instruction(line, previous)
    const end = lines.at(-1)
    const ret = /^ret i8\* (null|%[\w$]+)$/.exec(end)
    if (ret) return returned(ret[1])
    previous = label
    const br = /^br i1 (%r\d+), label %(L\d+), label %(L\d+)$/.exec(end)
    label = br ? br[bit(br[1]) ? 2 : 3] : /^br label %(L\d+)$/.exec(end)?.[1]
  }
  throw Error('cycle in compiled ownership path')
}

export function jumpTarget(line) {
  const jump = /^br label %([\w.]+)$/.exec(line)
  if (!jump) throw Error('unsupported LLVM jump: ' + line)
  return jump[1]
}

export function zeroSuccessors(entry = [], value, type = 'ptr') {
  const cmp = /^(%[\w.]+) = icmp (eq|ne) (ptr|i32) (%[\w.]+), (null|0)$/.exec(entry[0])
  const br = /^br i1 (%[\w.]+), label %([\w.]+), label %([\w.]+)$/.exec(entry[1])
  if (entry.length !== 2 || !cmp || !br || cmp[3] !== type || cmp[4] !== value || cmp[5] !== (type === 'ptr' ? 'null' : '0') ||
      br[1] !== cmp[1]) throw Error('unsupported LLVM zero guard')
  return cmp[2] === 'eq' ? br.slice(2) : br.slice(2).reverse()
}

const returnsImmediately = block => block.length === 1 && block[0] === 'ret void'

export function compatibilityReturns(ir) {
  const inert = (name, arity, type) => {
    const { lines } = voidBody(ir, name, arity, type)
    return returnsImmediately(operand(blocks(lines), 'entry'))
  }
  return '  (' + [inert('osp_release_unique', 1), inert('osp_collect', 0), inert('osp_mem_set_finalizer', 2, ['i64', 'ptr'])].join(', ') + ')'
}

// Reflect the compiled free entry and its null test. Only an immediate void
// return is inert. Every other successor is conservatively effectful.
export function nullDrop(file) {
  const ir = readFileSync(file, 'utf8')
  const wrapper = voidBody(ir, 'osp_ownership_free')
  const call = /^(?:tail )?call (?:fastcc )?void @tracked_drop\(ptr (?:noundef )?(%[\w.-]+)\)$/.exec(wrapper.lines[1])
  if (wrapper.lines.length !== 3 || wrapper.lines[0] !== 'entry:' || wrapper.lines[2] !== 'ret void' || call?.[1] !== wrapper.args[0])
    throw Error('unsupported ownership free entry')
  const { args: [arg], lines } = voidBody(ir, 'tracked_drop')
  const cfg = blocks(lines)
  const [absent, present] = zeroSuccessors(cfg.get('entry'), arg)
  const inert = label => {
    const block = cfg.get(label)
    if (!block) throw Error('missing drop successor: ' + label)
    return returnsImmediately(block)
  }
  return 'if isNull then ' + inert(absent) + ' else ' + inert(present)
}
