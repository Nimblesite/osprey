// [MEM-OWNERSHIP-STATIC-PATHS] Follow the Error arm of the emitted Result
// match. A null message selects the static fallback. Stack slots may alias the
// parent or message, so reads use the last stored pointer on this path.
import { blocks, operand, selectedPhi, tracePointer } from './runtime-returns.mjs'

const result = '{ i64, i8, i8* }'
const parent = 'parent', nil = 'nil', fallback = 'fallback'
const term = value => '(.' + value + ')'
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
  if ((m = /^icmp eq i8 (%r\d+), 0$/.exec(rhs)) && operand(regs, m[1]) === 'errorTag') return st.success
  if ((m = /^icmp eq i8\* (%r\d+), null$/.exec(rhs))) return operand(regs, m[1]) === nil
  if ((m = /^getelementptr \[(\d+) x i8\], \[\d+ x i8\]\* (@[.\w]+), i64 0, i64 0$/.exec(rhs))) return operand(st.literal, m[2] + '/' + m[1])
  if ((m = /^select i1 (%r\d+), i8\* (%r\d+), i8\* (%r\d+)$/.exec(rhs))) return operand(regs, operand(regs, m[1]) ? m[2] : m[3])
  if (rhs.startsWith('phi ')) return selectedPhi(rhs, regs, predecessor)
  return formattedValue(rhs, st)
}

// Model the pointer arguments actually passed to the known non-owning formatting
// helpers. Their literal format determines the number and type of the reads.
function pointerInputs(st, format, args) {
  const values = args.split(', ').map(arg => operand(st.regs, arg.replace(/^i8\* /, '')))
  const fmt = operand(st.regs, format)
  if (!(fmt === 'single' && values.length === 1 || fmt === 'pair' && values.length === 2))
    throw Error('unsupported Result render format')
  return '[' + values.map(v => '.' + v).join(', ') + ']'
}

function formattedValue(rhs, st) {
  let m
  if ((m = /^call i64 @osp_strlen\(i8\* (%r\d+)\)$/.exec(rhs))) {
    st.events.push('.length .' + operand(st.regs, m[1]))
    return 'length'
  }
  if ((m = /^call i8\* @osp_alloc\(i64 (21|%r\d+)\)$/.exec(rhs))) {
    if (m[1] !== '21' && operand(st.regs, m[1]) !== 'capacity') throw Error('unsupported render allocation')
    const root = 'buffer ' + st.allocated++
    st.events.push('.alloc (.' + root + ')')
    return root
  }
  if ((m = /^call i32 \(i8\*, i8\*, \.\.\.\) @sprintf\(i8\* (%r\d+), i8\* (%r\d+), i64 (%r\d+)\)$/.exec(rhs)) &&
      operand(st.regs, m[2]) === 'integer' && ['length', 'payload'].includes(operand(st.regs, m[3]))) {
    st.events.push('.number (.' + operand(st.regs, m[1]) + ')')
    return 'written'
  }
  if ((m = /^call i64 \(i8\*, \.\.\.\) @osp_format_size\(i8\* (%r\d+), (.+)\)$/.exec(rhs))) {
    st.events.push('.measure ' + pointerInputs(st, m[1], m[2]))
    return 'required'
  }
  if ((m = /^add i64 (%r\d+), 1$/.exec(rhs)) && operand(st.regs, m[1]) === 'required') return 'capacity'
  throw Error('unsupported Result operation: ' + rhs)
}

function effect(line, st) {
  let m
  if ((m = /^(%arc\.s\d+) = alloca i8\*$/.exec(line))) st.slots.set(m[1], 'uninitialized')
  else if ((m = /^(%r\d+) = (.+)$/.exec(line))) st.regs.set(m[1], assign(m[2], st))
  else if ((m = /^store i8\* (null|%r\d+), i8\*\* (%[\w.]+)$/.exec(line))) {
    const value = valueOf(st.regs, m[1])
    if (st.slots.has(m[2])) st.slots.set(m[2], value)
    else if (operand(st.regs, m[2]) === 'field2') { st.message = value; st.events.push('.write ' + term(value)) }
    else throw Error('unsupported error-arm store')
  } else if ((m = /^call void @osp_ownership_free\(i8\* (null|%r\d+)\)$/.exec(line))) st.events.push('.free ' + term(valueOf(st.regs, m[1])))
  else if ((m = /^call void \(i8\*, i64, i8\*, \.\.\.\) @osp_format_into\(i8\* (%r\d+), i64 (%r\d+), i8\* (%r\d+), (.+)\)$/.exec(line)) &&
      operand(st.regs, m[2]) === 'capacity')
    st.events.push('.render (.' + operand(st.regs, m[1]) + ') ' + pointerInputs(st, m[3], m[4]))
  else throw Error('unsupported error-arm operation: ' + line)
}

function resultArm(cfg, literal, message, success) {
  const st = { allocated: 0, success, literal, message, slots: new Map(), regs: new Map(), events: [], predecessor: '' }
  return tracePointer(cfg, (line, prev) => { st.predecessor = prev; effect(line, st) },
    reg => operand(st.regs, reg), reg => '[' + [...st.events, '.ret ' + term(valueOf(st.regs, reg))].join(', ') + ']')
}
export function resultMatch(ir, source, name) {
  const match = ['fn missing(n) = match find(n) {', '    Success { value } => "found${toString(value)}"',
    '    Error { message } => message', '}'].join('\n')
  const kept = ['fn missingKept(n) = {', '    let outcome = find(n)', '    let size = match outcome {',
    '        Success { value } => value', '        Error { message } => length(message)', '    }',
    '    "${toString(size)}/${found(outcome)}"', '}'].join('\n')
  if (!source.includes(name === 'missing' ? match : kept)) throw Error('unsupported source Result match')
  const fn = new RegExp('^define internal i8\\* @' + name + '\\(i64 %\\$p0\\) #\\d+ \\{\\n([\\s\\S]*?)^\\}', 'm').exec(ir)
  const formats = new Map([['Error', fallback], ['%lld', 'integer'], ['found %s', 'single'], ['missing: %s', 'single'], ['%s/%s', 'pair']])
  const globals = [...ir.matchAll(/^(@[.\w]+) = private unnamed_addr constant \[(\d+) x i8\] c"([^"\\]*)\\00"$/gm)]
  const literal = new Map(globals.filter(m => formats.has(m[3])).map(m => [m[1] + '/' + m[2], formats.get(m[3])]))
  if (!fn || !literal.size) throw Error('missing emitted Result match or literals')
  const cfg = blocks(fn[1].trim().split('\n').map(line => line.trim()))
  const run = (ptr, success) => resultArm(cfg, literal, ptr, success)
  const paths = success => {
    const absent = run(nil, success), present = run('child', success)
    return present === absent ? present : '(if present then ' + present + ' else ' + absent + ')'
  }
  return '  ' + (name === 'missing' ? paths(false) : 'if success then ' + paths(true) + ' else ' + paths(false))
}
