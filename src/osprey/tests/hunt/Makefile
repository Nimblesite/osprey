# Symbolic bug hunting with KLEE. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: hunt hunt-sample hunt-crashes hunt-corpus hunt-demo

## hunt: Symbolic bug hunting with KLEE on one program [SYMEX-CLI]:
##       make hunt FILE=path/to/program.osp [HUNT_ARGS="--max-time 120"]
hunt: build
	./target/release/osprey hunt $(FILE) $(HUNT_ARGS)

## hunt-sample: Qualify all runnable apps for a fast corpus; hard 10-second cutoff.
hunt-sample: build
	python3 scripts/hunt/hunt_sample.py $(HUNT_ARGS)

## hunt-crashes: Detect and natively reproduce every deliberately planted crash.
hunt-crashes: build
	$(MAKE) _test_hunt_crashes

## hunt-corpus: Hunt every program under src/osprey/tests/ and src/osprey/examples/ not listed in
##              src/osprey/tests/HUNT_UNPORTABLE.txt, then print the table [SYMEX-CORPUS].
hunt-corpus: build
	scripts/hunt/hunt_corpus.sh $(HUNT_ARGS)
	python3 scripts/hunt/hunt_summary.py target/hunt-corpus --findings-only

## hunt-demo: The bug-detection demo to show anyone first: a planted abort() on
##   input "9" (src/osprey/tests/hunt/crash.osp) must report `abort crash.osp:4` and
##   `reproduced (SIGABRT)`, matching src/osprey/tests/hunt/crash.expected. Clean controls:
##   src/osprey/tests/hunt/clean.osp and record_update_list.osp find nothing;
##   src/osprey/tests/hunt/process.osp is an engine limitation, never a bug demo.
hunt-demo:
	$(MAKE) -C src/osprey/tests/hunt hunt FILE=src/osprey/tests/hunt/crash.osp HUNT_ARGS="--input-bytes 1 --max-time 10 --out target/hunt-demo"
endif
