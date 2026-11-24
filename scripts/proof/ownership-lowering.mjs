// Reflect the complete scalar list-append specialization into Lean. Reject every
// unrecognized instruction, signature or source body [MEM-OWNERSHIP-STATIC-VERIFICATION].
import { readFileSync } from 'node:fs'
const [source, irFile, template] = process.argv.slice(2)
const src = readFileSync(source, 'utf8')
if (!/^fn grow\(xs, x\) = listAppend\(xs, x\)$/m.test(src)) throw Error('unsupported grow source')
const ir = readFileSync(irFile, 'utf8')
const bodies = [...ir.matchAll(/^define internal i8\* @grow\$mono\d+\(i8\* (%\$p0), i64 (%\$p1)\) #[0-9]+ \{\nentry:\n([^}]+)\n\}$/gm)]
if (bodies.length !== 1) throw Error('expected one scalar grow specialization')
const [, list, scalar, body] = bodies[0]
const regs = new Map([[list, 0]])
const slots = new Map()
function declare(map, name) {
  if (map.has(name)) throw Error('duplicate definition ' + name)
  map.set(name, map.size)
  return map.get(name)
}
function lookup(map, name) {
  if (!map.has(name)) throw Error('undefined operand ' + name)
  return map.get(name)
}
function instruction(line) {
  let m
  if ((m = /^(%arc\.s\d+) = alloca i8\*$/.exec(line))) return '.reserve ' + declare(slots, m[1])
  if ((m = /^store i8\* (null|%[\w$]+), i8\*\* (%arc\.s\d+)$/.exec(line))) {
    const value = m[1] === 'null' ? 'none' : '(some ' + lookup(regs, m[1]) + ')'
    return '.store ' + lookup(slots, m[2]) + ' ' + value
  }
  if ((m = /^(%r\d+) = call i8\* @osprey_list_append_of\(i8\* (%[\w$]+), i64 (%[\w$]+), i32 0\)$/.exec(line))) {
    if (m[3] !== scalar) throw Error('unsupported append element')
    const from = lookup(regs, m[2])
    return '.append ' + from + ' ' + declare(regs, m[1])
  }
  if ((m = /^call void @osp_ownership_free\(i8\* (%[\w$]+)\)$/.exec(line))) return '.free ' + lookup(regs, m[1])
  if ((m = /^ret i8\* (%[\w$]+)$/.exec(line))) return '.ret ' + lookup(regs, m[1])
  throw Error('unsupported emitted LLVM instruction: ' + line)
}
const code = body.trim().split('\n').map(line => instruction(line.trim()))
process.stdout.write(readFileSync(template, 'utf8').replace('-- EMITTED BODY', '[' + code.join(', ') + ']'))
