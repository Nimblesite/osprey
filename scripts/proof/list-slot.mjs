// [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Reflect one complete iteration
// from replace_slot's phi through the skip/retain edge, up to the loop latch.
import { pointerCfg, jumpTarget, operand } from './runtime-returns.mjs'

const slotAddress = /^(%[\w.]+) = getelementptr inbounds (?:nuw )?(?:(?:\[8 x i8\]|i64), ptr (%[\w.]+), i64 |\[32 x i64\], ptr (%[\w.]+), i64 0, i64 )(%[\w.]+)$/

function one(values, description) {
  if (values.length !== 1) throw Error('expected one ' + description)
  return values[0]
}

function decode(body, ...patterns) {
  if (body.length !== patterns.length) throw Error('unexpected slot block length')
  return patterns.map((pattern, i) => {
    const match = pattern.exec(body[i])
    if (!match) throw Error('unsupported slot instruction: ' + body[i])
    return match
  })
}

function retainIndex(body, fresh, indices, latch) {
  if (jumpTarget(body.at(-1)) !== latch) throw Error('unsupported slot retain path')
  const [gep, load, cast, retain] = decode(body.slice(0, -1),
    slotAddress,
    /^(%[\w.]+) = load i64, ptr (%[\w.]+), align 8$/,
    /^(%[\w.]+) = (?:tail )?call fastcc ptr @as_node_ptr\(i64 noundef (%[\w.]+)\)$/,
    /^(?:tail )?call void @osp_retain\(ptr noundef (%[\w.]+)\)$/)
  if ((gep[2] ?? gep[3]) !== fresh || load[2] !== gep[1] || cast[2] !== load[1] || retain[1] !== cast[1])
    throw Error('unsupported retained slot address')
  return '[' + operand(indices, gep[4]) + ']'
}

function slotFunction(ir, collision = false) {
  const { args, lines, cfg } = pointerCfg(ir, collision ? 'collision_words' : 'replace_slot')
  const allocator = collision ? 'alloc_slots' : 'alloc_trie_storage'
  const allocation = new RegExp('^(%[\\w.]+) = .*call .*ptr @' + allocator + '\\(')
  const allocated = one(lines.flatMap(s => s.match(allocation)?.slice(1) ?? []), 'slot allocation')
  if (args.length !== 5) throw Error('unsupported slot signature')
  return { args, cfg, allocated }
}

function checkLatch(cfg, header, phi, used) {
  const [inc, cmp, br] = decode(operand(cfg, phi[4]),
    /^(%[\w.]+) = add (?:nuw |nsw )*i64 (%[\w.]+), 1$/,
    /^(%[\w.]+) = icmp eq i64 (%[\w.]+), (%[\w.]+)$/,
    /^br i1 (%[\w.]+), label %([\w.]+), label %([\w.]+)(?:, !llvm\.loop !\d+)?$/)
  if (inc[1] !== phi[3] || inc[2] !== phi[1] || cmp[2] !== inc[1] || cmp[3] !== used ||
      br[1] !== cmp[1] || br[3] !== header || !operand(cfg, br[2]).at(-1)?.startsWith('ret ptr '))
    throw Error('unsupported slot iteration advance')
}

export function retainedSlot(ir) {
  const cast = pointerCfg(ir, 'as_node_ptr')
  const [ptr, ret] = decode(operand(cast.cfg, 'entry'),
    /^(%[\w.]+) = inttoptr i64 (%[\w.]+) to ptr$/, /^ret ptr (%[\w.]+)$/)
  if (ptr[2] !== cast.args[0] || ret[1] !== ptr[1]) throw Error('slot pointer conversion changed')
  const { args, cfg, allocated } = slotFunction(ir)
  const [header, loop] = one([...cfg].filter(([, b]) => b[0]?.includes(' = phi i64 ')), 'slot loop')
  const [phi, cmp, br] = decode(loop,
    /^(%[\w.]+) = phi i64 \[ 0, %([\w.]+) \], \[ (%[\w.]+), %([\w.]+) \]$/,
    /^(%[\w.]+) = icmp (eq|ne) i64 (%[\w.]+), (%[\w.]+)$/,
    /^br i1 (%[\w.]+), label %([\w.]+), label %([\w.]+)$/)
  if (cmp[3] !== phi[1] || br[1] !== cmp[1])
    throw Error('unsupported slot comparison')
  const pre = operand(cfg, phi[2])
  const [slot, used] = decode(pre.slice(0, -1), /^(%[\w.]+) = zext i32 (%[\w.]+) to i64$/,
    /^(%[\w.]+) = zext (?:nneg )?i32 (%[\w.]+) to i64$/)
  if (jumpTarget(pre.at(-1)) !== header || slot[1] !== cmp[4] || slot[2] !== args[3] || used[2] !== args[2])
    throw Error('comparison must use the replacement slot')
  checkLatch(cfg, header, phi, used[1])
  const indices = new Map([[phi[1], 'i'], [cmp[4], 'slot']])
  const edge = label => label === phi[4] ? '[]' : retainIndex(operand(cfg, label), allocated, indices, phi[4])
  return '  if i ' + (cmp[2] === 'eq' ? '=' : '≠') + ' slot then ' + edge(br[2]) + ' else ' + edge(br[3])
}

export function writtenSlot(ir, collision = false) {
  const { args, cfg, allocated } = slotFunction(ir, collision)
  const end = one([...cfg.values()].filter(body => body.at(-1)?.startsWith('ret ')), 'slot return')
  const [widen, gep, write] = decode(end.slice(0, -1),
    /^(%[\w.]+) = (sext|zext) i32 (%[\w.]+) to i64$/,
    slotAddress,
    /^store i64 (0|%[\w.]+), ptr (%[\w.]+), align 8$/)
  if (end.at(-1) !== 'ret ptr ' + allocated || widen[2] !== (collision ? 'zext' : 'sext') ||
      widen[3] !== args[collision ? 2 : 3] ||
      (gep[2] ?? gep[3]) !== allocated || gep[4] !== widen[1] || write[2] !== gep[1])
    throw Error('unsupported compiled slot write')
  return '  (slot.val * 8, ' + operand(new Map([[args[collision ? 3 : 4], '_value'], ['0', '0']]), write[1]) + ')'
}

// Only straight-line size arithmetic, allocation and memcpy can precede the
// reflected calls. Reject new effects before the first retain.
function collisionSetup(body) {
  const effects = body.filter(line => !/^%[\w.]+ = (?:zext|icmp|add|shl) /.test(line))
  decode(effects, /^%[\w.]+ = (?:tail )?call fastcc ptr @alloc_slots\(/,
    /^(?:tail )?call void @llvm\.memcpy\.p0\.p0\.i64\(/)
}

// LLVM xor with all ones complements a 32-bit word. The add wraps modulo 2^32.
export function collisionRanges(ir) {
  const { args: [, count, at, , managed], cfg, allocated } = slotFunction(ir, true)
  const entry = operand(cfg, 'entry')
  const call = /^(?:tail )?call fastcc void @retain_slots\(ptr (?:noundef |nonnull )*(%[\w.]+), i32 noundef (%[\w.]+), i32 noundef (%[\w.]+)\)$/
  const [prefix, cmp, br] = decode(entry.slice(-3), call,
    /^(%[\w.]+) = icmp ult i32 (%[\w.]+), (%[\w.]+)$/,
    /^br i1 (%[\w.]+), label %([\w.]+), label %([\w.]+)$/)
  collisionSetup(entry.slice(0, -3))
  if (cfg.size !== 3 ||
      prefix[1] !== allocated || prefix[3] !== managed || cmp[2] !== at || cmp[3] !== count ||
      br[1] !== cmp[1] || !operand(cfg, br[3]).at(-1)?.startsWith('ret ptr '))
    throw Error('unsupported collision prefix')
  const body = operand(cfg, br[2])
  const [index, base, offset, complement, sum, suffix] = decode(body.slice(0, -1),
    /^(%[\w.]+) = zext i32 (%[\w.]+) to i64$/, slotAddress,
    /^(%[\w.]+) = getelementptr inbounds (?:nuw )?i8, ptr (%[\w.]+), i64 (\d+)$/,
    /^(%[\w.]+) = xor i32 (%[\w.]+), -1$/,
    /^(%[\w.]+) = add i32 (%[\w.]+), (%[\w.]+)$/, call)
  if (jumpTarget(body.at(-1)) !== br[3] || index[2] !== at || (base[2] ?? base[3]) !== allocated ||
      base[4] !== index[1] || offset[2] !== base[1] || complement[2] !== at ||
      ![count, complement[1]].every(v => sum.slice(2).includes(v)) ||
      suffix[1] !== offset[1] || suffix[2] !== sum[1] || suffix[3] !== managed)
    throw Error('unsupported collision suffix')
  const first = operand(new Map([[count, 'count'], [at, 'atSlot']]), prefix[2])
  return '  ((' + first + '), if atSlot < count then some (atSlot.val * 8 + ' + offset[3] +
    ', (count.val + (4294967295 - atSlot.val)) % 4294967296) else none)'
}
