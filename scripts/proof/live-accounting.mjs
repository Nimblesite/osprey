// [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Interpret the native live counters and peak bytes
// as 64-bit words. Reject unknown instructions and control flow.
import { operand, runtimeCfg } from './runtime-returns.mjs'

const conditional = /^br i1 (%[\w.]+), label %([\w.]+), label %([\w.]+)$/

function accountInstruction(line, cells, values) {
  const load = /^(%[\w.]+) = load i64, ptr @(g_live|g_live_bytes|g_peak_bytes), align 8$/.exec(line)
  const store = /^store i64 (%[\w.]+), ptr @(g_live|g_live_bytes|g_peak_bytes), align 8$/.exec(line)
  const op = /^(%[\w.]+) = (add|sub|icmp ugt) i64 (%[\w.]+), (-?\d+|%[\w.]+)$/.exec(line)
  if (load) values.set(load[1], operand(cells, load[2]))
  else if (store && cells.has(store[2])) cells.set(store[2], operand(values, store[1]))
  else if (op) {
    const right = op[4], negative = right.startsWith('-')
    const operator = op[2] === 'icmp ugt' ? ' > ' : op[2] === 'add' && !negative ? ' + ' : ' - '
    if (negative && op[2] !== 'add') throw Error('unsupported signed counter operand')
    const value = right.startsWith('%') ? operand(values, right) : right.replace(/^-/, '')
    values.set(op[1], '(' + operand(values, op[3]) + operator + value + ')')
  } else throw Error('unsupported accounting instruction: ' + line)
}

function trackedPath(cfg, label, cells, values, seen = []) {
  if (!cfg.has(label) || seen.includes(label)) throw Error('unsupported accounting path')
  const block = cfg.get(label)
  for (const line of block.slice(0, -1)) accountInstruction(line, cells, values)
  const end = block.at(-1)
  if (end === 'ret void') return '(' + [...cells.values()].join(', ') + ')'
  const visit = to => trackedPath(cfg, to, new Map(cells), new Map(values), [...seen, label])
  const br = conditional.exec(end)
  if (br) return '(if ' + operand(values, br[1]) + ' then ' + visit(br[2]) + ' else ' + visit(br[3]) + ')'
  return visit(/^br label %([\w.]+)$/.exec(end)?.[1])
}

export function liveAccounting(ir, name) {
  const { arg: size, cfg, entry } = runtimeCfg(ir, name, 'i64')
  const guard = /^(%[\w.]+) = load i1, ptr @g_arc_track, align 4$/.exec(entry[0])
  const br = conditional.exec(entry[1])
  if (entry.length !== 2 || !guard || !br || guard[1] !== br[1]) throw Error('unsupported tracking switch')
  const cells = new Map([['g_live', 'objects'], ['g_live_bytes', 'bytes']])
  if (name === 'arc_live_inc') cells.set('g_peak_bytes', 'peak')
  const path = label => trackedPath(cfg, label, new Map(cells), new Map([[size, 'size']]))
  return '  if tracking then ' + path(br[2]) + ' else ' + path(br[3])
}

// Reflect the unconditional prefix, up to and including the header state store.
// The header parameter denotes the pointer actually returned by arc_hdr(body).
export function reclaimPrefix(ir) {
  const { arg: body, entry } = runtimeCfg(ir, 'arc_free_or_recycle')
  const hdr = /^(%[\w.]+) = (?:tail )?call (?:fastcc )?ptr @arc_hdr\(ptr (?:noundef )?(%[\w.]+)\)$/.exec(entry[0])
  const field = /^(%[\w.]+) = getelementptr inbounds (?:nuw )?i8, ptr (%[\w.]+), i64 (\d+)$/
  const size = field.exec(entry[1]), state = field.exec(entry[5])
  const load = /^(%[\w.]+) = load i32, ptr (%[\w.]+), align \d+$/.exec(entry[2])
  const widen = /^(%[\w.]+) = zext i32 (%[\w.]+) to i64$/.exec(entry[3])
  const call = /^(?:tail )?call (?:fastcc )?void @arc_live_dec\(i64 (?:noundef )?(%[\w.]+)\)$/.exec(entry[4])
  const store = /^store i32 (\d+), ptr (%[\w.]+), align \d+$/.exec(entry[6])
  if (!hdr || !size || !state || !load || !widen || !call || !store || hdr[2] !== body ||
      size[2] !== hdr[1] || state[2] !== hdr[1] || load[2] !== size[1] ||
      widen[2] !== load[1] || call[1] !== widen[1] || store[2] !== state[1])
    throw Error('unsupported reclamation prefix')
  return '  (emittedDebit tracking objects bytes ((header ' + size[3] + ').zeroExtend 64), ' + state[3] + ', ' + store[1] + ')'
}
