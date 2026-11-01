# Proof CI. Each target runs its recipe in the root Makefile, where it is the
# private `_` target of the same name; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../..

.PHONY: proof

## proof: Proof CI locally (plan 0041). Fresh Lean exports must be byte-identical
##        (Lean 4.25.0); recorder (18) and corpus/staging (41) regressions pass;
##        then the kernel, disciplines and corpora under GC, ARC and wasm32:
##        openai 51 PASS, lean-core 525 PASS + 1,050 rejected mutations, Unicode
##        6 checks + 12 rejections. Every PASS row is followed by its rejected
##        corruption. Checked exports, not Mathlib or headline-theorem ports.
proof:
	$(MAKE) -C $(ROOT) _proof
