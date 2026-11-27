// Coverage thresholds only go up (#232): every number in
// src/osprey/tests/coverage-thresholds.json is compared with the same path in
// the baseline (main's copy), and any that dropped fails the check. Monotonicity
// held by discipline alone until this ran; a key that disappears with its
// project is caught by the per-language gates' completeness checks instead.
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export const THRESHOLDS = 'src/osprey/tests/coverage-thresholds.json'

/** Every numeric leaf of `value`, as [dotted path, number]. */
const leaves = (value, path = []) =>
  typeof value === 'number'
    ? [[path.join('.'), value]]
    : value && typeof value === 'object'
      ? Object.entries(value).flatMap(([key, child]) => leaves(child, [...path, key]))
      : []

/** Each threshold in `current` lower than the same path in `baseline`. */
export const lowered = (baseline, current) => {
  const now = new Map(leaves(current))
  return leaves(baseline)
    .filter(([path, before]) => now.has(path) && now.get(path) < before)
    .map(([path, before]) => `${path}: ${before} -> ${now.get(path)}`)
}

if (resolve(process.argv[1] ?? '') === fileURLToPath(import.meta.url)) {
  const ref = process.argv[2] ?? 'origin/main'
  const baseline = JSON.parse(execFileSync('git', ['show', `${ref}:${THRESHOLDS}`], { encoding: 'utf8' }))
  const drops = lowered(baseline, JSON.parse(readFileSync(THRESHOLDS, 'utf8')))
  if (drops.length > 0) {
    console.error(`Coverage thresholds only go up; ${drops.length} lowered against ${ref}:\n${drops.join('\n')}`)
    process.exit(1)
  }
  console.log(`Coverage thresholds: none lowered against ${ref}`)
}
