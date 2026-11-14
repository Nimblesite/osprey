// Implements [ORCHESTRA-PLAN-CORE]: canonical schema, upstream Osprey emitter.
import { readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const [generator, schema, destination, namespace] = process.argv.slice(2)
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
const model = unwrap(buildModel(unwrap(parse(await readFile(schema, 'utf8')))))
const generated = swift ? converter.generateSwiftModule(model) : converter.generateOspreyModule(model)
await writeFile(destination, (swift ? '' : `namespace ${namespace}\n\n`) + unwrap(generated) + '\n')
