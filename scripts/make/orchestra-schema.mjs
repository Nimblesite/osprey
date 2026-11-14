// Implements [ORCHESTRA-PLAN-CORE]: canonical schema, upstream Osprey emitter.
import { readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const [generator] = process.argv.slice(2)
if (!generator) throw new Error('usage: orchestra-schema.mjs <typediagram/dist>')
const upstream = pathToFileURL(resolve(generator) + '/')
const { parse } = await import(new URL('parser/index.js', upstream))
const { buildModel } = await import(new URL('model/index.js', upstream))
const { generateOspreyModule } = await import(new URL('converters/osprey.js', upstream))
const unwrap = result => {
  if (!result.ok) throw new Error(JSON.stringify(result.error))
  return result.value
}
const base = new URL('../../src/osprey/examples/pocket_orchestra/', import.meta.url)
const model = unwrap(buildModel(unwrap(parse(await readFile(new URL('session.td', base), 'utf8')))))
await writeFile(new URL('src/model.ospml', base), 'namespace orchestra\n\n' + unwrap(generateOspreyModule(model)) + '\n')
