// [NATIVE-SHM-ABI] Use the upstream generator; never duplicate layout arithmetic.
// Until its release is pinned, pass the reviewed typeDiagram core dist directory.
import { readFile, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

// [NATIVE-SHM-WASM] One header for both pointer widths. The layouts must agree byte for byte; only
// the fingerprint and pointer size differ, so the header names both fingerprints and selects one by
// the compiler's pointer width: a host built for one width can never open the other's bridge.
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
const shared = ['ApplicationInstance', 'OwnerToken', 'AllocationReference', 'OwnedLease', 'BridgeError', 'SceneShape', 'ViewRow', 'AskRow']
const widthLines = /^#define TD_SHARED_(FINGERPRINT|POINTER_BYTES) .*$/gm
const fingerprintOf = header => /^#define TD_SHARED_FINGERPRINT (".*")$/m.exec(header)[1]
const wide = unwrap(await generateSharedC(model, 8, shared))
const narrow = unwrap(await generateSharedC(model, 4, shared))
if (wide.replace(widthLines, '') !== narrow.replace(widthLines, '')) throw new Error('the 64-bit and wasm32 layouts differ beyond their fingerprints')
const selected = [
  '#include <stdint.h>',
  `#define TD_SHARED_FINGERPRINT_64 ${fingerprintOf(wide)}`,
  `#define TD_SHARED_FINGERPRINT_32 ${fingerprintOf(narrow)}`,
  '#if UINTPTR_MAX == UINT32_MAX',
  '#define TD_SHARED_FINGERPRINT TD_SHARED_FINGERPRINT_32',
  '#define TD_SHARED_POINTER_BYTES 4',
  '#else',
  '#define TD_SHARED_FINGERPRINT TD_SHARED_FINGERPRINT_64',
  '#define TD_SHARED_POINTER_BYTES 8',
  '#endif',
].join('\n')
await writeFile(destination, wide.replace(/^#define TD_SHARED_FINGERPRINT .*\n#define TD_SHARED_POINTER_BYTES 8$/m, selected) + '\n')
