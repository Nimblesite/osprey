// [NATIVE-SHM-ABI] Use the upstream generator; never duplicate layout arithmetic.
// Until its release is pinned, pass the reviewed typeDiagram core dist directory.
import { readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

const [generator, destination] = process.argv.slice(2)
if (!generator || !destination) throw new Error('usage: native-schema.mjs <typediagram-core/dist> <header>')
const upstream = pathToFileURL(resolve(generator) + '/')
const { parse } = await import(new URL('parser/index.js', upstream))
const { buildModel } = await import(new URL('model/index.js', upstream))
const { generateSharedC } = await import(new URL('converters/shared-c.js', upstream))
const unwrap = result => {
  if (!result.ok) throw new Error(JSON.stringify(result.error))
  return result.value
}
const source = await readFile(new URL('../../src/toolchain/schemas/native.td', import.meta.url), 'utf8')
const model = unwrap(buildModel(unwrap(parse(source))))
const header = unwrap(await generateSharedC(model, 8,
  ['ApplicationInstance', 'OwnerToken', 'AllocationReference', 'OwnedLease', 'BridgeError', 'SceneShape']))
await writeFile(destination, header + '\n')
