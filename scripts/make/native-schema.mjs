// [NATIVE-SHM-ABI] Use the upstream generator; never duplicate layout arithmetic.
// Until its release is pinned, pass the reviewed typeDiagram core dist directory.
import { readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

// [NATIVE-SHM-WASM] One header per pointer width: `native_abi.h` for 64-bit hosts and
// `native_abi_wasm32.h` for wasm32, whose strings (inside `BridgeError`) are half the size. Each
// carries its own fingerprint, so a host built for one width can never open the other's bridge.
const [generator, destination, wasm32] = process.argv.slice(2)
if (!generator || !destination || !wasm32) throw new Error('usage: native-schema.mjs <typediagram-core/dist> <header> <wasm32 header>')
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
const shared = ['ApplicationInstance', 'OwnerToken', 'AllocationReference', 'OwnedLease', 'BridgeError', 'SceneShape', 'ViewRow', 'AskRow']
for (const [width, path] of [[8, destination], [4, wasm32]]) {
  const header = unwrap(await generateSharedC(model, width, shared))
  await writeFile(path, header + '\n')
}
