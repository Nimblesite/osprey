// Translate the complete reachable LLVM paths for the owned scalar list regressions.
// The Boolean parameter has two inhabitants; specialize both, resolving each phi by
// its actual predecessor. Reject unknown instructions [MEM-OWNERSHIP-STATIC-VERIFICATION].
import { readFileSync } from 'node:fs'
import { liveDebit, reclaimPrefix } from './live-accounting.mjs'
import { bodyPaths, popBody } from './worklist-drain.mjs'
import { childDrop, growthBytes, worklistPush } from './child-drop.mjs'
import { pairConstructor, pairWalk } from './record-ownership.mjs'
import { blocks as llvmBlocks, nullDrop, runtimeReturns } from './runtime-returns.mjs'
const [source, irFile, template, listFile, mapFile, stringFile, heapFile, walkFile] = process.argv.slice(2)
const src = readFileSync(source, 'utf8').split('\n')
const ir = readFileSync(irFile, 'utf8')
function define(map, key, value = map.size) {
  if (map.has(key)) throw Error('duplicate definition ' + key)
  map.set(key, value)
  return value
}
function get(map, key) {
  if (!map.has(key)) throw Error('undefined operand ' + key)
  return map.get(key)
}
function blocks(name, conditional) {
  const sig = 'i8\\* %\\$p0, i64 %\\$p1' + (conditional ? ', i1 %\\$p2' : '')
  const pattern = new RegExp('^define internal i8\\* @' + name + '\\$mono\\d+\\(' + sig + '\\) #\\d+ \\{\\n([^}]+)\\n\\}$', 'gm')
  const bodies = [...ir.matchAll(pattern)]
  if (bodies.length !== 1) throw Error('expected one specialization of ' + name)
  return llvmBlocks(bodies[0][1].split('\n').map(line => line.trim()))
}
const option = value => value === null ? 'none' : '(some ' + value + ')'
function instruction({ regs, slots, contents, bits, code }, line, previous) {
  let m
  if ((m = /^(%arc\.s\d+) = alloca i8\*$/.exec(line))) code.push('.reserve ' + define(slots, m[1]))
  else if ((m = /^store i8\* (null|%[\w$]+), i8\*\* (%arc\.s\d+)$/.exec(line))) {
    const v = m[1] === 'null' ? null : get(regs, m[1])
    contents.set(m[2], v)
    code.push('.store ' + get(slots, m[2]) + ' ' + option(v))
  } else if ((m = /^(%r\d+) = load i8\*, i8\*\* (%arc\.s\d+)$/.exec(line))) {
    const loaded = get(contents, m[2])
    define(regs, m[1], loaded)
    code.push('.load ' + get(slots, m[2]) + ' ' + option(loaded))
  } else if ((m = /^(%r\d+) = call i8\* @osprey_list_append_of\(i8\* (%[\w$]+), i64 %\$p1, i32 0\)$/.exec(line))) {
    code.push('.append ' + get(regs, m[2]) + ' ' + define(regs, m[1]))
  } else if ((m = /^call void @osp_ownership_free\(i8\* (%[\w$]+)\)$/.exec(line))) {
    const reg = get(regs, m[1])
    code.push('.free ' + option(reg))
  }
  else if ((m = /^(%r\d+) = zext i1 (0|1|%\$p2) to i64$/.exec(line))) define(bits, m[1], get(bits, m[2]))
  else if ((m = /^(%r\d+) = icmp eq i64 (%r\d+), (%r\d+)$/.exec(line))) define(bits, m[1], get(bits, m[2]) === get(bits, m[3]))
  else if ((m = /^(%r\d+) = phi i8\* \[ (%[\w$]+), %(L\d+) \], \[ (%[\w$]+), %(L\d+) \]$/.exec(line))) {
    const incoming = new Map()
    define(incoming, m[3], m[2])
    define(incoming, m[5], m[4])
    define(regs, m[1], get(regs, get(incoming, previous)))
  } else throw Error('unsupported LLVM instruction: ' + line)
}
function trace(body, enabled) {
  const regs = new Map([['%$p0', 0]])
  const slots = new Map()
  const bits = new Map([['0', false], ['1', true], ['%$p2', enabled]])
  const code = []
  const state = { regs, slots, contents: new Map(), bits, code }
  let label = 'entry', previous
  const visited = new Set()
  while (!visited.has(label)) {
    visited.add(label)
    const lines = get(body, label)
    for (const line of lines.slice(0, -1)) instruction(state, line, previous)
    const end = lines.at(-1)
    previous = label
    let m
    if ((m = /^ret i8\* (%[\w$]+)$/.exec(end))) return '[' + [...code, '.ret ' + get(regs, m[1])].join(', ') + ']'
    if ((m = /^br label %(L\d+)$/.exec(end))) label = m[1]
    else if ((m = /^br i1 (%r\d+), label %(L\d+), label %(L\d+)$/.exec(end))) label = get(bits, m[1]) ? m[2] : m[3]
    else throw Error('unsupported reachable terminator: ' + end)
  }
  throw Error('loop in scalar append specialization')
}
for (const definition of ['type Pair = { left: Box, right: Box }', 'fn pairUp(left, right) = Pair { left: left, right: right }', 'fn grow(xs, x) = listAppend(xs, x)', 'fn growWhen(xs, x, enabled) = if enabled { listAppend(xs, x) } else { xs }']) {
  if (!src.includes(definition)) throw Error('unsupported source: ' + definition)
}
const conditional = blocks('growWhen', true)
const list = runtimeReturns(listFile, 'osp_alloc_tagged_noinit')
const map = runtimeReturns(mapFile, 'osp_arc_calloc')
const strings = runtimeReturns(stringFile, 'osp_alloc_tagged')
const renderer = runtimeReturns(irFile, 'osp_string_show', strings('osp_string_show'))
const heap = readFileSync(heapFile, 'utf8')
const walk = readFileSync(walkFile, 'utf8')
const stringOps = 'concat take drop substring to_upper to_lower trim_start trim_end trim reverse replace repeat pad_start pad_end from_codepoint'.split(' ')
process.stdout.write(readFileSync(template, 'utf8')
  .replace('-- GROWTH BYTES', growthBytes(walk))
  .replace('-- WORKLIST PUSH', worklistPush(heap))
  .replace('-- POP BODY', popBody(heap))
  .replace('-- RECLAIM PREFIX', reclaimPrefix(heap))
  .replace('-- LIVE DEBIT', liveDebit(heap))
  .replace('-- BODY PATHS', bodyPaths(heap))
  .replace('-- CHILD DROP', childDrop(heap))
  .replace('-- PAIR WALK', pairWalk(walk))
  .replace('-- PAIR CONSTRUCTOR', pairConstructor(ir))
  .replace('-- NULL DROP', nullDrop(heapFile))
  .replace('-- STRING OP RETURNS', stringOps.map(op => '  | .«' + op + '» => ' + strings('osp_string_' + op)).join('\n'))
  .replace('-- APPEND RETURNS', list('osprey_list_append_of'))
  .replace('-- PREPEND RETURNS', list('osprey_list_prepend_of'))
  .replace('-- MAP SET RETURNS', map('osprey_map_set_of'))
  .replace('-- STRING RENDER RETURNS', renderer('osp.any.str.string'))
  .replace('-- EMITTED BODY', trace(blocks('grow', false), false))
  .replace('-- ENABLED BODY', trace(conditional, true))
  .replace('-- DISABLED BODY', trace(conditional, false)))
