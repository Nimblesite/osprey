// [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Reflect the complete straight-line
// constructor. Private stack stores disappear with its frame; no stack address
// may escape. Every instruction affecting the returned record is consumed.
export function pairConstructor(ir) {
  const body = /^define internal i8\* @pairUp\(i8\* %\$p0, i8\* %\$p1\) #\d+ \{\nentry:\n([\s\S]*?)^\}/m.exec(ir)
  if (!body) throw Error('missing pairUp specialization')
  const lines = recordInstructions(body[1])
  const shape = '{ i64, i8*, i8* }'
  const next = () => lines.shift()
  function result(rhs) {
    const [, reg, instruction] = /^(%r\d+) = (.+)$/.exec(next()) ?? []
    if (instruction !== rhs) throw Error('expected record instruction: ' + rhs)
    return reg
  }
  const size = result('getelementptr ' + shape + ', ' + shape + '* null, i64 1')
  const bytes = result('ptrtoint ' + shape + '* ' + size + ' to i64')
  const [, root, sizeArg, layout] = /^(%r\d+) = call i8\* @osp_alloc_tagged_noinit\(i64 (%r\d+), i64 (\d+)\)$/.exec(next()) ?? []
  if (!root || sizeArg !== bytes) throw Error('invalid record allocation')
  const typed = result('bitcast i8* ' + root + ' to ' + shape + '*')
  const base = 'getelementptr ' + shape + ', ' + shape + '* ' + typed + ', i32 0, i32 '
  const tag = result(base + '0')
  if (next() !== 'store i64 0, i64* ' + tag) throw Error('invalid record tag')
  const fields = []
  while (lines[0]?.includes(' = getelementptr ')) {
    const index = /, i32 (\d+)$/.exec(lines[0])?.[1]
    const field = result(base + index)
    const store = /^store i8\* %\$p([01]), i8\*\* (%r\d+)$/.exec(next())
    if (store?.[2] !== field) throw Error('invalid record field store')
    fields.push('(' + index + ', ' + (store[1] === '0' ? 'left' : 'right') + ')')
  }
  const returned = result('bitcast ' + shape + '* ' + typed + ' to i8*')
  if (next() !== 'ret i8* ' + returned || lines.length) throw Error('unsupported record exit')
  return '  (' + layout + ', [' + fields.reverse().join(', ') + '])'
}

function recordInstructions(body) {
  const slots = new Set()
  return body.trim().split('\n').map(line => line.trim()).filter(line => {
    const slot = /^(%arc\.s\d+) = alloca i8\*$/.exec(line)
    if (slot) {
      if (slots.has(slot[1])) throw Error('duplicate record frame slot')
      slots.add(slot[1])
      return false
    }
    const store = /^store i8\* (?:null|%\$p[01]|%r\d+), i8\*\* (%arc\.s\d+)$/.exec(line)
    return !store || !slots.has(store[1])
  })
}

// The specialized C walk loads pointer words and calls its real child drop.
// Preserve byte offsets and call order. An unrecognized instruction invalidates
// the reflection, including extra reads, branches and writes.
export function pairWalk(ir) {
  const layout = /^@proof_pair_layout = [^\n]*?constant i64 (\d+),/m.exec(ir)
  const body = /^define void @proof_pair_drop\([^\n]* %body\)[^\n]*\{\n[\w.]+:\n([\s\S]*?)^\}/m.exec(ir)
  if (!layout || !body) throw Error('missing specialized native record walk')
  const offsets = new Map([['%body', '0']])
  const loaded = new Map()
  const events = []
  const lines = body[1].trim().split('\n').map(line => line.trim())
  if (lines.pop() !== 'ret void') throw Error('unsupported record walk exit')
  for (const line of lines) {
    const gep = /^(%[\w.]+) = getelementptr (?:inbounds )?(?:nuw )?i8, ptr %body, i64 (\d+)$/.exec(line)
    const load = /^(%[\w.]+) = load ptr, ptr (%[\w.]+), align 8(?:, !tbaa !\d+)?$/.exec(line)
    const drop = /^(?:tail )?call (?:fastcc )?void @arc_drop_child\(ptr (?:noundef )?(%[\w.]+)\)$/.exec(line)
    if (gep) offsets.set(gep[1], gep[2])
    else if (load && offsets.has(load[2])) {
      loaded.set(load[1], offsets.get(load[2]))
      events.push('.read ' + offsets.get(load[2]))
    }
    else if (drop && loaded.has(drop[1])) events.push('.drop ' + loaded.get(drop[1]))
    else throw Error('unsupported record walk instruction: ' + line)
  }
  return '  (' + layout[1] + ', [' + events.join(', ') + '])'
}
