# Cross-language benchmarks. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: bench bench-osprey instruction-gate flock-bench painting-compare

## bench: Build, then run the cross-language performance benchmark suite
##        (Osprey vs Rust/C/OCaml/Haskell — CPU via hyperfine, peak memory via
##        /usr/bin/time). Absent toolchains are skipped. Informational only:
##        NOT part of `make ci`/`make test` (perf is noisy on shared runners).
##        Pass a name filter via BENCH_FILTER=<substr>; results in
##        src/toolchain/benchmarks/results/results.html. Every run preserves unmeasured
##        values and publishes only validated replacements. See src/toolchain/benchmarks/README.md.
bench: build
	@zsh src/toolchain/benchmarks/run.sh $(BENCH_FILTER)

## bench-osprey: Re-time ONLY the Osprey columns — default, `--memory=arc`,
##        `--memory=gc` and wasm32 — and merge them into the results already on
##        disk. No other language is compiled, run or timed, so this is the fast
##        loop for a src/toolchain/compiler/runtime change. Every non-Osprey measurement in
##        src/toolchain/benchmarks/results/ survives byte-for-byte at the data-record level;
##        the Osprey re-run happens in a temp dir and is published only if every
##        case succeeded, so a broken build can never erase the recorded data.
##        Needs one prior `make bench` for the other languages' baseline.
##        Honours BENCH_FILTER=<substr>. See src/toolchain/benchmarks/README.md.
bench-osprey: build
	@BENCH_PARTIAL=1 zsh src/toolchain/benchmarks/run.sh $(BENCH_FILTER)

## instruction-gate: Fail when an Osprey / checked-Rust instruction ratio rises above
##        instruction-ratios.json (cachegrind, so Linux with valgrind). The required
##        CI job runs this; its ratios only go down, through instruction_gate.py --record.
instruction-gate:
	cd src/toolchain/benchmarks && python3 instruction_gate.py

## flock-bench: macOS. Run Flock for 20 seconds and print fps, dispatch, paint and
##              envelope size. Keep the window in front.
flock-bench: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/flock/run.sh --bench 20

## painting-compare: macOS. Pixel-diff the painting port against GPUI's original, with and
##                   without synthetic strokes. Needs Screen Recording and Accessibility
##                   permission for the terminal.
painting-compare: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/painting/reference/compare.sh
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/painting/reference/compare.sh --draw
endif
