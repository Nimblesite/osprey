// [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Resolve the child's own registry
// lookup and interpret the shipped header switch, including its default edge.
import { blocks, nullSuccessors, operand, voidBody, wordSlot } from './runtime-returns.mjs'

const addressCast = /^(%[\w.]+) = ptrtoint ptr (%[\w.]+) to i64$/

function lookupForwarder(ir) {
  const { args: [child], lines } = voidBody(ir, 'arc_drop_child')
  const cast = addressCast.exec(lines[1])
  const lookup = /^(%[\w.]+) = (?:tail )?call (?:fastcc )?ptr @arc_lookup_body\(i64 (?:noundef )?(%[\w.]+)\)$/.exec(lines[2])
  const drop = /^(?:tail )?call (?:fastcc )?void @arc_drop_hdr\(ptr (?:noundef )?(%[\w.]+), ptr (?:noundef )?(%[\w.]+)\)$/.exec(lines[3])
  if (lines.length !== 5 || lines[0] !== 'entry:' || lines[4] !== 'ret void' ||
      !cast || !lookup || !drop || cast[2] !== child || lookup[2] !== cast[1] || drop[1] !== child || drop[2] !== lookup[1])
    throw Error('unsupported child lookup or header forwarding')
}

function recordAction(line, child, field, addresses, actions) {
  const store = /^store i32 (\d+), ptr (%[\w.]+), align \d+(?:, !tbaa !\d+)?$/.exec(line)
  const cast = addressCast.exec(line)
  const push = /^(?:tail )?call (?:fastcc )?void @arc_wl_push\(i64 (?:noundef )?(%[\w.]+)\)$/.exec(line)
  if (field && store?.[2] === field) actions.push('.write ' + store[1])
  else if (cast?.[2] === child) addresses.add(cast[1])
  else if (push && addresses.has(push[1])) actions.push('.queue child')
  else throw Error('unsupported child drop action: ' + line)
}

function effects(cfg, start, child, field) {
  const seen = new Set()
  const actions = []
  const addresses = new Set()
  let label = start
  while (!seen.has(label)) {
    seen.add(label)
    const body = cfg.get(label)
    if (!body) throw Error('missing child drop block: ' + label)
    for (const line of body.slice(0, -1)) {
      recordAction(line, child, field, addresses, actions)
    }
    if (body.at(-1) === 'ret void') return '[' + actions.join(', ') + ']'
    const branch = /^br label %([\w.]+)$/.exec(body.at(-1))
    if (!branch) throw Error('unsupported child drop terminator')
    label = branch[1]
  }
  throw Error('cycle in child drop')
}

export function childDrop(ir) {
  lookupForwarder(ir)
  const { args: [child, header], lines } = voidBody(ir, 'arc_drop_hdr', 2)
  const cfg = blocks(lines)
  const [absent, present] = nullSuccessors(cfg.get('entry'), header)
  const dispatch = cfg.get(present) ?? []
  const field = /^(%[\w.]+) = getelementptr (?:inbounds )?(?:nuw )?i8, ptr (%[\w.]+), i64 (\d+)$/.exec(dispatch[0])
  const load = /^(%[\w.]+) = load i32, ptr (%[\w.]+), align \d+(?:, !tbaa !\d+)?$/.exec(dispatch[1])
  const sw = /^switch i32 (%[\w.]+), label %([\w.]+) \[$/.exec(dispatch[2])
  if (!field || !load || !sw || field[2] !== header || load[2] !== field[1] || sw[1] !== load[1] || dispatch.at(-1) !== ']')
    throw Error('unsupported child header switch')
  const run = label => effects(cfg, label, child, field[1])
  const arms = dispatch.slice(3, -1).map(line => {
    const arm = /^i32 (\d+), label %([\w.]+)$/.exec(line)
    if (!arm) throw Error('unsupported child switch case')
    return '    | ' + arm[1] + ' => ' + run(arm[2])
  })
  return ['  | none => ' + effects(cfg, absent, child, null),
    '  | some state => .read ' + field[3] + ' :: (match state with', ...arms, '    | _ => ' + run(sw[2]) + ')'].join('\n')
}

// The spare-capacity theorem follows the selected edge only. Uninterpreted
// paths (including growth) yield none, so they cannot justify a successful push.
function pushWrites(body, regs) {
  if (body?.length !== 6 || body[5] !== 'ret void') return 'none'
  const buffer = /^(%[\w.]+) = load ptr, ptr @g_wl, align 8(?:, !tbaa !\d+)?$/.exec(body[0])
  const add = /^(%[\w.]+) = add i64 (%[\w.]+), (\d+)$/.exec(body[1])
  const depth = /^store i64 (%[\w.]+), ptr @g_wl_top, align 8(?:, !tbaa !\d+)?$/.exec(body[2])
  const slot = wordSlot.exec(body[3])
  const write = /^store i64 (%[\w.]+), ptr (%[\w.]+), align 8(?:, !tbaa !\d+)?$/.exec(body[4])
  if (!buffer || !add || !depth || !slot || !write || buffer[1] !== slot[3] || write[2] !== slot[1]) return 'none'
  const read = reg => operand(regs, reg)
  regs.set(add[1], '(' + read(add[2]) + ' + ' + add[3] + ')')
  return 'some (' + read(depth[1]) + ', ' + read(slot[4]) + ' * ' + (slot[2] ?? '8') + ', ' + read(write[1]) + ')'
}

export function worklistPush(ir) {
  const { args: [child], lines } = voidBody(ir, 'arc_wl_push', 1, 'i64')
  const cfg = blocks(lines)
  const entry = cfg.get('entry') ?? []
  const load = /^(%[\w.]+) = load i64, ptr @g_wl_(top|cap), align 8(?:, !tbaa !\d+)?$/
  const first = load.exec(entry[0]), second = load.exec(entry[1])
  const cmp = /^(%[\w.]+) = icmp (eq|ne) i64 (%[\w.]+), (%[\w.]+)$/.exec(entry[2])
  const branch = /^br i1 (%[\w.]+), label %([\w.]+), label %([\w.]+)$/.exec(entry[3])
  if (entry.length !== 4 || !first || !second || !cmp || !branch || cmp[1] !== branch[1]) throw Error('unsupported worklist guard')
  const regs = new Map([[child, 'child'], [first[1], first[2]], [second[1], second[2]]])
  if (!regs.has(cmp[3]) || !regs.has(cmp[4])) throw Error('unknown worklist comparison operand')
  const path = label => pushWrites(cfg.get(label), new Map(regs))
  const [equal, different] = cmp[2] === 'eq' ? branch.slice(2) : branch.slice(2).reverse()
  return '  if ' + regs.get(cmp[3]) + ' = ' + regs.get(cmp[4]) + ' then ' + path(equal) + ' else ' + path(different)
}

// The unchecked arm of an LLVM select may produce poison. Keep it as none and
// select the resulting Option so overflow matters only on the chosen arm.
export function growthBytes(ir) {
  const fn = /^define [^\n@]*i64 @proof_growth_bytes\(i64 noundef (%[\w.]+)\)[^\n]*\{\nentry:\n([\s\S]*?)^\}/m.exec(ir)
  if (!fn) throw Error('missing native worklist growth calculation')
  const numbers = new Map([[fn[1], 'some capacity']]), tests = new Map()
  const number = val => /^\d+$/.test(val) ? 'some ' + val : numbers.get(val)
  const lines = fn[2].trim().split('\n').map(line => line.trim())
  const ret = /^ret i64 (%[\w.]+)$/.exec(lines.pop())
  for (const line of lines) {
    const cmp = /^(%[\w.]+) = icmp (eq|ugt) i64 (%[\w.]+), (\d+)$/.exec(line)
    const shift = /^(%[\w.]+) = shl nuw i64 (%[\w.]+), (\d+)$/.exec(line)
    const sel = /^(%[\w.]+) = select i1 (%[\w.]+), i64 ([%\w.]+), i64 ([%\w.]+)$/.exec(line)
    if (cmp && cmp[3] === fn[1]) tests.set(cmp[1], 'capacity ' + (cmp[2] === 'eq' ? '=' : '>') + ' ' + cmp[4])
    else if (shift && shift[2] === fn[1] && Number(shift[3]) < 64) numbers.set(shift[1], 'unsignedShift capacity ' + shift[3])
    else if (sel && tests.has(sel[2]) && number(sel[3]) && number(sel[4]))
      numbers.set(sel[1], '(if ' + tests.get(sel[2]) + ' then ' + number(sel[3]) + ' else ' + number(sel[4]) + ')')
    else throw Error('unsupported growth instruction: ' + line)
  }
  if (!ret || !numbers.has(ret[1])) throw Error('unsupported growth return')
  return '  ' + numbers.get(ret[1])
}
