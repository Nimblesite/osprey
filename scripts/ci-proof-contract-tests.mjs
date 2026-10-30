import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'

export function registerProofContracts(proof) {
  test('the proof kernel and the Lean speed gate run on the staged build', () => {
    assertProofWorkflow(proof)
    assertProofCommands(proof)
    assertProofGraph()
  })
}

function assertProofWorkflow(proof) {
  assert.match(proof, /needs: \[changes, build\]/)
  // examples/proof/** classifies as code and benchmarks/proof/** as
  // benchmarks; both outputs gate the job exactly as the changes job gates
  // its own benchmark steps.
  assert.match(proof, /needs\.changes\.outputs\.code == 'true' \|\| needs\.changes\.outputs\.benchmarks == 'true'/)
  assert.match(proof, /run: make -j3 _test_proof_ci/)
  assert.match(proof, /timeout-minutes: 45/)
  // Lean is the speed gate's baseline, so the toolchain is pinned to an exact
  // release (`stable` floats) and a non-zero exit is never softened. The pin
  // lives in the tools' lean-toolchain, which the job installs and the tools
  // cache key hashes, so the two cannot disagree.
  assert.match(proof, /--default-toolchain "\$\(cat benchmarks\/proof\/tools\/lean-toolchain\)"/)
  assert.match(readFileSync('benchmarks/proof/tools/lean-toolchain', 'utf8'), /^leanprover\/lean4:v\d+\.\d+\.\d+\n$/)
  assert.match(proof, /hashFiles\([^)]*benchmarks\/proof\/tools\/lean-toolchain/)
  assert.doesNotMatch(proof, /--default-toolchain stable/)
  assert.doesNotMatch(proof, /continue-on-error/)
  assert.doesNotMatch(proof, /\|\| true/)
}

function assertProofCommands(proof) {
  // The recorded export is re-derived from the pinned exporter in CI.
  assert.match(proof, /run: bash scripts\/proof-export\.sh --check/)
  assert.match(proof, /run: bash scripts\/proof-export\.sh --check --theorem and_swap/)
  assert.match(proof, /run: bash scripts\/proof-export\.sh --check --theorem Nat\.add_comm/)
  assert.match(proof, /run: node --test scripts\/test-proof-export\.mjs/)
  assert.match(proof, /run: python3 scripts\/test-proof-openai\.py/)
  assert.match(proof, /run: node --test scripts\/ci-proof-schedule\.test\.mjs/)
}

function assertProofGraph() {
  const graph = readFileSync('scripts/make/proof.mk', 'utf8')
  assert.match(graph, /_test_proof_ci: _test_proof_kernel _test_disciplines \$\(PROOF_CORPUS_TARGETS\)\n\tzsh benchmarks\/proof\/run\.sh/)
  assert.match(graph, /PROOF_CORPUS_TARGETS := _test_proof_openai _test_proof_lean_core _test_proof_unicode/)
  assert.match(graph, /PROOF_BATCH_TARGETS := _test_proof_batch_gc _test_proof_batch_arc _test_proof_batch_wasm/)
  assert.match(graph, /\$\(PROOF_CORPUS_TARGETS\): \$\(PROOF_BATCH_TARGETS\)/)
  assert.match(graph, /python3 scripts\/proof-openai\.py --compiler \$\(abspath \$\(BIN\)\) --reuse-binaries/)
  assert.match(graph, /python3 scripts\/proof-openai\.py --corpus lean-core --compiler \$\(abspath \$\(BIN\)\) --reuse-binaries/)
  assert.match(graph, /python3 scripts\/test-proof-literals\.py/)
  assert.match(graph, /--compile --quiet --no-warnings --memory=\$\* -o target\/proof\/openai\/check-batch\.\$\*/)
  assert.match(graph, /--compile --quiet --no-warnings --target=wasm32 -o target\/proof\/openai\/check-batch\.wasm/)
  assert.doesNotMatch(graph, /--skip-lean|--memory=default|\|\| true/)
}
