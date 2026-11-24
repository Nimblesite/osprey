// [MEM-OWNERSHIP-STATIC-PATHS] Follow the Error arm of the emitted Result
// match. A null message selects the static fallback. Stack slots may alias the
// parent or message, so reads use the last stored pointer on this path.
import { blocks, operand, pointerPhi, tracePointer } from './runtime-returns.mjs'

const result = '{ i64, i8, i8* }'
const parent = 'parent', nil = 'nil', fallback = 'fallback'
const valueOf = (regs, v) => v === 'null' ? nil : operand(regs, v)

function assign(rhs, st) {
  const { regs, slots, predecessor } = st
  let m
  if (rhs === 'call ' + result + '* @find(i64 %$p0)') { st.events.push('.acquire'); return parent }
  if ((m = /^bitcast \{ i64, i8, i8\* \}\* (%r\d+) to i8\*$/.exec(rhs))) return operand(regs, m[1])
  if ((m = /^getelementptr \{ i64, i8, i8\* \}, \{ i64, i8, i8\* \}\* (%r\d+), i32 0, i32 ([012])$/.exec(rhs)) &&
      operand(regs, m[1]) === parent) return 'field' + m[2]
  if ((m = /^load (i8\*|i8|i64), (?:i8\*\*|i8\*|i64\*) (%[\w.]+)$/.exec(rhs))) {
    if (slots.has(m[2])) return operand(slots, m[2])
    const field = operand(regs, m[2])
    st.events.push('.read ' + field.slice(5))
    if (field === 'field2') return st.message
    if (field === 'field0' || field === 'field1') return field === 'field1' ? 'errorTag' : 'payload'
  }
  if ((m = /^icmp eq i8 (%r\d+), 0$/.exec(rhs)) && operand(regs, m[1]) === 'errorTag') return false
  if ((m = /^icmp eq i8\* (%r\d+), null$/.exec(rhs))) return operand(regs, m[1]) === nil
  if ((m = /^getelementptr \[6 x i8\], \[6 x i8\]\* (@[.\w]+), i64 0, i64 0$/.exec(rhs)) && st.literal.has(m[1])) return fallback
  if ((m = /^select i1 (%r\d+), i8\* (%r\d+), i8\* (%r\d+)$/.exec(rhs))) return operand(regs, operand(regs, m[1]) ? m[2] : m[3])
  if (rhs.startsWith('phi ')) return pointerPhi(rhs, regs, predecessor)
  throw Error('unsupported error-arm value: ' + rhs)
}

function effect(line, st) {
  let m
  if ((m = /^(%arc\.s\d+) = alloca i8\*$/.exec(line))) st.slots.set(m[1], 'uninitialized')
  else if ((m = /^(%r\d+) = (.+)$/.exec(line))) st.regs.set(m[1], assign(m[2], st))
  else if ((m = /^store i8\* (null|%r\d+), i8\*\* (%[\w.]+)$/.exec(line))) {
    const value = valueOf(st.regs, m[1])
    if (st.slots.has(m[2])) st.slots.set(m[2], value)
    else if (operand(st.regs, m[2]) === 'field2') { st.message = value; st.events.push('.write .' + value) }
    else throw Error('unsupported error-arm store')
  } else if ((m = /^call void @osp_ownership_free\(i8\* (null|%r\d+)\)$/.exec(line))) st.events.push('.free .' + valueOf(st.regs, m[1]))
  else throw Error('unsupported error-arm operation: ' + line)
}

function errorArm(cfg, literal, message) {
  const st = { literal, message, slots: new Map(), regs: new Map(), events: [], predecessor: '' }
  return tracePointer(cfg, (line, prev) => { st.predecessor = prev; effect(line, st) },
    reg => operand(st.regs, reg), reg => '[' + [...st.events, '.ret .' + valueOf(st.regs, reg)].join(', ') + ']')
}
export function resultTake(ir, source) {
  const match = ['fn missing(n) = match find(n) {', '    Success { value } => "found${toString(value)}"',
    '    Error { message } => message', '}'].join('\n')
  if (!source.includes(match)) throw Error('unsupported source Result match')
  const fn = /^define internal i8\* @missing\(i64 %\$p0\) #\d+ \{\n([\s\S]*?)^\}/m.exec(ir)
  const literal = new Set([...ir.matchAll(/^(@[.\w]+) = private unnamed_addr constant \[6 x i8\] c"Error\\00"$/gm)].map(match => match[1]))
  if (!fn || !literal.size) throw Error('missing emitted Result match or static Error fallback')
  const cfg = blocks(fn[1].trim().split('\n').map(line => line.trim()))
  return '  if present then ' + errorArm(cfg, literal, 'child') + ' else ' + errorArm(cfg, literal, nil)
}
