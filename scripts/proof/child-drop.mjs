// [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Resolve the child's own registry
// lookup and interpret the shipped header switch, including its default edge.
import { blocks, nullSuccessors, voidBody } from './runtime-returns.mjs'

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
