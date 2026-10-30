# Proof, language and discipline conformance gates.

# Assertion-driven language corpus: recursively runs both *.test.osp and
# *.test.ospml suites. These inspect internal values rather than stdout goldens.
# Some loopback integration twins intentionally use the same fixed port so their
# Default and ML sources lower to identical IR; one worker prevents port races.
# _test_proof_kernel: the proof kernel of plan 0041 (examples/proof/kernel)
# checks `id_imp` and rejects the controls with the kernel's own reason,
# byte-exact against its golden, and its assertion suite (adversarial
# structural-equality and universe cases, every single-edit corruption of the
# recorded export asserted on term structure, never on rendering) passes,
# under GC, ARC and on wasm32 [PROOF-KERNEL]. No pipe hides a status: every
# run is written to a file and must exit zero; a golden run is then diffed,
# and a suite run's TAP summary must say `failed=0` on all three backends.
_test_proof_kernel: _test_proof_kernel_gc _test_proof_kernel_arc _test_proof_kernel_wasm

_test_proof_kernel_gc _test_proof_kernel_arc: _test_proof_kernel_%:
	@$(MKDIR) target/proof
	./$(BIN) examples/proof/kernel --run --memory=$* 2>/dev/null > target/proof/kernel.$*.txt
	diff target/proof/kernel.$*.txt examples/proof/kernel/expectedoutput
	cd examples/proof/export_check && ../../../$(BIN) . --run --memory=$* 2>/dev/null > ../../../target/proof/export-check.$*.txt
	diff target/proof/export-check.$*.txt examples/proof/export_check/expectedoutput
	./$(BIN) examples/proof/kernel/tests --run --memory=$* 2>/dev/null > target/proof/kernel-tests.$*.txt; status=$$?; tail -1 target/proof/kernel-tests.$*.txt; test $$status -eq 0
	grep -q "failed=0" target/proof/kernel-tests.$*.txt

_test_proof_kernel_wasm:
	@$(MKDIR) target/proof
	./$(BIN) examples/proof/kernel --target=wasm32 --compile -o target/proof/kernel.wasm >/dev/null 2>&1
	node scripts/wasm-smoke.mjs target/proof/kernel.wasm examples/proof/kernel/expectedoutput
	./$(BIN) examples/proof/kernel/tests --target=wasm32 --compile -o target/proof/kernel-tests.wasm >/dev/null 2>&1
	node scripts/wasm-smoke.mjs target/proof/kernel-tests.wasm >/dev/null
	grep -q "failed=0" target/proof/kernel-tests.wasm.stdout.txt

# _test_disciplines_passthrough: the `passthrough` reference discipline of spec
# 0050 ([TYPES-DISCIPLINE-REFERENCE]), run by `_test_disciplines` below. Its
# suite proves the marker rewrite, the stderr-to-diagnostic parser and the exact
# protocol JSON; the round trip proves an `hm` module importing a disciplined one
# and the reverse, byte-exact against its golden under GC, ARC and wasm32; the
# same program with the marker removed prints the same bytes; and a planted type
# error proves the discipline is what rejected the module, because the
# diagnostic carries its prefix.
_test_disciplines_passthrough:
	@echo "==> [disciplines] passthrough checker suite..."
	./$(BIN) test examples/disciplines/passthrough
	@echo "==> [disciplines] passthrough round trip: GC, ARC, wasm32..."
	set -o pipefail && ./$(BIN) examples/disciplines/roundtrip --run --memory=gc 2>/dev/null | diff - examples/disciplines/roundtrip/expectedoutput
	set -o pipefail && ./$(BIN) examples/disciplines/roundtrip --run --memory=arc 2>/dev/null | diff - examples/disciplines/roundtrip/expectedoutput
	@$(MKDIR) target/disciplines
	./$(BIN) examples/disciplines/roundtrip --target=wasm32 --compile -o target/disciplines/roundtrip.wasm >/dev/null 2>&1
	node scripts/wasm-smoke.mjs target/disciplines/roundtrip.wasm examples/disciplines/roundtrip/expectedoutput
	@echo "==> [disciplines] the same program under hm alone (marker removed)..."
	@rm -rf target/disciplines/roundtrip-hm && $(MKDIR) target/disciplines/roundtrip-hm/src
	@cp examples/disciplines/roundtrip/src/main.ospml examples/disciplines/roundtrip/src/numbers.ospml target/disciplines/roundtrip-hm/src/
	@grep -v 'osprey: types=passthrough' examples/disciplines/roundtrip/src/count.ospml > target/disciplines/roundtrip-hm/src/count.ospml
	@grep -v '@osprey/passthrough' examples/disciplines/roundtrip/osprey.toml > target/disciplines/roundtrip-hm/osprey.toml
	set -o pipefail && ./$(BIN) target/disciplines/roundtrip-hm --run --memory=gc 2>/dev/null | diff - examples/disciplines/roundtrip/expectedoutput
	@echo "==> [disciplines] the checker is what rejected a broken module..."
	@rm -rf target/disciplines/roundtrip-broken && $(MKDIR) target/disciplines/roundtrip-broken/src
	@cp examples/disciplines/roundtrip/src/main.ospml examples/disciplines/roundtrip/src/numbers.ospml examples/disciplines/roundtrip/src/count.ospml target/disciplines/roundtrip-broken/src/
	@printf '    export broken () = 1 + "x"\n' >> target/disciplines/roundtrip-broken/src/count.ospml
	@sed 's|"../passthrough"|"../../../examples/disciplines/passthrough"|' examples/disciplines/roundtrip/osprey.toml > target/disciplines/roundtrip-broken/osprey.toml
	@if ./$(BIN) target/disciplines/roundtrip-broken --check > target/disciplines/broken.txt 2>&1; then \
	  echo "FAIL: the module with a planted type error was accepted"; cat target/disciplines/broken.txt; exit 1; fi
	@grep -q 'passthrough/check: type mismatch' target/disciplines/broken.txt
	@cat target/disciplines/broken.txt
	@$(RM) examples/disciplines/roundtrip/.osprey examples/disciplines/passthrough/.osprey

_test_language_corpus:
	@echo "==> [language] Default + ML assertion corpus..."
	@OSPREY_TEST_JOBS=1 ./$(BIN) test tests

# _test_goldens: byte-exact stdout comparison against the .expectedoutput
# goldens. Distinct from the target above, which runs the same corpus through
# the built-in TAP runner and so only ever observes whether the in-language
# assertions passed. An assertion can only state what someone thought to write
# down as a value; the goldens pin the ENTIRE observable output — print ORDER,
# exact formatting, and the TAP tally that exposes a deleted assertion.
_test_goldens: _conformance-gc

# _test_wasm_goldens: the same corpus, the same goldens, the OTHER code
# generator. Every program that links on wasm32 is compiled to a module, run
# under Node's WASI host, and held to the byte-exact output the native backend
# produces; the rest report as named skips because they use a feature the
# portable runtime deliberately does not port [WASM-TARGET]. Without this the
# wasm target is gated by three hand-picked example programs, which cannot
# notice a backend that miscompiles arithmetic, strings, maps or pattern
# matching everywhere else.
_test_wasm_goldens:
	@echo "==> [wasm32] golden stdout comparison under WASI..."
	@WASM_CC="$(WASM_CC)" WASI_SYSROOT="$(WASI_SYSROOT)" bash scripts/test-wasm-arc-ownership.sh
	@OSPREY_TARGET=wasm32 zsh crates/run_test_corpus.sh

# _conformance-gc: run every assertion suite under the tracing GC backend.
_conformance-gc:
	@echo "==> [conformance] assertion corpus under --memory=gc..."
	@OSPREY_TEST_JOBS=1 zsh crates/run_test_corpus.sh gc

# _conformance-arc: run every assertion suite under the Perceus ARC backend;
# every suite must pass and end with zero live ARC objects. OSPREY_ARC_DEBUG=1 makes
# memory_arc.c report its live count at exit, which is the only automatic check
# for the [GC-ARC-PERCEUS] zero-leak bar.
_conformance-arc:
	@echo "==> [conformance] assertion corpus under --memory=arc..."
	@OSPREY_ARC_DEBUG=1 OSPREY_TEST_JOBS=1 zsh crates/run_test_corpus.sh arc

# _test_disciplines: the reference type disciplines of [TYPES-DISCIPLINE-REFERENCE]
# (spec 0050). The `bounded` package reads `// @bound p in 0..100`, holds every
# call site inside the module to it by interval arithmetic, and erases to plain
# `int`; `bounded_roundtrip` links that erased module into an `hm` program, so a
# green run proves the whole seam — checker build, protocol, erasure, signature
# ascription — and not just the checker. Every leg is byte-exact: the program's
# stdout against its golden under GC and ARC, the compiled wasm module against
# the same golden under WASI, and the out-of-range call site against the
# compiler's own stderr. The scratch copy keeps `path = "../bounded"` resolvable
# (`target/disciplines/bounded` beside `target/disciplines/bounded_roundtrip`),
# and `sed` drops the absolute prefix so the golden holds no checkout path.
# The checker's assertion suite drives the built binary through the protocol.
_test_disciplines:
	@echo "==> [disciplines] bounded: GC, ARC, wasm32 against expectedoutput..."
	set -o pipefail && ./$(BIN) examples/disciplines/bounded_roundtrip --run --memory=gc 2>/dev/null | diff - examples/disciplines/bounded_roundtrip/expectedoutput
	set -o pipefail && ./$(BIN) examples/disciplines/bounded_roundtrip --run --memory=arc 2>/dev/null | diff - examples/disciplines/bounded_roundtrip/expectedoutput
	@$(MKDIR) target/disciplines
	./$(BIN) examples/disciplines/bounded_roundtrip --target=wasm32 --compile -o target/disciplines/bounded.wasm >/dev/null 2>&1
	node scripts/wasm-smoke.mjs target/disciplines/bounded.wasm examples/disciplines/bounded_roundtrip/expectedoutput
	@echo "==> [disciplines] bounded: an out-of-range call site is rejected..."
	@$(RM) target/disciplines/bounded target/disciplines/bounded_roundtrip
	cp -R examples/disciplines/bounded target/disciplines/bounded
	cp -R examples/disciplines/bounded_roundtrip target/disciplines/bounded_roundtrip
	cp target/disciplines/bounded_roundtrip/src/bad.ospml.rejected target/disciplines/bounded_roundtrip/src/percent.ospml
	$(RM) target/disciplines/bounded_roundtrip/src/bad.ospml.rejected target/disciplines/bounded_roundtrip/expectedoutput
	$(RM) target/disciplines/bounded/.osprey target/disciplines/bounded_roundtrip/.osprey
	@./$(BIN) target/disciplines/bounded_roundtrip --check > target/disciplines/rejected.txt 2>&1; test $$? -ne 0
	sed -e "s|$(CURDIR)/||g" target/disciplines/rejected.txt | diff - examples/disciplines/bounded_roundtrip/rejected.expectedoutput
	@echo "==> [disciplines] bounded: the checker's protocol assertion suite..."
	@$(RM) examples/disciplines/bounded/.osprey
	@OSPREY_BIN="$(abspath $(BIN))" ./$(BIN) test examples/disciplines/bounded --memory=gc 2>/dev/null > target/disciplines/bounded-suite.txt; status=$$?; tail -2 target/disciplines/bounded-suite.txt; exit $$status
	grep -q "failed=0" target/disciplines/bounded-suite.txt
	@$(RM) examples/disciplines/bounded/.osprey examples/disciplines/bounded_roundtrip/.osprey
	$(MAKE) _test_disciplines_passthrough

.PHONY: _test_proof_kernel _test_proof_kernel_gc _test_proof_kernel_arc _test_proof_kernel_wasm
.PHONY: _test_disciplines _test_disciplines_passthrough

# Fresh checkers are shared read-only by the complete corpus checks. Make's
# jobserver bounds compilation and checking together; speed runs in isolation.
PROOF_BATCH_TARGETS := _test_proof_batch_gc _test_proof_batch_arc _test_proof_batch_wasm
PROOF_CORPUS_TARGETS := _test_proof_openai _test_proof_lean_core _test_proof_unicode

_test_proof_ci: _test_proof_kernel _test_disciplines $(PROOF_CORPUS_TARGETS)
	zsh benchmarks/proof/run.sh

_test_proof_batch_gc _test_proof_batch_arc: _test_proof_batch_%:
	@$(MKDIR) target/proof/openai
	./$(BIN) examples/proof/batch --compile --quiet --no-warnings --memory=$* -o target/proof/openai/check-batch.$*

_test_proof_batch_wasm:
	@$(MKDIR) target/proof/openai
	./$(BIN) examples/proof/batch --compile --quiet --no-warnings --target=wasm32 -o target/proof/openai/check-batch.wasm

$(PROOF_CORPUS_TARGETS): $(PROOF_BATCH_TARGETS)

_test_proof_openai:
	python3 scripts/proof-openai.py --compiler $(abspath $(BIN)) --reuse-binaries

_test_proof_lean_core:
	python3 scripts/proof-openai.py --corpus lean-core --compiler $(abspath $(BIN)) --reuse-binaries

_test_proof_unicode:
	python3 scripts/test-proof-literals.py

.PHONY: _test_proof_ci $(PROOF_BATCH_TARGETS) $(PROOF_CORPUS_TARGETS)
