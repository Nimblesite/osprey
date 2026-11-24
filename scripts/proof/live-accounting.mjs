// [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Interpret the two native
// counter cells as 64-bit words. Reject unknown instructions and control flow.
import { operand, runtimeCfg } from './runtime-returns.mjs'

function trackedPath(cfg, label, size) {
  const cells = { g_live: 'objects', g_live_bytes: 'bytes' }
  const values = new Map([[size, 'size']])
  const seen = new Set()
  const read = reg => operand(values, reg)
  while (!seen.has(label) && cfg.has(label)) {
    seen.add(label)
    const block = cfg.get(label)
    for (const line of block.slice(0, -1)) {
      const load = /^(%[\w.]+) = load i64, ptr @(g_live(?:_bytes)?), align 8$/.exec(line)
      const store = /^store i64 (%[\w.]+), ptr @(g_live(?:_bytes)?), align 8$/.exec(line)
      const dec = /^(%[\w.]+) = add i64 (%[\w.]+), -(\d+)$/.exec(line)
      const sub = /^(%[\w.]+) = sub i64 (%[\w.]+), (%[\w.]+)$/.exec(line)
      if (load) values.set(load[1], cells[load[2]])
      else if (store) cells[store[2]] = read(store[1])
      else if (dec) values.set(dec[1], '(' + read(dec[2]) + ' - ' + dec[3] + ')')
      else if (sub) values.set(sub[1], '(' + read(sub[2]) + ' - ' + read(sub[3]) + ')')
      else throw Error('unsupported accounting instruction: ' + line)
    }
    if (block.at(-1) === 'ret void') return '(' + cells.g_live + ', ' + cells.g_live_bytes + ')'
    label = /^br label %([\w.]+)$/.exec(block.at(-1))?.[1]
  }
  throw Error('unsupported accounting path')
}

export function liveDebit(ir) {
  const { arg: size, cfg, entry } = runtimeCfg(ir, 'arc_live_dec', 'i64')
  const guard = /^(%[\w.]+) = load i1, ptr @g_arc_track, align 4$/.exec(entry[0])
  const br = /^br i1 (%[\w.]+), label %([\w.]+), label %([\w.]+)$/.exec(entry[1])
  if (entry.length !== 2 || !guard || !br || guard[1] !== br[1]) throw Error('unsupported tracking switch')
  return '  if tracking then ' + trackedPath(cfg, br[2], size) + ' else ' + trackedPath(cfg, br[3], size)
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
