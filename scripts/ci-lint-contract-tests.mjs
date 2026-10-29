// The three `_lint` commands no other workflow runs, and the guard that keeps
// them reachable. `_lint` (scripts/make/tests.mk) has twelve steps; the
// required analysis job re-implements them inline, and these three were the
// only ones absent from every workflow — so a licence-bundle or dead-code
// regression could not fail a merge. The caller passes the `build` job text,
// so this file needs no workflow parser of its own.
import assert from 'node:assert/strict'
import { test } from 'node:test'

const LINT_COMMANDS = ['node scripts/verify-no-dead-code.mjs',
  'node scripts/verify-package-licenses.mjs', 'node --test scripts/test-package-licenses.mjs']

// A job-shaped block with the contract's own name and detector gate, so an
// evasion fails on the guard and not on an incidental mismatch.
const JOB_HEADER = '  build:\n    name: Build, Format & Analyse\n    needs: changes\n' +
  "    if: ${{ needs.changes.outputs.code == 'true' }}\n    runs-on: ubuntu-latest\n" +
  '    timeout-minutes: 30\n    steps:\n'

const EVASIONS = [
  `      # run: ${LINT_COMMANDS[0]}\n`,
  `      - if: \${{ needs.changes.outputs.code == 'true' }}\n        run: ${LINT_COMMANDS[0]}\n`,
  `      - name: Dead-code gate intact\n        continue-on-error: true\n        run: ${LINT_COMMANDS[0]}\n`,
  '      - name: Dead-code gate intact\n        run: node scripts/verify-node-deps-guard.mjs\n',
  `      - name: Dead-code gate intact\n        run: |\n          run: ${LINT_COMMANDS[0]}\n`,
]

// The step carrying `run: <command>` as its own raw eight-space run line, if
// any. Membership is checked before trimming, and a step that opens a block
// scalar is refused outright, so text inside a `run: |` body is never a run
// line however it is indented; comments and other steps cannot match either.
const OPENS_BLOCK = /^ {6}(?:- )?run: [|>]/
const stepRunning = (build, command) =>
  build.split(/(?=^ {6}- )/m).map((step) => step.split('\n'))
    .find((lines) => lines.includes(`        run: ${command}`) && !lines.some((line) => OPENS_BLOCK.test(line)))

// That step must also carry no `if:` in any spelling (`if:` indented under the
// dash or written on the dash line) and no `continue-on-error`, which would
// swallow the failure instead of failing CI.
const assertUngated = (build, command) => {
  const step = stepRunning(build, command)
  assert.ok(step, `${command} must run as its own run: line`)
  const lines = (step ?? []).map((line) => line.trim())
  assert.ok(!lines.some((line) => /^(?:-\s*)?if:/.test(line)), `${command} must not be individually skipped`)
  assert.ok(!lines.some((line) => line.includes('continue-on-error')), `${command} must not swallow a failure`)
}

const assertBuildRunsLint = (build) => {
  assert.match(build, /^    name: Build, Format & Analyse$/m)
  assert.match(build, /^    if: \$\{\{ needs\.changes\.outputs\.code == 'true' \}\}$/m)
  assert.doesNotMatch(build, /continue-on-error/)
  for (const command of LINT_COMMANDS) assertUngated(build, command)
}

const assertGuardRejectsEvasions = () => {
  for (const evasion of EVASIONS) {
    assert.throws(() => assertUngated(JOB_HEADER + evasion, LINT_COMMANDS[0]), evasion)
  }
}

export const registerLintContracts = (build) => {
  test('the build job runs the lint commands no other workflow covers', () => assertBuildRunsLint(build))
  test('the run-line guard rejects a commented, gated, suppressed or missing lint command', assertGuardRejectsEvasions)
}
