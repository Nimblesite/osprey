// [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Interpret the continuation of
// the actual recursive HAMT call, assuming its output flag contains zero.
import { pointerCfg, jumpTarget, operand, zeroSuccessors } from './runtime-returns.mjs'

export function mapMissFrame(ir) {
  const name = 'osprey_map_node_remove'
  const { args, cfg } = pointerCfg(ir, name)
  if (args.length !== 6) throw Error('unsupported HAMT signature')
  const calls = [...cfg].flatMap(([label, body]) => body.flatMap((line, i) =>
    line.includes('@' + name + '(') ? [{ label, line, rest: body.slice(i + 1) }] : []))
  if (calls.length !== 1) throw Error('expected one recursive HAMT call')
  const { label, line, rest } = calls[0]
  const call = /^(%[\w.]+) = (?:tail )?call ptr @osprey_map_node_remove\(.*?, ptr (?:noundef |nonnull )*(%[\w.]+)\)$/.exec(line)
  const load = /^(%[\w.]+) = load i32, ptr (%[\w.]+), align \d+$/.exec(rest[0])
  if (!call || !load || call[2] !== args[5] || load[2] !== args[5]) throw Error('unsupported HAMT miss flag')
  const [miss] = zeroSuccessors(rest.slice(1), load[1], 'i32')
  const regs = new Map([['null', 'none'], [args[0], '_parent'], [call[1], '_child']])
  return returned(cfg, label, miss, regs)
}

function returned(cfg, previous, label, regs) {
  const seen = new Set(), releases = []
  while (!seen.has(label)) {
    seen.add(label)
    const body = operand(cfg, label)
    for (const line of body.slice(0, -1)) {
      const phi = /^(%[\w.]+) = phi ptr (.+)$/.exec(line)
      const drop = /^(?:tail )?call void @osp_release\(ptr (?:noundef |nonnull )*(null|%[\w.]+)\)$/.exec(line)
      if (phi) {
        const arms = [...phi[2].matchAll(/\[ (null|%[\w.]+), %([\w.]+) \]/g)]
        const choices = arms.filter(m => m[2] === previous)
        if (choices.length !== 1) throw Error('missing HAMT predecessor')
        regs.set(phi[1], operand(regs, choices[0][1]))
      } else if (drop) releases.push(operand(regs, drop[1]))
      else throw Error('unsupported HAMT miss operation: ' + line)
    }
    const ret = /^ret ptr (null|%[\w.]+)$/.exec(body.at(-1))
    if (ret) return '  ([' + releases.join(', ') + '], ' + operand(regs, ret[1]) + ')'
    previous = label
    label = jumpTarget(body.at(-1))
  }
  throw Error('cycle in HAMT miss propagation')
}
