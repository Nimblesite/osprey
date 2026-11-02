# Symbolic bug hunting with KLEE. Each target runs its recipe in the root Makefile, where it is the
# private `_` target of the same name; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../..

.PHONY: hunt hunt-sample hunt-crashes hunt-corpus hunt-demo

## hunt: Symbolic bug hunting with KLEE on one program [SYMEX-CLI]:
##       make hunt FILE=path/to/program.osp [HUNT_ARGS="--max-time 120"]
hunt:
	$(MAKE) -C $(ROOT) _hunt

## hunt-sample: Qualify all runnable apps for a fast corpus; hard 10-second cutoff.
hunt-sample:
	$(MAKE) -C $(ROOT) _hunt-sample

## hunt-crashes: Detect and natively reproduce every deliberately planted crash.
hunt-crashes:
	$(MAKE) -C $(ROOT) _hunt-crashes

## hunt-corpus: Hunt every program under tests/ and examples/ not listed in
##              tests/HUNT_UNPORTABLE.txt, then print the table [SYMEX-CORPUS].
hunt-corpus:
	$(MAKE) -C $(ROOT) _hunt-corpus

## hunt-demo: The bug-detection demo to show anyone first: a planted abort() on
##   input "9" (tests/hunt/crash.osp) must report `abort crash.osp:4` and
##   `reproduced (SIGABRT)`, matching tests/hunt/crash.expected. Clean controls:
##   tests/hunt/clean.osp and record_update_list.osp find nothing;
##   tests/hunt/process.osp is an engine limitation, never a bug demo.
hunt-demo:
	$(MAKE) -C $(ROOT) _hunt-demo
