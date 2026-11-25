# Proof CI. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: proof math mathlib-manifest mathlib-ledger

## math: Math to code against Lean (spec MATH-CODE, needs Lean 4.25.0). Every tracked
##       math source is accepted by the pinned Lean, and Lean computes the same bits
##       as the Osprey module generated from it, under GC and ARC. Expect one
##       `PASS control` row, one `PASS <source>` row per math source with its value
##       count, then `math oracle: <n> sources`.
math: build
	python3 scripts/math/math-oracle.py --compiler $(BIN)


## mathlib-manifest: The exhaustive manifest of pinned Mathlib (tag v4.25.0, plan 0041 M5) under
##                   target/proof/mathlib: every module with its imports, every declaration it
##                   reaches with kind, hashes, direct dependencies and assumed axioms, and every
##                   source and licence with its sha256. Expect `mathlib 1ccd71f8...`, the totals
##                   line `{"declarations":655176,...,"modules":9347,"root":"Mathlib"}` and both sha256s.
mathlib-manifest:
	bash scripts/proof/mathlib-manifest.sh

## mathlib-ledger: Check pinned Mathlib in dependency order (plan 0041 M5). Writes one incremental
##                 lean4export file per module under target/proof/mathlib/incremental (only what
##                 no earlier file declared), then checks MODULES of them (default 1) against the
##                 environment the earlier ones built. One `accepted <name>` or `rejected <name>:
##                 <reason>` line per item, then `ledger: accepted A rejected R unread files U of
##                 655176 declarations`.
mathlib-ledger: build
	cd src/toolchain/benchmarks/proof/mathlib && PATH="$$HOME/.elan/bin:$$PATH" lake exe incremental Mathlib $(CURDIR)/target/proof/mathlib/incremental 0 $(or $(MODULES),1)
	$(BIN) src/osprey/examples/proof/ledger --compile --no-warnings -o target/proof/ledger
	target/proof/ledger target/proof/mathlib/incremental $(or $(MODULES),1) 655176

## proof: Proof CI locally (plan 0041). Fresh Lean exports must be byte-identical
##        (Lean 4.25.0); recorder (18) and corpus/staging (41) regressions pass;
##        then the kernel, disciplines and corpora under GC, ARC and wasm32:
##        openai 51 PASS, lean-core 525 PASS + 1,050 rejected mutations, Unicode
##        6 checks + 12 rejections. Every PASS row is followed by its rejected
##        corruption. Mathlib: Function.Bijective.comp (tag v4.25.0) re-exports byte for
##        byte and its five kernel cases pass. Checked exports, not headline-theorem ports.
proof: build
	for theorem in id_imp and_swap Nat.add_comm Function.Bijective.comp; do bash scripts/proof/proof-export.sh --check --theorem $$theorem || exit 1; done
	node --test scripts/proof/test-proof-export.mjs
	python3 scripts/proof/test-proof-openai.py
	$(MAKE) -j3 _test_proof_ci
	$(MAKE) _check_synthesis_proofs
endif
