// [PKG-CONSUMERS-BUILD] Compiles every tracked project whose source roots include a package's source, so a
// package change that breaks a consumer fails the package's own target.
// usage: node package-consumers.mjs --list <package>
//        node package-consumers.mjs <compiler> <package>
import { execFileSync, spawnSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { dirname, join, normalize } from 'node:path'

const [first, name] = process.argv.slice(2)
const source = `src/osprey/packages/${name}/src`
const roots = (toml) => [...(readFileSync(toml, 'utf8').match(/^source_roots\s*=\s*\[(.*)\]/m)?.[1] ?? '').matchAll(/"([^"]+)"/g)]
  .map(([, root]) => normalize(join(dirname(toml), root)))
const consumers = execFileSync('git', ['ls-files', '*osprey.toml'], { encoding: 'utf8' }).split('\n')
  .filter((toml) => toml && !toml.startsWith(`src/osprey/packages/${name}/`) && roots(toml).includes(source))
  .map(dirname).sort()
if (first === '--list') {
  console.log(consumers.join('\n'))
} else {
  for (const dir of consumers) {
    const { status, stderr } = spawnSync(first, [dir, '--llvm', '--quiet', '--no-warnings'], { encoding: 'utf8', stdio: ['ignore', 'ignore', 'pipe'] })
    console.log(`${status === 0 ? 'ok' : 'FAIL'} ${dir} consumes ${name}`)
    if (status !== 0) process.exitCode = 1, process.stderr.write(stderr)
  }
}
