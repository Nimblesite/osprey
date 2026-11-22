// Implements [ORCHESTRA-PLAN-CORE]: canonical schema, upstream Osprey emitter.
import { readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const [generator, schema, destination, namespace, projectRoot] = process.argv.slice(2)
if (!generator || !schema || !destination || !namespace) throw new Error('usage: osprey-schema.mjs <typediagram/dist> <schema.td> <output.ospml> <namespace>')
const upstream = pathToFileURL(resolve(generator) + '/')
const { parse } = await import(new URL('parser/index.js', upstream))
const { buildModel } = await import(new URL('model/index.js', upstream))
const swift = destination.endsWith('.swift')
const converter = await import(new URL(`converters/${swift ? 'swift' : 'osprey'}.js`, upstream))
const unwrap = result => {
  if (!result.ok) throw new Error(JSON.stringify(result.error))
  return result.value
}
const source = await readFile(schema, 'utf8')
const ast = unwrap(parse(source))
const model = unwrap(buildModel(ast))

// Each type is declared once. Projections let a renderer use the song types without linking the UI.
const projected = !swift && source.includes('# osprey namespace ')
if (projected) {
  const owners = new Map()
  const imports = new Map()
  let section
  const atLine = source.split('\n').map(line => {
    const declared = line.match(/^# osprey namespace (\w+)$/)
    if (declared) section = declared[1]
    const imported = line.match(/^# osprey import (.+)$/)
    for (const name of imported?.[1].split(/\s+/) ?? []) imports.set(name.split('::').at(-1), name)
    return section
  })
  for (const decl of ast.decls) owners.set(decl.name, atLine[decl.span.line - 1])
  model.decls = model.decls.filter(decl => owners.get(decl.name) === namespace)
  if (model.decls.length === 0) throw new Error(`No declarations in Osprey namespace ${namespace}`)
  function qualify(ref) {
    const owner = owners.get(ref.name)
    const qualified = imports.get(ref.name) ?? (owner && owner !== namespace ? `${owner}::${ref.name}` : undefined)
    if (qualified) {
      ref.name = qualified
      ref.resolution = { kind: 'declared', declName: qualified }
    }
    ref.args.forEach(qualify)
  }
  for (const decl of model.decls) {
    for (const field of decl.fields ?? []) qualify(field.type)
    for (const variant of decl.variants ?? []) for (const field of variant.fields) qualify(field.type)
    if (decl.kind === 'alias') qualify(decl.target)
  }
}
if (projectRoot) {
  const { projectCodec } = await import('./osprey-project-codec.mjs')
  await writeFile(destination, `namespace ${namespace}\n\n` + projectCodec(model, projectRoot))
} else {
  const generated = swift ? converter.generateSwiftModule(model) : converter.generateOspreyModule(model)
  await writeFile(destination, (swift ? '' : `namespace ${namespace}\n\n`) + (projected ? unwrap(generated).trimEnd() : unwrap(generated)) + '\n')
}
