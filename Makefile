# agent-pmo:b636503
# =============================================================================
# Standard Makefile — osprey
# Cross-platform: Linux, macOS, Windows (via GNU Make)
# Primary language: Rust (crates/ workspace → the osprey compiler), with a
# pure-C runtime (compiler/runtime → lib*_runtime.a, linked by `osprey
# --run`) and TypeScript sub-projects (vscode-extension, webcompiler, website).
# =============================================================================

.PHONY: build test lint fmt clean ci setup run install bench wasm wasm-site wasm-serve vsix-rebuild-reinstall

# ---------------------------------------------------------------------------
# OS Detection
# ---------------------------------------------------------------------------
ifeq ($(OS),Windows_NT)
  ifeq ($(MSYSTEM),)
    # Native Windows: PowerShell. (No MSYS2/MinGW environment present.)
    SHELL := powershell.exe
    .SHELLFLAGS := -NoProfile -Command
    RM = Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
    MKDIR = New-Item -ItemType Directory -Force
    HOME ?= $(USERPROFILE)
  else
    # MSYS2/MinGW shell (CI's UCRT64 runtime build). $(OS) is still Windows_NT
    # here, so without this branch we'd inherit PowerShell's `.SHELLFLAGS`
    # (`-NoProfile -Command`) and feed `-N` to bash. Force bash + `-c`.
    SHELL := /usr/bin/bash
    .SHELLFLAGS := -c
    RM = rm -rf
    MKDIR = mkdir -p
  endif
else
  # bash needed for `pipefail` in tee'd test recipes; Ubuntu's /bin/sh is dash.
  SHELL := /bin/bash
  RM = rm -rf
  MKDIR = mkdir -p
endif

# ---------------------------------------------------------------------------
# Variables. NOTE: `?=` (not `:=`) on purpose — the VSCode Makefile-Tools panel
# lists `:=` assignments as if they were targets; `?=` keeps the panel clean.
# ---------------------------------------------------------------------------
# Coverage — single source of truth is coverage-thresholds.json.
# See REPO-STANDARDS-SPEC [COVERAGE-THRESHOLDS-JSON].
COVERAGE_THRESHOLDS_FILE ?= coverage-thresholds.json

# Toolchain / paths. BIN: the built CLI. RTB: C-runtime archive output dir
# (osprey searches compiler/bin at --run time).
CC  ?= cc
AR  ?= ar
BIN ?= target/release/osprey
RTB ?= compiler/bin

# VSIX (VSCode extension) — macOS only. Bundles the Rust binary as `osprey`.
# All VSIX targets touch ONLY this extension id, ONLY in the default profile;
# they never enumerate VSCode profiles and never affect any other extension.
EXT_DIR        ?= vscode-extension
EXT_ID         ?= nimblesite.osprey

# C runtime compile flag profiles (hardened; mirror the original recipes).
A    ?= -c -fPIC -O2 -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Werror -Wall -Wextra -ftrapv -fPIE -D_GNU_SOURCE
B    ?= $(A) -std=c11
OSSL ?= -DOPENSSL_SUPPRESS_DEPRECATED -DOPENSSL_API_COMPAT=30000 -Wno-deprecated-declarations
# Object lists for the archives (paths relative to compiler/, where `ar` runs).
FIB_OBJ  ?= bin/memory_runtime.o bin/fiber_runtime.o bin/system_runtime.o bin/effects_runtime.o bin/string_runtime.o bin/string_runtime_list.o bin/list_runtime.o bin/map_runtime.o bin/map_runtime_hamt.o bin/json_runtime.o bin/ffi_runtime.o bin/term_runtime.o bin/random_runtime.o
HTTP_OBJ ?= bin/http_shared.o bin/http_client_runtime.o bin/http_server_runtime.o bin/websocket_client_runtime.o bin/websocket_server_runtime.o $(FIB_OBJ)
# GC backend archives (osprey --memory=gc): the tracing collector replaces
# memory_runtime.o, and the value-container units are rebuilt with the malloc
# redirect (osp_gc_shim.h) so their nodes live in the managed heap. Everything
# else is the same object. Implements [GC-TRACE-CONSERVATIVE], docs/plans/0011.
FIB_OBJ_GC  ?= bin/memory_gc.o bin/fiber_runtime.o bin/system_runtime.o bin/effects_runtime.o bin/string_runtime.o bin/string_runtime_list.o bin/gc/list_runtime.o bin/gc/map_runtime.o bin/gc/map_runtime_hamt.o bin/json_runtime.o bin/ffi_runtime.o bin/term_runtime.o bin/random_runtime.o
HTTP_OBJ_GC ?= bin/http_shared.o bin/http_client_runtime.o bin/http_server_runtime.o bin/websocket_client_runtime.o bin/websocket_server_runtime.o $(FIB_OBJ_GC)

# WebAssembly (wasm32-wasip1) cross-build toolchain — opt-in via `make wasm`.
# Compiles the portable C-runtime subset (no pthreads/sockets/OpenSSL/syscalls)
# to a wasm archive osprey links with `--target=wasm32`. See docs/specs/0022.
WASM_LLVM_BIN ?= $(shell for d in /opt/homebrew/opt/llvm/bin /usr/local/opt/llvm/bin; do [ -x "$$d/clang" ] && { echo "$$d"; break; }; done)
WASM_LLD_BIN  ?= $(shell for d in /opt/homebrew/opt/lld/bin /usr/local/opt/lld/bin "$(WASM_LLVM_BIN)"; do [ -n "$$d" ] && [ -x "$$d/wasm-ld" ] && { echo "$$d"; break; }; done)
WASM_PATH_PREFIX ?= $(shell for d in "$(WASM_LLVM_BIN)" "$(WASM_LLD_BIN)"; do [ -n "$$d" ] && printf "%s:" "$$d"; done)
WASM_CC      ?= $(if $(WASM_LLVM_BIN),$(WASM_LLVM_BIN)/clang,clang)
WASM_AR      ?= $(if $(WASM_LLVM_BIN),$(WASM_LLVM_BIN)/llvm-ar,llvm-ar)
WASM_TARGET  ?= wasm32-wasip1
# WASI sysroot (libc + crt1). Override with WASI_SYSROOT=/path; else probe the
# Homebrew (macOS), wasi-sdk and common Linux locations in turn.
WASI_SYSROOT ?= $(shell for d in "$$OSPREY_WASI_SYSROOT" \
  /opt/homebrew/opt/wasi-libc/share/wasi-sysroot \
  /usr/local/opt/wasi-libc/share/wasi-sysroot \
  /opt/wasi-sdk/share/wasi-sysroot "$$WASI_SDK_PATH/share/wasi-sysroot" \
  /usr/share/wasi-sysroot; do [ -n "$$d" ] && [ -d "$$d" ] && { echo "$$d"; break; }; done)
WASM_CFLAGS  ?= --target=$(WASM_TARGET) --sysroot=$(WASI_SYSROOT) -O2 -std=c11 -Wall -Wextra -Werror -c
# Portable subset that compiles for wasm32: allocator + strings + value
# containers + JSON + effects. Excludes fiber (pthreads), http/websocket
# (sockets/OpenSSL), system (fork/wait), term (termios) and ffi (dlopen).
WASM_RT_SRC  ?= memory_runtime string_runtime string_runtime_list list_runtime map_runtime map_runtime_hamt json_runtime effects_runtime
# `make wasm-serve` static-host dir + port for the in-browser example.
WASM_SERVE_DIR  ?= examples/wasm
WASM_SERVE_PORT ?= 8080

# =============================================================================
# Standard Targets
# =============================================================================

## build: C runtime archives + Rust workspace (release) + VSCode extension
build: _runtime
	@echo "==> Building..."
	cargo build --release --workspace
	cd $(EXT_DIR) && npm run compile

## test: Fail-fast tests + coverage + per-project threshold enforcement.
##       See REPO-STANDARDS-SPEC [TEST-RULES] and [COVERAGE-THRESHOLDS-JSON].
##       Projects listed in coverage-thresholds.json are each tested + checked.
test: build
	@echo "==> Testing (fail-fast + coverage + per-project thresholds)..."
	$(MAKE) _test_rust
	$(MAKE) _coverage_check_rust
	$(MAKE) _test_c_runtime
	$(MAKE) _test_differential
	$(MAKE) _test_vscode_extension
	$(MAKE) _coverage_check_vscode_extension

## lint: Run all linters/analyzers (read-only). Does NOT format.
lint: deslop
	@echo "==> Linting..."
	cargo clippy --workspace --all-targets -- -D warnings
	cd $(EXT_DIR) && npm run lint

## deslop: Code-duplication gate [CI-DESLOP]. Fails the build when measured
## duplication exceeds the ceiling in .deslop.toml (exit 3). Exclusions and the
## threshold live in that committed config — the single source of truth. When
## the `deslop` binary is absent the gate is skipped with a loud warning so a
## fresh checkout still builds; CI installs it, so the gate is enforced there.
deslop:
	@echo "==> Duplication gate (deslop)..."
	@if command -v deslop >/dev/null 2>&1; then \
		deslop . --nohtml --nojson --output $(CURDIR)/target/deslop-report --log-to-console --log-level error --no-color; \
	else \
		echo "WARNING: deslop not installed — skipping duplication gate. Install: https://deslop.live"; \
	fi

## fmt: Format all code in-place. Pass CHECK=1 for read-only check (CI use).
fmt:
	@echo "==> Formatting$(if $(CHECK), (check mode),)..."
	cargo fmt --all$(if $(CHECK), --check,)
	cd $(EXT_DIR) && npx prettier$(if $(CHECK), --check, --write) .

## clean: Remove all build artifacts
clean:
	@echo "==> Cleaning..."
	cargo clean
	$(RM) $(RTB) compiler/lib outputs lcov.info test.log
	cd $(EXT_DIR) && $(RM) out dist coverage test.log

## ci: lint + test + build (full CI simulation)
ci: lint test build

## wasm: Build everything for the WebAssembly target, ready to go — the wasm
## runtime archive (compiler/bin/libosprey_runtime_wasm.a), the hello example,
## and Osprey Data Studio in BOTH flavors (studio.{osp,ospml} -> one byte-
## identical manifest that drives the SQLite dashboard in examples/wasm/
## index.html) — then validate them and smoke-run under Node's WASI, the browser
## WASI shim, and the full golden suite. Requires clang (wasm32 backend),
## wasm-ld and a WASI sysroot —
## `brew install lld wasi-libc` (macOS) or the wasi-sdk. See
## docs/specs/0022-WebAssemblyTarget.md.
wasm: build _runtime_wasm
	@echo "==> compiling the wasm example -> examples/wasm/build/"
	@$(MKDIR) examples/wasm/build
	$(BIN) examples/wasm/hello.osp --target=wasm32 --compile -o examples/wasm/build/hello.wasm
	@echo "==> validating + smoke-running examples/wasm/build/hello.wasm"
	@command -v wasm-validate >/dev/null 2>&1 && wasm-validate examples/wasm/build/hello.wasm || echo "(wasm-validate not found — skipping structural check)"
	node scripts/wasm-smoke.mjs         examples/wasm/build/hello.wasm examples/wasm/hello.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/hello.wasm examples/wasm/hello.expectedoutput
	@echo "==> compiling Osprey Data Studio (BOTH flavors) -> examples/wasm/build/"
	$(BIN) examples/wasm/studio.osp   --target=wasm32 --compile -o examples/wasm/build/studio.osp.wasm
	$(BIN) examples/wasm/studio.ospml --target=wasm32 --compile -o examples/wasm/build/studio.ospml.wasm
	@command -v wasm-validate >/dev/null 2>&1 && wasm-validate examples/wasm/build/studio.osp.wasm && wasm-validate examples/wasm/build/studio.ospml.wasm || echo "(wasm-validate not found — skipping structural check)"
	@echo "==> both Studio flavors must emit the SAME manifest (byte-identical golden)"
	node scripts/wasm-smoke.mjs         examples/wasm/build/studio.osp.wasm   examples/wasm/studio.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/studio.osp.wasm   examples/wasm/studio.expectedoutput
	node scripts/wasm-smoke.mjs         examples/wasm/build/studio.ospml.wasm examples/wasm/studio.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/studio.ospml.wasm examples/wasm/studio.expectedoutput
	@echo "==> [wasm differential] osprey --target=wasm32 vs examples/tested..."
	@out=$$(zsh crates/diff_wasm_examples.sh); echo "$$out"; \
	  echo "$$out" | grep -Eq '(^| )FAIL=0 '  || { echo 'FAIL: wasm differential mismatch'; exit 1; }; \
	  echo "$$out" | grep -Eq '(^| )NOEXP=0 ' || { echo 'FAIL: example missing .expectedoutput'; exit 1; }
	@echo "==> wasm ready: built + validated + WASI/browser smoke + golden suite green"

wasm wasm-site _runtime_wasm: export PATH := $(WASM_PATH_PREFIX)$(PATH)

## wasm-site: Build only the WebAssembly artifacts published by the website.
##      Used by GitHub Pages before `npm run build`; does not rely on checked-in
##      wasm binaries. Requires clang, wasm-ld, a WASI sysroot, and node.
wasm-site: _runtime_wasm
	@echo "==> building osprey compiler for the website wasm demo"
	cargo build --release -p osprey-cli
	@echo "==> compiling Osprey Data Studio website assets -> examples/wasm/build/"
	@$(MKDIR) examples/wasm/build
	$(BIN) examples/wasm/studio.osp   --target=wasm32 --compile -o examples/wasm/build/studio.osp.wasm
	$(BIN) examples/wasm/studio.ospml --target=wasm32 --compile -o examples/wasm/build/studio.ospml.wasm
	@command -v wasm-validate >/dev/null 2>&1 && wasm-validate examples/wasm/build/studio.osp.wasm && wasm-validate examples/wasm/build/studio.ospml.wasm || echo "(wasm-validate not found — skipping structural check)"
	node scripts/wasm-smoke.mjs         examples/wasm/build/studio.osp.wasm   examples/wasm/studio.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/studio.osp.wasm   examples/wasm/studio.expectedoutput
	node scripts/wasm-smoke.mjs         examples/wasm/build/studio.ospml.wasm examples/wasm/studio.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/studio.ospml.wasm examples/wasm/studio.expectedoutput
	@echo "==> website wasm demo ready"

## wasm-serve: Build the wasm target (full `make wasm`), then static-host
##      $(WASM_SERVE_DIR) at http://localhost:$(WASM_SERVE_PORT)/ and open it in
##      your browser. Long-running dev server — Ctrl-C to stop. Override the port
##      with WASM_SERVE_PORT=<n>. (`make wasm` itself stays headless for CI.)
wasm-serve: wasm
	@URL="http://localhost:$(WASM_SERVE_PORT)/"; \
	  command -v python3 >/dev/null 2>&1 || { echo "FAIL: python3 not found (needed for the dev server)"; exit 1; }; \
	  echo "==> serving $(WASM_SERVE_DIR)/ at $$URL — opening browser (Ctrl-C to stop)"; \
	  OPENER=$$(command -v open || command -v xdg-open || true); \
	  if [ -n "$$OPENER" ]; then ( sleep 1; "$$OPENER" "$$URL" >/dev/null 2>&1 || true ) & \
	  else echo "  (no 'open'/'xdg-open' found — browse to $$URL manually)"; fi; \
	  cd $(WASM_SERVE_DIR) && exec python3 -m http.server $(WASM_SERVE_PORT)

## setup: Post-create dev environment setup (used by devcontainer)
setup:
	@echo "==> Setting up development environment..."
	rustup component add rustfmt clippy llvm-tools-preview
	command -v cargo-llvm-cov >/dev/null 2>&1 || cargo install cargo-llvm-cov
	cd $(EXT_DIR) && npm ci
	cd webcompiler && npm ci
	cd website && npm ci
	@echo "==> Setup complete. Run 'make ci' to validate."

# ---------------------------------------------------------------------------
# Internal helpers — NOT public targets, NOT in .PHONY
# ---------------------------------------------------------------------------

# Build the pure-C runtime archives osprey links at `--run` time. One shell
# so `cd` persists; faithful port of the original hardened C recipes.
_runtime:
	@echo "==> building C runtime archives ($(RTB)/lib*_runtime.a)"
	@cd compiler && set -e && $(MKDIR) bin lib bin/gc && \
	  $(CC) $(B) runtime/memory_runtime.c       -o bin/memory_runtime.o && \
	  $(CC) $(B) runtime/memory_gc.c            -o bin/memory_gc.o && \
	  $(CC) $(B) -include runtime/osp_gc_shim.h runtime/list_runtime.c     -o bin/gc/list_runtime.o && \
	  $(CC) $(B) -include runtime/osp_gc_shim.h runtime/map_runtime.c      -o bin/gc/map_runtime.o && \
	  $(CC) $(B) -include runtime/osp_gc_shim.h runtime/map_runtime_hamt.c -o bin/gc/map_runtime_hamt.o && \
	  $(CC) -c -fPIC -O2 -Werror -Wall -Wextra -Wpedantic -std=c11 -D_GNU_SOURCE runtime/fiber_runtime.c -o bin/fiber_runtime.o && \
	  $(CC) $(A) runtime/system_runtime.c       -o bin/system_runtime.o && \
	  $(CC) $(A) runtime/effects_runtime.c      -o bin/effects_runtime.o && \
	  $(CC) $(A) runtime/string_runtime.c       -o bin/string_runtime.o && \
	  $(CC) $(A) runtime/string_runtime_list.c  -o bin/string_runtime_list.o && \
	  $(CC) $(B) runtime/list_runtime.c         -o bin/list_runtime.o && \
	  $(CC) $(B) runtime/map_runtime.c          -o bin/map_runtime.o && \
	  $(CC) $(B) runtime/map_runtime_hamt.c     -o bin/map_runtime_hamt.o && \
	  $(CC) $(B) runtime/json_runtime.c         -o bin/json_runtime.o && \
	  $(CC) $(B) runtime/ffi_runtime.c          -o bin/ffi_runtime.o && \
	  $(CC) $(B) runtime/term_runtime.c         -o bin/term_runtime.o && \
	  $(CC) $(B) runtime/random_runtime.c       -o bin/random_runtime.o && \
	  $(CC) -c -fPIC -O2 -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Werror -Wall -Wextra \
	        -Wformat -Werror=format-security -Werror=implicit-function-declaration \
	        -Werror=incompatible-pointer-types -Werror=int-conversion -Warray-bounds -ftrapv \
	        -fno-delete-null-pointer-checks -fno-strict-overflow -fno-strict-aliasing -fPIE \
	        -DWITH_OPENSSL $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` \
	        runtime/http_shared.c -o bin/http_shared.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_client_runtime.c      -o bin/http_client_runtime.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_server_runtime.c      -o bin/http_server_runtime.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/websocket_client_runtime.c -o bin/websocket_client_runtime.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/websocket_server_runtime.c -o bin/websocket_server_runtime.o && \
	  $(AR) rcs bin/libfiber_runtime.a $(FIB_OBJ) && \
	  $(AR) rcs bin/libhttp_runtime.a  $(HTTP_OBJ) && \
	  $(AR) rcs bin/libfiber_runtime_gc.a $(FIB_OBJ_GC) && \
	  $(AR) rcs bin/libhttp_runtime_gc.a  $(HTTP_OBJ_GC) && \
	  cp bin/libfiber_runtime.a bin/libhttp_runtime.a bin/libfiber_runtime_gc.a bin/libhttp_runtime_gc.a lib/

# Cross-compile the portable C-runtime subset to a wasm32-wasip1 archive that
# osprey links for `--target=wasm32`. One shell so `cd` persists. Fails loudly
# if no WASI sysroot is found. Output: compiler/{bin,lib}/libosprey_runtime_wasm.a
_runtime_wasm:
	@if [ -z "$(WASI_SYSROOT)" ]; then \
	  echo "ERROR: no WASI sysroot found. Install it with 'brew install lld wasi-libc'"; \
	  echo "       (macOS) or the wasi-sdk, or set WASI_SYSROOT=/path/to/wasi-sysroot."; \
	  exit 1; fi
	@echo "==> building wasm runtime archive ($(WASM_TARGET), sysroot $(WASI_SYSROOT))"
	@cd compiler && set -e && $(MKDIR) bin/wasm lib && \
	  for u in $(WASM_RT_SRC); do \
	    $(WASM_CC) $(WASM_CFLAGS) runtime/$$u.c -o bin/wasm/$$u.o; \
	  done && \
	  $(WASM_AR) rcs bin/libosprey_runtime_wasm.a bin/wasm/*.o && \
	  cp bin/libosprey_runtime_wasm.a lib/

# --- rust (crates/) ---------------------------------------------------------
# Implements [TEST-RULES] — cargo test is fail-fast at the binary level by
# default (a failing test binary aborts the run); coverage via cargo-llvm-cov.
# `--profile ci` is the workspace's fast-compile profile (see root Cargo.toml).
_test_rust:
	@echo "==> [rust] running tests with coverage..."
	set -o pipefail && cargo llvm-cov --workspace --profile ci --lcov --output-path lcov.info 2>&1 | tee test.log

# Per-crate enforcement ([COVERAGE-THRESHOLDS-JSON]): every rust crate is gated
# independently against its own threshold (floor 95% + monotonic ratchet). lcov
# SF records are grouped by their crates/<name>/ path; a single crate below its
# gate fails the whole target. Aggregating the workspace into one number would
# let a well-covered crate mask an under-tested one — exactly what the ratchet
# exists to prevent.
_coverage_check_rust:
	@if [ ! -f "$(COVERAGE_THRESHOLDS_FILE)" ]; then echo "FAIL: $(COVERAGE_THRESHOLDS_FILE) not found"; exit 1; fi; \
	if [ ! -f lcov.info ]; then echo "[rust] FAIL: lcov.info not produced"; exit 1; fi; \
	fail=0; \
	for crate in $$(jq -r '.projects | to_entries[] | select(.value.language=="rust") | .key' "$(COVERAGE_THRESHOLDS_FILE)"); do \
	  threshold=$$(jq -r --arg c "$$crate" '.projects[$$c].threshold' "$(COVERAGE_THRESHOLDS_FILE)"); \
	  set -- $$(awk -F: -v c="$$crate" 'index($$0,"SF:")==1{in_c=index($$2,"/crates/" c "/")>0} in_c&&/^LH:/{h+=$$2} in_c&&/^LF:/{f+=$$2} END{printf "%d %d",h+0,f+0}' lcov.info); \
	  lh=$$1; lf=$$2; \
	  if [ "$$lf" -eq 0 ]; then echo "[rust] $$crate FAIL: no lines found in lcov.info"; fail=1; continue; fi; \
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
# binary). Covers the string cursor (BUILTIN-STRING-CURSOR) + the error-message
# contract ([ERR-PAYLOAD]) exhaustively, under the same hardening flags the
# archives use. Built as an executable (no `-c`), so it links the runtime TUs
# directly. Runs on the `make test` (ubuntu) job; Windows CI uses its own steps.
_test_c_runtime:
	@echo "==> [c-runtime] string cursor + error-message contract tests..."
	@cd compiler && $(CC) -O2 -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Werror -Wall -Wextra \
	  -ftrapv -std=c11 -D_GNU_SOURCE \
	  runtime/string_runtime_tests.c runtime/string_runtime.c runtime/string_runtime_list.c \
	  -o bin/string_runtime_tests && ./bin/string_runtime_tests

# Differential golden harness: every examples/tested/*.osp run through
# `osprey --run` must match its .expectedoutput byte-for-byte, and the
# must-reject suite (examples/failscompilation) must stay within the
# FC_EXPECTED_ESCAPES ratchet declared in the harness.
_test_differential:
	@echo "==> [differential] osprey --run vs .expectedoutput..."
	@out=$$(zsh crates/diff_examples.sh); echo "$$out"; \
	  echo "$$out" | grep -Eq 'FAIL=0 '  || { echo 'FAIL: differential mismatch'; exit 1; }; \
	  echo "$$out" | grep -Eq 'NOEXP=0 ' || { echo 'FAIL: example missing .expectedoutput'; exit 1; }; \
	  echo "$$out" | grep -q  'FC_OK'    || { echo 'FAIL: must-reject ratchet exceeded'; exit 1; }

# _conformance-gc: run every tested example under the tracing GC backend; output
# must be byte-identical to the default ([MEM-BACKENDS] oracle, docs/plans/0011).
_conformance-gc: build
	@echo "==> [conformance] differential harness under --memory=gc..."
	@out=$$(OSPREY_RUN_FLAGS=--memory=gc zsh crates/diff_examples.sh); echo "$$out"; \
	  echo "$$out" | grep -Eq 'FAIL=0 ' || { echo 'FAIL: GC backend output diverged'; exit 1; }

# --- vscode-extension -------------------------------------------------------
# The extension's LSP server spawns the `osprey` binary at runtime, so the
# integration tests need a real compiler on PATH: the Rust binary is staged as
# `osprey`. vscode-test runs with V8 coverage; c8 merges the profiles into
# coverage/coverage-summary.json.
_test_vscode_extension:
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

_coverage_check_vscode_extension:
	@if [ ! -f "$(COVERAGE_THRESHOLDS_FILE)" ]; then echo "FAIL: $(COVERAGE_THRESHOLDS_FILE) not found"; exit 1; fi; \
	THRESHOLD=$$(jq -r '.projects["vscode-extension"].threshold' "$(COVERAGE_THRESHOLDS_FILE)"); \
	if [ ! -f "$(EXT_DIR)/coverage/coverage-summary.json" ]; then \
	  echo "[vscode-extension] FAIL: coverage-summary.json not produced"; exit 1; \
	fi; \
	PCT=$$(jq -r '.total.lines.pct' "$(EXT_DIR)/coverage/coverage-summary.json"); \
	PCT_INT=$$(echo "$$PCT" | awk '{printf "%d", $$1}'); \
	echo "[vscode-extension] coverage: $${PCT}% (threshold: $${THRESHOLD}%)"; \
	if [ "$$PCT_INT" -lt "$$THRESHOLD" ]; then echo "[vscode-extension] FAIL: $${PCT}% < $${THRESHOLD}%"; exit 1; fi; \
	echo "[vscode-extension] OK: $${PCT}% >= $${THRESHOLD}%"

# =============================================================================
# Repo-Specific Targets
# =============================================================================

# _tui: Build, then launch the interactive TUI demo (live GitHub API browser).
#       Runs in the current terminal so the raw-mode key reader gets real stdin.
_tui: build
	@echo "==> launching TUI demo (live GitHub API browser)"
	./$(BIN) examples/tui/api_browser.osp --run

## run: Compile and run an Osprey file (usage: make run FILE=<path>)
run: build
	@if [ -z "$(FILE)" ]; then echo "Usage: make run FILE=<path>"; exit 1; fi
	./$(BIN) $(FILE) --run

## install: Install osprey + runtime archives system-wide
install: build
	cargo install --path crates/osprey-cli --force
	sudo $(MKDIR) /usr/local/lib
	sudo cp $(RTB)/libfiber_runtime.a $(RTB)/libhttp_runtime.a /usr/local/lib/
	@echo "==> installed osprey and runtime archives."

# _uninstall: Remove osprey + runtime archives from the system
_uninstall:
	cargo uninstall osprey-cli 2>/dev/null || true
	sudo rm -f /usr/local/lib/libfiber_runtime.a /usr/local/lib/libhttp_runtime.a
	@echo "==> uninstalled."

# _website-dev: Start local website development server
_website-dev:
	cd website && npm run dev

# _website-build: Build static site
_website-build:
	cd website && npm run build

## bench: Build, then run the cross-language performance benchmark suite
##        (Osprey vs Rust/C/OCaml/Haskell — CPU via hyperfine, peak memory via
##        /usr/bin/time). Absent toolchains are skipped. Informational only:
##        NOT part of `make ci`/`make test` (perf is noisy on shared runners).
##        Pass a name filter via BENCH_FILTER=<substr>; results in
##        benchmarks/results/results.md. See benchmarks/README.md.
bench: build
	@zsh benchmarks/run.sh $(BENCH_FILTER)

## vsix-rebuild-reinstall: Clean → build → reinstall the Osprey VSCode
##      extension in place, bundling the freshly-built Rust compiler as `osprey`.
##      Touches ONLY the Osprey extension ($(EXT_ID)) in the DEFAULT profile —
##      never another extension, never another VSCode profile. macOS only.
##      ONE `code` invocation (install --force, no separate uninstall) so the
##      running VSCode reconciles its extension host exactly once, not twice.
##      See [MAKE-IDE-EXT].
vsix-rebuild-reinstall: _vsix_clean build _vsix_build _vsix_bundle _vsix_package _vsix_install

# _rebuild-install-vsix: deprecated private alias of `vsix-rebuild-reinstall`.
_rebuild-install-vsix: vsix-rebuild-reinstall

# --- vsix sub-steps ---------------------------------------------------------
_vsix_clean:
	cd $(EXT_DIR) && $(RM) out dist osprey-*.vsix

_vsix_build:
	cd $(EXT_DIR) && npm run compile

# Stage the freshly-built Rust binary AND the C runtime archives where the
# extension expects its bundled compiler (bin/<os>-<arch>/), so the VSIX runs
# against THIS build. The compiler locates its runtime archives next to its own
# executable (find_runtime_lib in osprey-cli), so libfiber_runtime.a /
# libhttp_runtime.a must sit beside the bundled `osprey` for `--run` to link.
_vsix_bundle:
	@OS=$$(uname -s | tr '[:upper:]' '[:lower:]'); \
	case "$$OS" in darwin) OS=darwin;; linux) OS=linux;; *) OS=win32;; esac; \
	ARCH=$$(uname -m); case "$$ARCH" in arm64|aarch64) ARCH=arm64;; *) ARCH=x64;; esac; \
	DEST="$(EXT_DIR)/bin/$$OS-$$ARCH"; $(MKDIR) "$$DEST"; \
	cp $(BIN) "$$DEST/osprey"; \
	cp $(RTB)/libfiber_runtime.a $(RTB)/libhttp_runtime.a "$$DEST/"; \
	echo "  bundled $(BIN) + libfiber_runtime.a + libhttp_runtime.a -> $$DEST/"

_vsix_package:
	cd $(EXT_DIR) && npm run package

# Install the newest Osprey VSIX into the DEFAULT profile only. `--install-
# extension <file> --force` upgrades that one extension id in place — no
# separate uninstall needed, so the live VSCode reconciles its extension host
# once. It installs exactly that VSIX (the Osprey extension) and no other, and
# does NOT enumerate VSCode profiles, so it can never touch any other extension.
_vsix_install:
	@VSIX=$$(ls -t $(EXT_DIR)/osprey-*.vsix 2>/dev/null | head -1); \
	if [ -z "$$VSIX" ]; then echo "FAIL: no osprey-*.vsix in $(EXT_DIR)/"; exit 1; fi; \
	echo "  vsix: $$VSIX"; \
	code --install-extension "$$VSIX" --force && echo "  installed $(EXT_ID)"
