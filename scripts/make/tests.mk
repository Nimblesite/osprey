# --- rust (crates/) ---------------------------------------------------------
# cargo test is fail-fast at the binary level by
# default (a failing test binary aborts the run); coverage via cargo-llvm-cov.
# `--profile ci` is the workspace's fast-compile profile (see root Cargo.toml).
# `_runtime_wasm` is a prerequisite because the coverage gate below already
# DEPENDS on it: osprey-cli's wasm end-to-end test (crates/osprey-cli/src/wasm.rs)
# self-skips unless wasm-ld, a WASI sysroot and libosprey_runtime_wasm.a are all
# present, and skipping it drops osprey-cli from 96.7% to 94.8% — under its 95%
# floor. Without this line the archive is whatever an earlier target happened to
# leave behind, so a plain `make clean && make ci` fails on a coverage number
# that says nothing about the code that changed. Build what the gate measures.
_test_rust: _runtime_wasm
	@echo "==> [rust] running tests with coverage..."
	@$(MKDIR) $(COVERAGE_DIR) $(LOG_DIR)
	set -o pipefail && RUST_TEST_THREADS=$${RUST_TEST_THREADS:-4} cargo llvm-cov --workspace --profile ci --lcov --output-path $(RUST_LCOV) 2>&1 | tee $(RUST_LOG)

# Every crate under crates/ must have a valid threshold and measured lines.
# Check inventory before results so an omitted crate cannot silently pass.
_coverage_check_rust:
	@if [ ! -f "$(COVERAGE_THRESHOLDS_FILE)" ]; then echo "FAIL: $(COVERAGE_THRESHOLDS_FILE) not found"; exit 1; fi; \
	if [ ! -f $(RUST_LCOV) ]; then echo "[rust] FAIL: $(RUST_LCOV) not produced"; exit 1; fi; \
	fail=0; \
	for manifest in crates/*/Cargo.toml; do \
	  crate=$${manifest%/Cargo.toml}; crate=$${crate#crates/}; \
	  jq -e --arg c "$$crate" '.projects.rust | has($$c)' "$(COVERAGE_THRESHOLDS_FILE)" >/dev/null || { \
	    echo "[rust] $$crate FAIL: missing coverage threshold"; fail=1; }; \
	done; \
	for crate in $$(jq -er '.projects.rust | keys[]' "$(COVERAGE_THRESHOLDS_FILE)"); do \
	  threshold=$$(jq -er --arg c "$$crate" '.projects.rust[$$c].threshold | if type == "number" and . >= 95 and . <= 100 and floor == . then . else error("Rust coverage threshold must be an integer in 95..100") end' "$(COVERAGE_THRESHOLDS_FILE)") || exit 1; \
	  set -- $$(awk -F: -v c="$$crate" 'index($$0,"SF:")==1{in_c=index($$2,"/crates/" c "/")>0} in_c&&/^LH:/{h+=$$2} in_c&&/^LF:/{f+=$$2} END{printf "%d %d",h+0,f+0}' $(RUST_LCOV)); \
	  lh=$$1; lf=$$2; \
	  if [ "$$lf" -eq 0 ]; then echo "[rust] $$crate FAIL: no lines found in $(RUST_LCOV)"; fail=1; continue; fi; \
	  pct=$$(awk "BEGIN{printf \"%.1f\", $$lh/$$lf*100}"); \
	  pct_int=$$(awk "BEGIN{printf \"%d\", $$lh/$$lf*100}"); \
	  if [ "$$pct_int" -lt "$$threshold" ]; then \
	    echo "[rust] $$crate FAIL: $${pct}% < $${threshold}% ($$lh/$$lf lines)"; fail=1; \
	  else \
	    echo "[rust] $$crate OK: $${pct}% >= $${threshold}% ($$lh/$$lf lines)"; \
	  fi; \
	done; \
	if [ "$$fail" -ne 0 ]; then echo "[rust] FAIL: one or more crates below threshold"; exit 1; fi; \
	echo "[rust] OK: all crates meet their thresholds"

# Hardened C runtime unit tests (assertion-driven; a failed assert aborts the
# binary). Covers the string cursor (BUILTIN-STRING-CURSOR), the error-message
# contract ([ERR-PAYLOAD]), complete HTTP reads/writes, the fiber/channel and
# websocket surface and the reclaiming memory
# backend and both persistent containers: memory_arc.c (header, registry,
# retain/release, every layout kind's drop walk, the shim allocators) and
# list_runtime.c / map_runtime.c / map_runtime_hamt.c (trie + HAMT persistence,
# O(1) views, node refcounting) [GC-ARC-PERCEUS], [MEM-BACKENDS]. Built as
# executables (no `-c`), so they link the runtime TUs directly. Runs on
# `make test`; Windows CI uses its own steps.
#
# Shared container suites use GC hooks; dedicated ARC suites cover ownership.
C_TEST_MEMORY_SRC = runtime/memory_gc.c
C_TEST_MEMORY_LIBS = -pthread
OSSL_CFLAGS = $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""`
OSSL_LIBS   = `pkg-config --libs openssl 2>/dev/null || echo "-lssl -lcrypto"`
# The profiler's sources, linked by every suite that links a thread-spawning
# runtime unit [PROF-COLLECT-REGISTRY] [PROF-ATTACH-ENDPOINT].
PROF_SRC    = runtime/profiler_runtime.c runtime/profiler_store.c runtime/profiler_dump.c \
              runtime/profiler_sampler.c runtime/profiler_stream.c runtime/profiler_endpoint.c
RT_THREADS  = runtime/fiber_runtime.c runtime/system_runtime.c runtime/file_runtime.c runtime/effects_runtime.c runtime/effects_coro.c \
              $(PROF_SRC)
# Frame-pointer profile for the profiler suite: its unwind tests need -g and
# real frame chains, and it predates the WARN core, so it keeps its own flags.
PROF_T = -O2 -g -fno-omit-frame-pointer -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Werror -Wall -Wextra -ftrapv -std=c11 -D_GNU_SOURCE

# --- C runtime suite TABLE ---------------------------------------------------
# One row per test binary. C_SRC_<name> lists sources relative to compiler/,
# C_FLAGS_<name> extra compile flags, C_LIBS_<name> trailing libraries, and
# C_PROFILE_<name> optionally replaces the default $(T) flag core. BOTH the
# hardened run (_test_c_runtime) and the per-library coverage gate
# (_coverage_check_c_runtime) iterate this same table, so adding one row wires
# a suite into testing AND coverage measurement.
C_TEST_SUITES ?= memory_gc_stack_root_tests memory_arc_tests memory_gc_tests \
  host_import_gc_tests host_import_arc_tests \
  memory_pool_tests memory_golden_tests gpu_runtime_tests gpu_shader_tests gpu_opencl_tests gpu_selection_tests gpu_no_device_tests \
  list_tests map_tests string_runtime_tests float_spelling_portable_tests json_runtime_tests \
  effects_runtime_tests builtins_runtime_tests test_system_runtime \
  test_file_runtime tensor_list_tests \
  test_http_length_validation http_server_send_tests http_server_request_tests \
  fiber_runtime_tests http_runtime_tests http_shared_tests \
  websocket_runtime_tests profiler_runtime_tests profiler_stream_tests \
  coverage_runtime_tests
C_SRC_memory_gc_stack_root_tests = runtime/memory_gc_stack_root_tests.c runtime/memory_gc.c
C_LIBS_memory_gc_stack_root_tests = -pthread
C_SRC_memory_arc_tests = runtime/memory_arc_tests.c runtime/memory_arc.c
C_LIBS_memory_arc_tests = -pthread
C_SRC_memory_gc_tests = runtime/memory_gc_tests.c runtime/memory_gc.c
C_SRC_host_import_gc_tests = runtime/host_import_tests.c runtime/string_runtime.c runtime/float_spelling.c runtime/memory_gc.c
C_SRC_host_import_arc_tests = $(filter-out runtime/memory_gc.c,$(C_SRC_host_import_gc_tests)) runtime/memory_arc.c
C_FLAGS_host_import_arc_tests = -DHOST_IMPORT_ARC
C_LIBS_memory_gc_tests = -pthread
C_SRC_memory_pool_tests = runtime/memory_pool_tests.c
C_SRC_memory_golden_tests = runtime/memory_golden_tests.c runtime/memory_arc.c runtime/gpu_runtime.c runtime/gpu_host.c runtime/tensor_file.c runtime/file_runtime.c
C_LIBS_memory_golden_tests = -pthread
C_FLAGS_memory_golden_tests = $(GPU_DEVICE_FLAGS)
C_LIBS_memory_golden_tests += $(GPU_DEVICE_LINK) $(GPU_DEVICE_LIBS)
C_SRC_gpu_runtime_tests = runtime/gpu_runtime_tests.c runtime/gpu_runtime.c runtime/gpu_host.c runtime/tensor_file.c runtime/file_runtime.c $(C_TEST_MEMORY_SRC)
C_FLAGS_gpu_runtime_tests = $(GPU_DEVICE_FLAGS)
C_LIBS_gpu_runtime_tests = $(GPU_DEVICE_LINK) $(GPU_DEVICE_LIBS)
C_SRC_gpu_device_tests = runtime/gpu_device_tests.c runtime/gpu_runtime.c runtime/gpu_host.c runtime/tensor_file.c runtime/file_runtime.c $(C_TEST_MEMORY_SRC)
C_FLAGS_gpu_device_tests = $(GPU_DEVICE_FLAGS)
C_LIBS_gpu_device_tests = $(GPU_DEVICE_LINK) $(GPU_DEVICE_LIBS)
C_SRC_gpu_shader_tests = runtime/gpu_shader_tests.c
C_FLAGS_gpu_shader_tests = -ffp-contract=off
C_LIBS_gpu_shader_tests = -lm
C_SRC_gpu_opencl_tests = runtime/gpu_opencl_tests.c runtime/gpu_opencl.c runtime/file_runtime.c $(C_TEST_MEMORY_SRC)
C_FLAGS_gpu_opencl_tests = -DOSPREY_GPU_TESTING=1
C_LIBS_gpu_opencl_tests = -pthread $(if $(filter Linux,$(UNAME_S)),-ldl)
C_SRC_gpu_selection_tests = runtime/gpu_selection_tests.c runtime/gpu_runtime_tests.c runtime/gpu_runtime.c runtime/gpu_host.c runtime/tensor_file.c runtime/file_runtime.c $(C_TEST_MEMORY_SRC)
C_FLAGS_gpu_selection_tests = -DOSPREY_GPU_BUFFER_SUITE=1 -DOSPREY_GPU_METAL=1 -DOSPREY_GPU_OPENCL=1
C_SRC_gpu_no_device_tests = $(C_SRC_gpu_selection_tests)
C_FLAGS_gpu_no_device_tests = -DOSPREY_GPU_BUFFER_SUITE=1
ifeq ($(UNAME_S),Darwin)
C_TEST_SUITES += gpu_metal_tests
C_SRC_gpu_metal_tests = runtime/gpu_metal_tests.m runtime/gpu_metal.m runtime/file_runtime.c $(C_TEST_MEMORY_SRC)
C_FLAGS_gpu_metal_tests = -fobjc-arc -DOSPREY_GPU_METAL=1
C_LIBS_gpu_metal_tests = -framework Foundation -framework Metal -pthread
endif
C_SRC_list_tests = runtime/list_tests.c runtime/list_sort_tests.c runtime/list_runtime.c $(C_TEST_MEMORY_SRC)
C_SRC_map_tests = runtime/map_tests.c runtime/map_runtime.c runtime/map_runtime_hamt.c $(C_TEST_MEMORY_SRC)
C_SRC_string_runtime_tests = runtime/string_runtime_tests.c runtime/string_runtime.c runtime/float_spelling.c runtime/string_runtime_list.c runtime/list_runtime.c $(C_TEST_MEMORY_SRC)
# The same suite on the portable float-spelling path wasm32 uses.
C_SRC_float_spelling_portable_tests = $(C_SRC_string_runtime_tests)
C_FLAGS_float_spelling_portable_tests = -DOSP_FLOAT_SPELLING_PORTABLE
C_SRC_json_runtime_tests = runtime/json_runtime_tests.c runtime/json_grammar_tests.c runtime/json_runtime.c runtime/json_text.c runtime/json_codec.c runtime/json_codec_tests.c runtime/file_runtime.c $(C_TEST_MEMORY_SRC) runtime/string_runtime.c runtime/float_spelling.c
C_LIBS_json_runtime_tests = -pthread
C_SRC_effects_runtime_tests = runtime/effects_runtime_tests.c runtime/effects_runtime.c runtime/effects_coro.c runtime/memory_arc.c runtime/gpu_runtime.c runtime/file_runtime.c $(PROF_SRC)
C_LIBS_effects_runtime_tests = -pthread
C_SRC_builtins_runtime_tests = runtime/builtins_runtime_tests.c runtime/random_entropy_tests.c runtime/term_tty_tests.c runtime/ffi_runtime.c runtime/random_runtime.c runtime/term_runtime.c runtime/test_runtime.c
C_SRC_test_system_runtime = runtime/test_system_runtime.c runtime/test_system_failures.c runtime/system_runtime.c runtime/file_runtime.c $(C_TEST_MEMORY_SRC)
C_LIBS_test_system_runtime = -pthread
C_SRC_test_file_runtime = runtime/test_file_runtime.c runtime/file_runtime.c $(C_TEST_MEMORY_SRC)
C_SRC_tensor_list_tests = runtime/tensor_list_tests.c runtime/tensor_list.c runtime/tensor_file.c runtime/list_runtime.c runtime/file_runtime.c $(C_TEST_MEMORY_SRC)
C_FLAGS_test_http_length_validation = $(OSSL_CFLAGS)
C_SRC_test_http_length_validation = runtime/test_http_length_validation.c
C_FLAGS_http_server_send_tests = $(OSSL_CFLAGS)
C_SRC_http_server_send_tests = runtime/http_server_send_tests.c $(C_TEST_MEMORY_SRC)
C_LIBS_http_server_send_tests = -pthread
C_FLAGS_http_server_request_tests = $(OSSL_CFLAGS)
C_SRC_http_server_request_tests = runtime/http_server_request_tests.c $(C_TEST_MEMORY_SRC)
C_LIBS_http_server_request_tests = -pthread
C_SRC_fiber_runtime_tests = runtime/fiber_runtime_tests.c $(C_TEST_MEMORY_SRC) $(RT_THREADS)
C_LIBS_fiber_runtime_tests = -pthread
C_FLAGS_http_runtime_tests = $(OSSL_CFLAGS)
C_SRC_http_runtime_tests = runtime/http_runtime_tests.c runtime/http_origin_tests.c runtime/http_tls_tests.c runtime/http_server_live_tests.c runtime/http_client_runtime.c runtime/http_client_response.c runtime/http_server_runtime.c runtime/http_server_request.c runtime/http_server_response.c runtime/http_shared.c runtime/string_runtime.c runtime/float_spelling.c $(C_TEST_MEMORY_SRC) runtime/random_runtime.c $(RT_THREADS)
C_LIBS_http_runtime_tests = -pthread $(OSSL_LIBS)
# http_shared.c's coverage number is OWNED by this suite: it is the only one
# that drives every helper in the unit rather than the subset one transport
# reaches. Keep it that way -- the gate takes the MAX across suites, never a
# union, so splitting these tests across transports would lower the number
# while testing exactly as much.
C_FLAGS_http_shared_tests = $(OSSL_CFLAGS)
C_SRC_http_shared_tests = runtime/http_shared_tests.c runtime/http_shared.c runtime/random_runtime.c
C_LIBS_http_shared_tests = -pthread $(OSSL_LIBS)
# Owns websocket_client_runtime.c and websocket_server_runtime.c. Binds real
# loopback ports (18086/18087) and runs a real accept loop, so it is single
# threaded across suites by construction -- the Makefile runs suites serially.
C_FLAGS_websocket_runtime_tests = $(OSSL_CFLAGS)
C_SRC_websocket_runtime_tests = runtime/websocket_runtime_tests.c runtime/websocket_client_runtime.c runtime/websocket_server_runtime.c runtime/http_shared.c runtime/random_runtime.c $(C_TEST_MEMORY_SRC)
C_LIBS_websocket_runtime_tests = -pthread $(OSSL_LIBS)
C_PROFILE_profiler_runtime_tests = $(PROF_T)
C_SRC_profiler_runtime_tests = runtime/profiler_runtime_tests.c runtime/profiler_walk_tests.c runtime/profiler_spec_tests.c $(PROF_SRC)
C_LIBS_profiler_runtime_tests = -pthread
# Drives the live-attach endpoint over its real socket [PROF-ATTACH-ENDPOINT],
# inside a private TMPDIR, so it never touches the user's diagnostics directory.
C_SRC_profiler_stream_tests = runtime/profiler_stream_tests.c runtime/profiler_stream_test_support.c runtime/profiler_stream_lifecycle_tests.c $(PROF_SRC)
C_LIBS_profiler_stream_tests = -pthread
C_SRC_coverage_runtime_tests = runtime/coverage_runtime_tests.c runtime/coverage_runtime.c

# Shared GC suites manage container allocations too; never shim the collector itself.
C_SUITE_GC = $(if $(filter-out memory_gc_tests memory_gc_stack_root_tests,$(1)),$(filter $(C_TEST_MEMORY_SRC),$(C_SRC_$(1))))
C_SUITE_FLAGS = $(or $(C_PROFILE_$(1)),$(T)) $(C_FLAGS_$(1)) $(4)
C_SUITE_OBJECT = $(3)-$(basename $(notdir $(2))).o
C_GC_SHIM_SOURCES = $(patsubst bin/gc/%.o,runtime/%.c,$(filter bin/gc/%,$(FIB_OBJ_GC))) runtime/list_tests.c runtime/list_sort_tests.c runtime/map_tests.c
C_SUITE_BUILD = $(foreach src,$(C_SRC_$(1)),$(CC) $(call C_SUITE_FLAGS,$(1),,,$(4)) $(if $(and $(call C_SUITE_GC,$(1)),$(filter $(src),$(C_GC_SHIM_SOURCES))),-include $(2)runtime/osp_gc_shim.h) -c $(2)$(src) -o $(call C_SUITE_OBJECT,$(1),$(src),$(3)) && ) $(CC) $(foreach src,$(C_SRC_$(1)),$(call C_SUITE_OBJECT,$(1),$(src),$(3))) $(C_LIBS_$(1)) $(C_TEST_MEMORY_LIBS) $(4) -o $(3)
C_SUITE_CMD = echo "--- $(1)" && $(call C_SUITE_BUILD,$(1),,bin/$(1)) && ./bin/$(1)

_test_gc_stack_root:
	@echo "==> [c-runtime] GC caller-stack root regression..."
	@cd compiler && $(call C_SUITE_CMD,memory_gc_stack_root_tests)

_test_c_runtime: $(GPU_DEVICE_NATIVE)
	@echo "==> [c-runtime] memory/gpu/effects/json/builtins/system/string/HTTP/fiber suites..."
	@cd compiler && set -e; $(foreach s,$(C_TEST_SUITES),$(call C_SUITE_CMD,$(s)) && ) true

_test_gpu_device: $(GPU_DEVICE_NATIVE)
	@cd compiler && $(call C_SUITE_CMD,gpu_device_tests)

# [SYMEX-MOCK-TESTS] the mock C library behind `osprey hunt`, compiled for the
# host against its own headers and linked with the runtime units it must
# intercept (`osp_input`, `readFile`); the driver is compiled against the host
# headers. Runs without KLEE installed: the mock carries its own klee/klee.h.
# Deterministic Hunt runs [SYMEX-CLI]: each tests/hunt/<name>.osp must
# report exactly the findings in <name>.expected (see tests/hunt/README.md).
# Skipped loudly without KLEE, like the bitcode it needs. Like the two KLEE
# targets below, it is run by hand: neither `make test` nor CI runs the KLEE
# engine gates while issues #306 and #307 are open [SYMEX-CRASH-GATE].
# HUNT_KNOWN_BUGS: programs with a known complete-exploration defect, skipped
# by the expectation suite. clean.osp cannot finish inside the pinned
# 64-byte/30-second settings on either backend (~49s of solver work on idle
# Apple silicon; strictly worse under GC) — GitHub issue #306, plan 0034
# known-bug items, docs/reports/klee-gc-status-2026-10-26.md. Verify a fix
# with `make _test_hunt_clean`; it stays red until the fix lands.
HUNT_KNOWN_BUGS ?= clean

# Known-bug verification target, run by hand: red until the fix lands.
.PHONY: _test_hunt_clean
_test_hunt_clean:
	@if [ -z "$(KLEE_PREFIX)" ] || [ ! -f compiler/bin/libosprey_runtime_klee_gc.bc ]; then \
	  echo "(KLEE not installed — tests/hunt skipped)"; exit 0; fi; \
	set -e; fail=0; for backend in gc arc; do \
	  out="target/hunt-tests/$$backend/clean.json"; $(MKDIR) "target/hunt-tests/$$backend"; \
	  status=0; ./target/release/osprey hunt tests/hunt/clean.osp --memory=$$backend --json --quiet --no-replay --max-time 30 > "$$out" || status=$$?; \
	  python3 scripts/hunt_expect.py "$$out" tests/hunt/clean.expected "$$status" || fail=1; \
	done; exit $$fail

_test_hunt:
	@if [ -z "$(KLEE_PREFIX)" ] || [ ! -f compiler/bin/libosprey_runtime_klee_gc.bc ]; then \
	  echo "(KLEE not installed — tests/hunt skipped)"; exit 0; fi; \
	echo "==> [hunt] tests/hunt expectations..."; \
	set -e; fail=0; for backend in gc arc; do for p in tests/hunt/*.osp; do \
	  skip=0; for bug in $(HUNT_KNOWN_BUGS); do [ "$$(basename "$${p%.osp}")" = "$$bug" ] && skip=1; done; \
	  [ "$$skip" = 1 ] && continue; \
	  e="$${p%.osp}.expected"; out="target/hunt-tests/$$backend/$$(basename "$${p%.osp}").json"; \
	  $(MKDIR) "target/hunt-tests/$$backend"; \
	  status=0; ./target/release/osprey hunt "$$p" --memory=$$backend --json --quiet --no-replay --max-time 30 > "$$out" || status=$$?; \
	  python3 scripts/hunt_expect.py "$$out" "$$e" "$$status" || fail=1; \
	done; done; exit $$fail

# No skip path: run by hand (`make hunt-crashes`), it must detect and natively
# reproduce every planted failure.
.PHONY: _test_hunt_crashes
_test_hunt_crashes:
	@test -x "$(KLEE_PREFIX)/bin/klee" || { echo "FAIL: KLEE is required for planted crash tests"; exit 1; }
	@test -f compiler/bin/libosprey_runtime_klee_gc.bc
	@test -f compiler/bin/libosprey_runtime_klee_arc.bc
	python3 scripts/test-hunt-gc-roots.py --llvm "$(KLEE_LLVM_BIN)" --klee "$(KLEE_PREFIX)/bin/klee"
	@set -e; for backend in gc arc; do python3 scripts/hunt_crashes.py --memory $$backend; done

_test_hunt_mock:
	@echo "==> [hunt] mock C library unit tests..."
	@cd compiler && set -e && $(MKDIR) bin/hunt_mock && rm -f bin/hunt_mock/*.o && \
	  for u in $(HUNT_MOCK_SRC) random_runtime file_runtime; do \
	    $(CC) $(HUNT_MOCK_HOST_CFLAGS) -c runtime/$$u.c -o bin/hunt_mock/$$(basename $$u).o; \
	  done && \
	  for t in runtime/klee/hunt_mock*_tests.c; do \
	    $(CC) -std=c11 -O1 -g $(WARN_MAX) -fno-builtin -I runtime/klee -c $$t -o bin/hunt_mock/driver_$$(basename $$t .c).o; \
	  done && \
	  $(CC) bin/hunt_mock/*.o -o bin/hunt_mock_tests && ./bin/hunt_mock_tests
	@echo "==> [hunt] corpus scripts and the format oracle..."
	@python3 scripts/test-hunt-scripts.py
	@python3 scripts/test-hunt-sample.py

# Per-library C line-coverage gate. Rebuilds every table suite with gcov
# instrumentation into compiler/bin/cov/<suite>/, reruns it there (a failing
# suite still contributes whatever it covered before aborting), reduces each
# suite's gcov summaries, and gates every `language: "c"` entry in
# coverage-thresholds.json at its threshold. A library's number is the MAX
# line coverage across the suites linking it — per-TU summaries cannot be
# unioned, so max is the honest lower bound.
#
# The gate reads the JSON and looks each key up in the summaries, so a runtime
# .c that no key names is compiled, instrumented, summarised — and discarded.
# effects_coro.c reached 375 lines of the whole continuation core in exactly
# that state, split out of an already-gated effects_runtime.c and inheriting
# none of its 90% floor. The completeness check below closes it: every unit that
# SHIPS must be gated or exempt, and an unlisted one fails the gate.
#
# "Ships" is decided by ARCHIVE MEMBERSHIP, not by a name pattern over
# runtime/*.c. The first cut of this check skipped `test_*` to get past the test
# harness sources — and so skipped runtime/test_runtime.c, which is a real
# member of every native archive, leaving the gate green with it ungated and its
# exemption entry doing nothing. Membership is the fact the check actually
# wants; a filename never was. Units built only for wasm (web_runtime.c,
# wasm_builtins_runtime.c) are absent from these lists and so are not required —
# they do not build natively, so gcov has nothing to measure.
C_SHIPPED_UNITS = $(sort $(basename $(notdir $(FIB_OBJ_GC) $(HTTP_OBJ_GC) $(FIB_OBJ_ARC) $(HTTP_OBJ_ARC))))
# No GC/ARC native unit is exempt. term_runtime.c and test_runtime.c used to
# be: every one of their cases runs in a FORKED CHILD, whose gcov counters were
# never flushed back, so gcov reported 0% against thousands of passing
# assertions. The harness now calls __gcov_dump before the child _exits
# (compiler/runtime/test_gcov.h), which is what that exemption was waiting for.
C_COV_EXEMPT ?=
GCOV_TOOL ?= $(shell if $(CC) --version 2>/dev/null | grep -qi clang; then \
    if command -v xcrun >/dev/null 2>&1; then echo "xcrun llvm-cov gcov"; \
    else echo "llvm-cov gcov"; fi; \
  else echo gcov; fi)

_coverage_check_c_runtime:
	@echo "==> [c-runtime] per-library line coverage (gcov)..."
	@rm -rf compiler/bin/cov
	@set -e; cd compiler/bin && mkdir -p cov && cd cov; \
	$(foreach s,$(C_TEST_SUITES),mkdir -p $(s) && (cd $(s) && $(call C_SUITE_BUILD,$(s),../../../,$(s),--coverage -DOSP_COVERAGE_DUMP) && { ./$(s) >/dev/null 2>&1 || true; } && { $(GCOV_TOOL) *.gcda > summary.txt 2>/dev/null || true; }) && ) true
	@command -v jq >/dev/null || { echo "[c] FAIL: jq is required to read $(COVERAGE_THRESHOLDS_FILE); a gate that cannot run must not report success"; exit 1; }
	@libs=$$(jq -r --arg platform "$(if $(filter Darwin,$(UNAME_S)),darwin,$(if $(filter Linux,$(UNAME_S)),linux,windows))" '.projects.c_runtime | to_entries[] | select((.value.platform // $$platform)==$$platform) | .key' "$(COVERAGE_THRESHOLDS_FILE)"); \
	if [ -z "$$libs" ]; then echo "[c] FAIL: no C entries in $(COVERAGE_THRESHOLDS_FILE) -- the gate would pass vacuously"; exit 1; fi; \
	fail=0; \
	for n in $(C_SHIPPED_UNITS); do \
	  printf '%s\n' $(C_COV_EXEMPT) $$libs | grep -qx "$$n" || { \
	    echo "[c] FAIL: runtime/$$n.c ships in a native archive but is neither gated in $(COVERAGE_THRESHOLDS_FILE) nor in C_COV_EXEMPT -- an ungated library cannot regress visibly"; fail=1; }; \
	done; \
	for lib in $$libs; do \
	  thr=$$(jq -er --arg l "$$lib" '.projects.c_runtime[$$l] | (if has("threshold") then .threshold else 90 end) | if type == "number" and . >= 90 and . <= 100 and floor == . then . else error("coverage threshold must be an integer in 90..100") end' "$(COVERAGE_THRESHOLDS_FILE)") || exit 1; \
	  best=$$(for f in compiler/bin/cov/*/summary.txt; do \
	    [ -f "$$f" ] || continue; \
      sed -n "\#File '.*/runtime/$$lib\.[cmh]'#{n;s/Lines executed:\([0-9.]*\)% of .*/\1/p;}" "$$f"; \
	  done | sort -rn | head -1); \
	  if [ -z "$$best" ]; then echo "[c] $$lib FAIL: no coverage data (no suite links it, or its suite died before dumping)"; fail=1; continue; fi; \
	  best_int=$${best%.*}; \
	  if [ "$${best_int:-0}" -lt "$$thr" ]; then \
	    echo "[c] $$lib FAIL: $${best}% < $${thr}%"; fail=1; \
	  else \
	    echo "[c] $$lib OK: $${best}% >= $${thr}%"; \
	  fi; \
	done; \
	if [ "$$fail" -ne 0 ]; then echo "[c] FAIL: one or more C libraries below threshold"; exit 1; fi; \
	echo "[c] OK: all C libraries meet their thresholds"

# [PROF-TEST] end-to-end profiler gate: --profile runs, exports, and reports.
_test_profiler:
	@echo "==> [profiler] osprey --profile end-to-end..."
	@bash scripts/test_profiler.sh

# --- vscode-extension -------------------------------------------------------
# The extension's LSP server spawns the `osprey` binary at runtime, so the
# integration tests need a real compiler on PATH: the Rust binary is staged as
# `osprey`. vscode-test runs with V8 coverage; c8 merges the profiles into
# coverage/coverage-summary.json.
#
# `_vsix_bundle` is a prerequisite because staging PATH is NOT enough:
# `resolveServerCommand` prefers the extension's BUNDLED compiler
# (bin/<os>-<arch>/osprey) over PATH, so a bundle left behind by an earlier
# build silently shadows the one under test. That is a local-only trap — CI
# checks out a tree with no bin/ and falls through to PATH — which makes it
# exactly the kind of failure a developer cannot reproduce from a green CI run.
# Restaging keeps `make test` honest about which compiler it just tested.
# CI consumes the exact stage-1 bundle; public `make test` builds it first.
_test_vscode_extension: _vsix_verify $(EXT_NODE_DEPS) $(PROFILER_UI_BUILT)
	@echo "==> [vscode-extension] staging osprey as 'osprey' for LSP integration..."
	$(MKDIR) target/path-bin
	cp $(BIN) target/path-bin/osprey
	@echo "==> [vscode-extension] running tests with V8 coverage..."
	$(RM) $(EXT_DIR)/coverage
	cd $(EXT_DIR) && set -o pipefail && \
	  PATH="$(CURDIR)/target/path-bin:$$PATH" \
	  npm run pretest 2>&1 | tee test.log && \
	  PATH="$(CURDIR)/target/path-bin:$$PATH" \
	  ./node_modules/.bin/vscode-test --coverage --coverage-output coverage \
	    --coverage-reporter text-summary --coverage-reporter json-summary --coverage-reporter html 2>&1 | tee -a test.log
# The VS Code CLI otherwise waits for an interactive install prompt on WSL.
	@echo "==> [vscode-extension] testing the installed VSIX and its bundled compiler..."
	$(MAKE) _vsix_package
	cd $(EXT_DIR) && set -o pipefail && DONT_PROMPT_WSL_INSTALL=1 npm run test:vsix 2>&1 | tee -a test.log

# Gates EVERY metric the summary reports, not lines alone. Lines-only was the
# dishonest half of this gate: branches and functions were measured, printed by
# the text-summary reporter, and then thrown away, so a module could add an
# untested `else` arm or an unreachable handler without moving the number the
# gate reads. All four are held to the one threshold the project declares.
_coverage_check_vscode_extension:
	@if [ ! -f "$(COVERAGE_THRESHOLDS_FILE)" ]; then echo "FAIL: $(COVERAGE_THRESHOLDS_FILE) not found"; exit 1; fi; \
	THRESHOLD=$$(jq -r '.projects.editor["vscode-extension"].threshold' "$(COVERAGE_THRESHOLDS_FILE)"); \
	SUMMARY="$(EXT_DIR)/coverage/coverage-summary.json"; \
	if [ ! -f "$$SUMMARY" ]; then \
	  echo "[vscode-extension] FAIL: coverage-summary.json not produced"; exit 1; \
	fi; \
	SHORTFALL=""; \
	for METRIC in lines statements branches functions; do \
	  PCT=$$(jq -r --arg m "$$METRIC" '.total[$$m].pct' "$$SUMMARY"); \
	  if [ "$$PCT" = "null" ]; then \
	    echo "[vscode-extension] FAIL: the summary reports no $$METRIC total"; exit 1; \
	  fi; \
	  PCT_INT=$$(echo "$$PCT" | awk '{printf "%d", $$1}'); \
	  echo "[vscode-extension] $$METRIC: $${PCT}% (threshold: $${THRESHOLD}%)"; \
	  if [ "$$PCT_INT" -lt "$$THRESHOLD" ]; then SHORTFALL="$$SHORTFALL $$METRIC=$${PCT}%"; fi; \
	done; \
	if [ -n "$$SHORTFALL" ]; then \
	  echo "[vscode-extension] FAIL: below $${THRESHOLD}% -$$SHORTFALL"; exit 1; \
	fi; \
	echo "[vscode-extension] OK: lines, statements, branches and functions all >= $${THRESHOLD}%"

# =============================================================================
# Repo-Specific Targets
# =============================================================================

# _tui: Build, then launch the interactive TUI demo (live GitHub API browser).
#       Runs in the current terminal so the raw-mode key reader gets real stdin.
_tui: build
	@echo "==> launching TUI demo (live GitHub API browser)"
	./$(BIN) examples/tui/api_browser.osp --run

# _gpu-demo: Render the gpu* kernel demo (fractal, shaded sphere, composite)
#            on the host backend [GPU-BACKEND-HOST]; the raster suite uses
#            gpuScan/gpuFilter, which have no device form yet, so a selected
#            device would decline it. For pixels on the actual GPU, see `make graphics`.
#            FLAVOR=ml renders the ML twin, which prints byte-identical output.
_gpu-demo: build
	@echo "==> rendering gpu* kernel demo on the host backend (CPU)"
	./$(BIN) tests/core/gpu/raster.test.$(if $(filter ml,$(FLAVOR)),ospml,osp) --run

# The same suite against an ALREADY-BUILT binary. No prerequisites, so the
# staged pipeline can run it on the downloaded `osprey-build` artifact. Going
# through `bank-test` there rebuilt the whole workspace and then ran `tsc -b`
# in vscode-extension, whose node_modules that job never installs — three
# TS2688 errors and a dead stage, for work the build job had already done.
_bank_test:
	@echo "==> Bank native tests (osprey test)..."
	./$(BIN) test examples/projects/modules/test

# Osprey package system (packages/manager/): one process runs every suite from the
# packages/manager/tests project with core, host and CLI sources assembled together,
# because `osprey test` compiles suites in isolation and cannot import them.
# packages/manager/fixture-check runs first and fails fast: every fixture is data,
# parses strictly, cites a defined spec section and is indexed, every typed
# fixture value is schema-valid and canonically ordered, and every registry
# source cites only defined sections. packages/manager/manifest/tests is the TOML
# parser and manifest-rule suite, a separate program until that code joins
# core. packages/manager/e2e runs last: it starts the
# server over packages/manager/sample-registry on 127.0.0.1:18797, compares every
# endpoint with the pure router, fetches every release, and builds and runs a
# consumer project against the fetched sources. Local tests and CI run all
# suites under both managed backends; the TAP runner propagates failed checks.
REGISTRY_MEMORY ?= gc
REGISTRY_NATIVE_CLIENT = $(CURDIR)/target/$(if $(filter arc,$(REGISTRY_MEMORY)),registry-tests/osprey-pkg-arc,osprey-pkg)
_test_registry: export OSPREY_E2E_COMPILER = $(abspath $(BIN))
_test_registry: export OSPREY_E2E_MEMORY = $(REGISTRY_MEMORY)
_test_registry:
	@case "$(REGISTRY_MEMORY)" in gc|arc) ;; *) echo "Registry tests require GC or ARC"; exit 1 ;; esac
	@echo "==> Registry fixture and schema validation (fail-fast)..."
	./$(BIN) test packages/manager/fixture-check --memory=$(REGISTRY_MEMORY)
	@echo "==> Registry manifest (TOML parser and manifest rules) tests..."
	./$(BIN) test packages/manager/manifest/tests --memory=$(REGISTRY_MEMORY)
	@echo "==> Registry CLI (package frontend) tests..."
	./$(BIN) test packages/manager/cli/tests --memory=$(REGISTRY_MEMORY)
	@echo "==> Registry (Osprey package system) tests..."
	./$(BIN) test packages/manager/tests --memory=$(REGISTRY_MEMORY)
	@echo "==> Registry end-to-end: server, fetch and consumer build..."
	./$(BIN) test packages/manager/e2e --memory=$(REGISTRY_MEMORY)
	@echo "==> Native publication and installed-source integrity regressions..."
	@if [ "$(REGISTRY_MEMORY)" = arc ] && [ -z "$$OSPREY_PACKAGE_CLIENT" ]; then \
	  mkdir -p "$(CURDIR)/target/registry-tests" && \
	  "$(abspath $(BIN))" build packages/manager/cli --memory=arc --quiet --no-warnings -o "$(REGISTRY_NATIVE_CLIENT)"; \
	fi
	OSPREY_PACKAGE_CLIENT="$${OSPREY_PACKAGE_CLIENT:-$(REGISTRY_NATIVE_CLIENT)}" python3 scripts/test-package-publication.py
	OSPREY_PACKAGE_CLIENT="$${OSPREY_PACKAGE_CLIENT:-$(REGISTRY_NATIVE_CLIENT)}" python3 scripts/test-package-interruption.py

# Blocked host capabilities, each asserted as the behavior the package system
# needs. Red is the documented state until the capability lands in Osprey;
# a probe turning green is the signal to delete its entry from
# packages/manager/core/src/capabilities.osp.
_test_registry_blockers:
	@echo "==> Registry blocked-capability probes (expected red until each lands)..."
	./$(BIN) packages/manager/blockers --run

# _language-test: Run the assertion-driven Default + ML core language corpus.
_language-test: build
	$(MAKE) _test_language_corpus

_lint: $(EXT_NODE_DEPS) $(PROFILER_UI_BUILT)
	@echo "==> Linting..."
	node scripts/verify-node-deps-guard.mjs
	node scripts/verify-no-dead-code.mjs
	node scripts/verify-release-gates.mjs
	node scripts/verify-package-licenses.mjs
	node --test scripts/test-package-licenses.mjs
	python3 scripts/test-release-gate.py
	python3 scripts/test-package-coverage.py
	python3 scripts/test-package-coverage-projects.py
	python3 scripts/test-python-app-ports-contracts.py
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cd $(EXT_DIR) && npm run lint

# _deslop: Code-duplication gate. Fails the build when measured
# duplication exceeds the ceiling in .deslop.toml (exit 3). Exclusions and the
# threshold live in that committed config — the single source of truth.
# The installer resolves the latest release for both local and hosted CI.

_deslop:
	@echo "==> Duplication gate (deslop)..."
	bash scripts/install-deslop.sh
	deslop . --nohtml --nojson --output $(CURDIR)/target/deslop-report --log-to-console --log-level error --no-color
