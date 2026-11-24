// [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] The native loop reloads the
// worklist depth after each child walk, which can append more bodies to visit.
import { blocks, runtimeCfg, voidBody, wordSlot } from './runtime-returns.mjs'

function loopGuard(lines) {
  const load = /^(%[\w.]+) = load i64, ptr @g_wl_top, align 8$/.exec(lines[0])
  const cmp = /^(%[\w.]+) = icmp (eq|ne) i64 (%[\w.]+), 0$/.exec(lines[1])
  const br = /^br i1 (%[\w.]+), label %([\w.]+), label %([\w.]+)$/.exec(lines[2])
  if (!load || !cmp || !br || load[1] !== cmp[3] || cmp[1] !== br[1]) throw Error('unsupported worklist loop guard')
  return { depth: load[1], exit: br[cmp[2] === 'eq' ? 2 : 3], repeat: br[cmp[2] === 'eq' ? 3 : 2] }
}

export function popBody(ir) {
  const { arg: root, cfg, entry } = runtimeCfg(ir, 'arc_release_zero_locked', 'i64')
  const push = /^(?:tail )?call (?:fastcc )?void @arc_wl_push\(i64 (?:noundef )?(%[\w.]+)\)$/.exec(entry[1])
  if (entry.length !== 5 || entry[0] !== 'store i64 0, ptr @g_wl_top, align 8' || push?.[1] !== root) throw Error('unsupported destruction entry')
  const start = loopGuard(entry.slice(2))
  const body = cfg.get(start.repeat) ?? []
  const next = loopGuard(body.slice(-3))
  if (next.repeat !== start.repeat || next.exit !== start.exit || cfg.get(next.exit)?.join(' ') !== 'ret void')
    throw Error('unsupported destruction back edge')
  const phi = /^(%[\w.]+) = phi i64 (.*)$/.exec(body[0])
  const inputs = phi?.[2].match(/\[ [^\]]+ \]/g) ?? []
  if (inputs.length !== 2 || !inputs.includes('[ ' + start.depth + ', %entry ]') || !inputs.includes('[ ' + next.depth + ', %' + start.repeat + ' ]'))
    throw Error('unsupported destruction loop depth')
  return popWrites(body.slice(1, -3), phi[1])
}

function popWrites(lines, depth) {
  const base = /^(%[\w.]+) = load ptr, ptr @g_wl, align 8$/.exec(lines[0])
  const dec = /^(%[\w.]+) = add i64 (%[\w.]+), -(\d+)$/.exec(lines[1])
  const store = /^store i64 (%[\w.]+), ptr @g_wl_top, align 8$/.exec(lines[2])
  const gep = wordSlot.exec(lines[3])
  const load = /^(%[\w.]+) = load i64, ptr (%[\w.]+), align 8$/.exec(lines[4])
  const cast = /^(%[\w.]+) = inttoptr i64 (%[\w.]+) to ptr$/.exec(lines[5])
  const drop = /^(?:tail )?call (?:fastcc )?void @arc_drop_body\(ptr (?:noundef )?(%[\w.]+)\)$/.exec(lines[6])
  if (lines.length !== 7 || !base || !dec || !store || !gep || !load || !cast || !drop ||
      dec[2] !== depth || gep[3] !== base[1] || load[2] !== gep[1] || cast[2] !== load[1] || drop[1] !== cast[1])
    throw Error('unsupported compiled worklist pop')
  const regs = new Map([[depth, 'depth'], [dec[1], '(depth - ' + dec[3] + ')']])
  if (!regs.has(store[1]) || !regs.has(gep[4])) throw Error('unknown worklist index')
  return '  (' + regs.get(store[1]) + ', memory (' + regs.get(gep[4]) + ' * ' + (gep[2] ?? 8) + '))'
}

export function bodyPaths(ir) {
  const { args: [body], lines } = voidBody(ir, 'arc_drop_body')
  const cfg = blocks(lines)
  function visit(label, events, seen) {
    if (seen.includes(label) || !cfg.has(label)) throw Error('unsupported cycle or label in destruction')
    const block = cfg.get(label)
    for (const line of block) {
      if (line === 'ret void') return ['[' + events.join(', ') + ']']
      if (/^(?:switch |br )/.test(line)) {
        const labels = [...block.slice(block.indexOf(line)).join(' ').matchAll(/label %([\w.]+)/g)].map(m => m[1])
        if (!labels.length) throw Error('missing destruction successors')
        return [...new Set(labels)].flatMap(to => visit(to, [...events], [...seen, label]))
      }
      if (/^%[\w.]+ = load /.test(line)) { events.push('.inspect'); continue }
      if (/^%[\w.]+ = (?:trunc |and |or |icmp |zext |lshr |getelementptr )/.test(line)) continue
      const call = /^(?:%[\w.]+ = )?(?:tail )?call (?:fastcc )?(?:ptr|void) @(arc_hdr|arc_drop_masked|arc_drop_list_hdr|arc_drop_ptr_array|arc_free_or_recycle)\(ptr (?:noundef )?(%[\w.]+)(?:,|\))/.exec(line)
      if (!call || call[2] !== body) throw Error('unsupported destruction operation: ' + line)
      events.push(call[1] === 'arc_hdr' ? '.inspect' : call[1] === 'arc_free_or_recycle' ? '.reclaim' : '.scan')
    }
    throw Error('unterminated destruction path')
  }
  return '  [' + [...new Set(visit('entry', [], []))].join(', ') + ']'
}
