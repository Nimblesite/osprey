# Proof CI. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: proof

## proof: Proof CI locally (plan 0041). Fresh Lean exports must be byte-identical
##        (Lean 4.25.0); recorder (18) and corpus/staging (41) regressions pass;
##        then the kernel, disciplines and corpora under GC, ARC and wasm32:
##        openai 51 PASS, lean-core 525 PASS + 1,050 rejected mutations, Unicode
##        6 checks + 12 rejections. Every PASS row is followed by its rejected
##        corruption. Checked exports, not Mathlib or headline-theorem ports.
proof: build
	for theorem in id_imp and_swap Nat.add_comm; do bash scripts/proof/proof-export.sh --check --theorem $$theorem || exit 1; done
	node --test scripts/proof/test-proof-export.mjs
	python3 scripts/proof/test-proof-openai.py
	$(MAKE) -j3 _test_proof_ci
endif
