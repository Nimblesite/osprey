# agent-pmo:b636503
# =============================================================================
# Standard Makefile — osprey
# Cross-platform: Linux, macOS, Windows (via GNU Make)
# Primary language: Rust (crates/ workspace → the osprey compiler), with a
# pure-C runtime (compiler/runtime → lib*_runtime.a, linked by `osprey
# --run`) and TypeScript sub-projects (vscode-extension, webcompiler, website).
# =============================================================================

.PHONY: build test language-test lint fmt clean ci setup run install bench partial-bench wasm wasm-site wasm-serve vsix-rebuild-reinstall bank bank-web bank-test bank-e2e hawk gpu-demo graphics graphics-shader _test_gc_stack_root \
	_test_c_runtime _coverage_check_c_runtime \
	_rebuild-install-vsix _vsix_clean _vsix_build _vsix_bundle _vsix_package _vsix_install

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

# Shared hardened warning sets — defined ONCE so a lint added here reaches every
# C recipe below (archives, fiber, http_shared, unit tests). WARN is the core
# every C translation unit compiles under. WARN_MAX adds two lints the SHIPPED
# archive objects fully satisfy but a few in-tree test programs intentionally
# trip (string literals passed into char* buffers under test; K&R decls), so it
# applies to the archives only; the unit-test profile uses WARN.
# EVERY flag here MUST be understood by BOTH clang (macOS/dev) and gcc (Linux
# CI + the web-compiler Docker builder). clang-only spellings such as
# -Wmissing-variable-declarations break `make _runtime` under gcc — keep the set
# to the portable intersection.
WARN     ?= -Werror -Wall -Wextra -Wshadow -Wpointer-arith -Wvla -Wundef -Wredundant-decls -Wcast-qual -Wcast-align -Wold-style-definition -Wbad-function-cast
WARN_MAX ?= $(WARN) -Wstrict-prototypes -Wwrite-strings
# C runtime compile flag profiles (hardened; mirror the original recipes).
A    ?= -c -fPIC -O2 -D_FORTIFY_SOURCE=2 -fstack-protector-strong $(WARN_MAX) -ftrapv -fPIE -D_GNU_SOURCE
B    ?= $(A) -std=c11
OSSL ?= -DOPENSSL_SUPPRESS_DEPRECATED -DOPENSSL_API_COMPAT=30000 -Wno-deprecated-declarations
# C unit-test flag profile: the archive hardening core, but linking an
# executable (no -c/-fPIC/-fPIE). Named once so _test_c_runtime does not repeat
# the flag list per suite.
T    ?= -O2 -D_FORTIFY_SOURCE=2 -fstack-protector-strong $(WARN) -ftrapv -std=c11 -D_GNU_SOURCE
# Object lists for the archives (paths relative to compiler/, where `ar` runs).
FIB_OBJ  ?= bin/memory_runtime.o bin/gpu_runtime.o bin/fiber_runtime.o bin/system_runtime.o bin/file_runtime.o bin/effects_runtime.o bin/effects_coro.o bin/string_runtime.o bin/string_runtime_list.o bin/list_runtime.o bin/map_runtime.o bin/map_runtime_hamt.o bin/json_runtime.o bin/ffi_runtime.o bin/term_runtime.o bin/random_runtime.o bin/test_runtime.o bin/coverage_runtime.o bin/profiler_runtime.o bin/profiler_sampler.o
HTTP_OBJ ?= bin/http_shared.o bin/http_client_runtime.o bin/http_server_request.o bin/http_server_response.o bin/http_server_runtime.o bin/websocket_client_runtime.o bin/websocket_server_runtime.o $(FIB_OBJ)
# GC backend archives (osprey --memory=gc): the tracing collector replaces
# memory_runtime.o, and the value-container units are rebuilt with the malloc
# redirect (osp_gc_shim.h) so their nodes live in the managed heap. Everything
# else is the same object. Implements [GC-TRACE-CONSERVATIVE], spec 0018.
FIB_OBJ_GC  ?= bin/memory_gc.o bin/gpu_runtime.o bin/fiber_runtime.o bin/system_runtime.o bin/file_runtime.o bin/effects_runtime.o bin/effects_coro.o bin/string_runtime.o bin/string_runtime_list.o bin/gc/list_runtime.o bin/gc/map_runtime.o bin/gc/map_runtime_hamt.o bin/json_runtime.o bin/ffi_runtime.o bin/term_runtime.o bin/random_runtime.o bin/test_runtime.o bin/coverage_runtime.o bin/profiler_runtime.o bin/profiler_sampler.o
HTTP_OBJ_GC ?= bin/http_shared.o bin/http_client_runtime.o bin/http_server_request.o bin/http_server_response.o bin/http_server_runtime.o bin/websocket_client_runtime.o bin/websocket_server_runtime.o $(FIB_OBJ_GC)
# ARC backend archives (osprey --memory=arc): Perceus reference counting
# replaces memory_runtime.o, and the value-producing units (containers +
# strings + JSON) are rebuilt with the allocation redirect (osp_arc_shim.h) so
# their nodes/buffers carry the 16-byte header and registry entry. Implements
# [GC-ARC-PERCEUS], spec 0018.
FIB_OBJ_ARC  ?= bin/memory_arc.o bin/gpu_runtime.o bin/fiber_runtime.o bin/system_runtime.o bin/arc/file_runtime.o bin/effects_runtime.o bin/effects_coro.o bin/arc/string_runtime.o bin/arc/string_runtime_list.o bin/arc/list_runtime.o bin/arc/map_runtime.o bin/arc/map_runtime_hamt.o bin/arc/json_runtime.o bin/ffi_runtime.o bin/term_runtime.o bin/random_runtime.o bin/test_runtime.o bin/coverage_runtime.o bin/profiler_runtime.o bin/profiler_sampler.o
HTTP_OBJ_ARC ?= bin/http_shared.o bin/http_client_runtime.o bin/http_server_request.o bin/http_server_response.o bin/http_server_runtime.o bin/websocket_client_runtime.o bin/websocket_server_runtime.o $(FIB_OBJ_ARC)
NATIVE_RUNTIME_CONFIG ?= compiler/bin/.native-runtime-config
NATIVE_RUNTIME_STAMP ?= compiler/bin/.native-runtime.stamp
NATIVE_RUNTIME_INPUTS ?= $(filter-out compiler/runtime/%_tests.c compiler/runtime/test_http_length_validation.c compiler/runtime/test_openssl.c compiler/runtime/test_system_runtime.c compiler/runtime/web_runtime.c,$(wildcard compiler/runtime/*.c)) $(wildcard compiler/runtime/*.h)
NATIVE_RUNTIME_ARCHIVES ?= compiler/bin/libfiber_runtime.a compiler/bin/libhttp_runtime.a compiler/bin/libfiber_runtime_gc.a compiler/bin/libhttp_runtime_gc.a compiler/bin/libfiber_runtime_arc.a compiler/bin/libhttp_runtime_arc.a compiler/lib/libfiber_runtime.a compiler/lib/libhttp_runtime.a compiler/lib/libfiber_runtime_gc.a compiler/lib/libhttp_runtime_gc.a compiler/lib/libfiber_runtime_arc.a compiler/lib/libhttp_runtime_arc.a

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

# wasm_validate: structural check of the modules named in $(1). wasm-validate
# ships with wabt and is genuinely optional, so an ABSENT binary skips loudly —
# but a check that RUNS and FAILS must fail the build. Every call site used to
# spell this as `command -v wasm-validate && wasm-validate a && wasm-validate b
# || echo "(not found — skipping)"`, where the trailing `||` catches BOTH arms:
# a module that failed validation reported itself as a missing tool and the
# build went green. Defined once so the three call sites cannot drift.
define wasm_validate
	@if command -v wasm-validate >/dev/null 2>&1; then \
		for module in $(1); do wasm-validate "$$module" || exit 1; done; \
	else \
		echo "(wasm-validate not found — structural check skipped; install wabt)"; \
	fi
endef
# Portable subset that compiles for wasm32: allocator + strings + value
# containers + JSON + effects + the browser host bridge. Excludes fiber
# (pthreads), http/websocket (sockets/OpenSSL), system (fork/wait), term
# (termios) and ffi (dlopen).
# profiler_runtime compiles to inert stubs on wasm32 (no pthreads/signals) but
# must be present: codegen anchors `osp_prof_boot` into every main [PROF-ACTIVATE-ENV].
# system_runtime and random_runtime are here for their PORTABLE halves: file
# I/O + JSON/string helpers, and the OS CSPRNG (wasi-libc's arc4random_buf over
# the WASI random_get host call). Each compiles its non-portable half out under
# `#ifndef __wasm__`, so adding them unskips file and random programs on wasm32
# without pretending fork/exec or pthreads exist.
WASM_RT_SRC  ?= memory_runtime gpu_runtime string_runtime string_runtime_list list_runtime map_runtime map_runtime_hamt json_runtime effects_runtime test_runtime coverage_runtime web_runtime profiler_runtime wasm_builtins_runtime system_runtime file_runtime random_runtime
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
##       Projects listed in coverage-thresholds.json are each tested + checked.
test: build
	@echo "==> Testing (fail-fast + coverage + per-project thresholds)..."
	$(MAKE) _test_runtime_incremental
	$(MAKE) _test_rust
	$(MAKE) _coverage_check_rust
	$(MAKE) _test_c_runtime
	$(MAKE) _coverage_check_c_runtime
	$(MAKE) _test_language_corpus
	$(MAKE) _test_goldens
	$(MAKE) _conformance-gc
	$(MAKE) _conformance-arc
	$(MAKE) _test_profiler
	$(MAKE) _test_vscode_extension
	$(MAKE) _coverage_check_vscode_extension

## bank: Rebuild and run the Talon Bank showcase for manual testing.
##       Opens http://127.0.0.1:18790 (dashboard) / /api/accounts (JSON API).
##       The hold marker keeps the server up; Ctrl-C removes it and exits.
bank: bank-web
	@echo "==> Talon Bank live on http://127.0.0.1:18790  (Ctrl-C to stop)"
	@touch /tmp/talon_bank.hold
	@trap 'rm -f /tmp/talon_bank.hold' EXIT INT TERM; \
	  (if command -v open >/dev/null && command -v curl >/dev/null; then \
	    attempts=0; \
	    until accounts="$$(curl -fsS http://127.0.0.1:18790/api/accounts 2>/dev/null)" && [[ "$$accounts" == *'"Priya Sharma"'* ]]; do \
	      attempts=$$((attempts + 1)); [ "$$attempts" -ge 200 ] && exit 0; sleep 0.1; \
	    done; \
	    open http://127.0.0.1:18790; \
	  fi) & \
	  ./$(BIN) examples/projects/modules --run

## bank-web: Regenerate the embedded React host + Osprey WebAssembly client.
##           Requires Node and a WASI sysroot; the generated Osprey Bundle is
##           committed so ordinary native/CI builds do not need either tool.
bank-web: build _runtime_wasm
	@echo "==> Building Talon Bank browser application..."
	cd examples/projects/modules/web && npm ci && npm run build

## bank-test: Native Osprey unit tests for the Talon Bank pure domain layer,
##            run through the built-in `osprey test` harness (TAP output).
bank-test: build
	@echo "==> Bank native tests (osprey test)..."
	./$(BIN) test examples/projects/modules/test

## language-test: Run the assertion-driven Default + ML core language corpus.
language-test: build
	$(MAKE) _test_language_corpus

## bank-e2e: Browser end-to-end tests for the Talon Bank modules showcase
##           (examples/projects/modules) — real Chromium via Playwright drives
##           the compiled osprey binary serving its HTTP API and web UI.
bank-e2e: bank-web
	@echo "==> Bank e2e (Playwright)..."
	cd examples/projects/modules/e2e && npm ci && npx playwright install chromium && npx playwright test

## lint: Run all linters/analyzers (read-only). Checks formatting but does
## NOT rewrite it — `make fmt` does that. The fmt check lives HERE because
## `lint` is what the required CI job runs; a format gate only in an optional
## job cannot block a merge.
lint: deslop _lint

_lint:
	@echo "==> Linting..."
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cd $(EXT_DIR) && npm run lint

## deslop: Code-duplication gate. Fails the build when measured
## duplication exceeds the ceiling in .deslop.toml (exit 3). Exclusions and the
## threshold live in that committed config — the single source of truth. When
## the `deslop` binary is absent the gate is skipped with a loud warning so a
## fresh checkout still builds; CI enforces the gate through the official action.
deslop:
	@echo "==> Duplication gate (deslop)..."
	@if ! command -v deslop >/dev/null 2>&1; then \
		echo "FAIL: deslop is not installed, so the duplication gate cannot run."; \
		echo "      A gate that cannot run must not report success — this used to"; \
		echo "      print a warning and exit 0, which made every local 'make ci'"; \
		echo "      green with the ceiling in .deslop.toml unchecked."; \
		echo "      Install: https://deslop.live   (CI uses the official action.)"; \
		exit 1; \
	fi
	deslop . --nohtml --nojson --output $(CURDIR)/target/deslop-report --log-to-console --log-level error --no-color

## hawk: Dead-code gate (astral-sh/hawk). Fails the build when any `pub`
## declaration is unreachable from the osprey binary (hawk::dead_public). Scoped
## to dead_public ONLY — unnecessary_public / restricted-visibility findings are
## over-exposure, not dead code, and several are irreducibly public for the
## integration tests under this workspace's `dead_code = "deny"` policy, so they
## must NOT fail the gate. hawk needs rustc_private (RUSTC_BOOTSTRAP=1) and its
## prebuilt driver is pinned to the workspace toolchain (1.97.1) — bump the
## installer and the CI toolchain together. An absent cargo-hawk FAILS this
## target: a gate that cannot run must not report success.
## Install: https://github.com/astral-sh/hawk
hawk:
	@echo "==> Dead-code gate (hawk)..."
	@if ! command -v cargo-hawk >/dev/null 2>&1; then \
		echo "FAIL: cargo-hawk is not installed, so the dead-code gate cannot run."; \
		echo "      A gate that cannot run must not report success — this used to"; \
		echo "      print a warning and exit 0, so 'make hawk' was green on every"; \
		echo "      machine without the binary, the CI runner included if its"; \
		echo "      install step ever stopped landing cargo-hawk on PATH."; \
		echo "      Install: https://github.com/astral-sh/hawk"; \
		exit 1; \
	fi
	RUSTC_BOOTSTRAP=1 cargo hawk check --only dead-public -D hawk::dead_public --target-dir $(CURDIR)/target/hawk

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

## ci: lint + hawk + test + bank-test + bank-e2e + build (full CI simulation)
ci: lint hawk test bank-test bank-e2e build

## wasm: Build everything for the WebAssembly target, ready to go — the wasm
## runtime archive (compiler/bin/libosprey_runtime_wasm.a), the hello example,
## and Osprey Data Studio in BOTH flavors (studio.{osp,ospml} -> one byte-
## identical manifest that drives the SQLite dashboard in examples/wasm/
## index.html) — then validate them and smoke-run under Node's WASI, the browser
## WASI shim, with committed expected output for hello and both Studio flavors.
## Requires clang (wasm32 backend),
## wasm-ld and a WASI sysroot —
## `brew install lld wasi-libc` (macOS) or the wasi-sdk. See
## docs/specs/0022-WebAssemblyTarget.md.
wasm: build _runtime_wasm
	@echo "==> compiling the wasm example -> examples/wasm/build/"
	@$(MKDIR) examples/wasm/build
	$(BIN) examples/wasm/hello.osp --target=wasm32 --compile -o examples/wasm/build/hello.wasm
	@echo "==> validating + smoke-running examples/wasm/build/hello.wasm"
	$(call wasm_validate,examples/wasm/build/hello.wasm)
	node scripts/wasm-smoke.mjs         examples/wasm/build/hello.wasm examples/wasm/hello.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/hello.wasm examples/wasm/hello.expectedoutput
	@echo "==> compiling Osprey Data Studio (BOTH flavors) -> examples/wasm/build/"
	$(BIN) examples/wasm/studio.osp   --target=wasm32 --compile -o examples/wasm/build/studio.osp.wasm
	$(BIN) examples/wasm/studio.ospml --target=wasm32 --compile -o examples/wasm/build/studio.ospml.wasm
	$(call wasm_validate,examples/wasm/build/studio.osp.wasm examples/wasm/build/studio.ospml.wasm)
	@echo "==> both Studio flavors must emit the SAME manifest (byte-identical golden)"
	node scripts/wasm-smoke.mjs         examples/wasm/build/studio.osp.wasm   examples/wasm/studio.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/studio.osp.wasm   examples/wasm/studio.expectedoutput
	node scripts/wasm-smoke.mjs         examples/wasm/build/studio.ospml.wasm examples/wasm/studio.expectedoutput
	node scripts/wasm-browser-smoke.mjs examples/wasm/build/studio.ospml.wasm examples/wasm/studio.expectedoutput
	$(MAKE) _test_wasm_goldens
	@echo "==> wasm ready: built + validated + WASI/browser smoke green"

wasm wasm-site _runtime_wasm bank-web _test_wasm_goldens: export PATH := $(WASM_PATH_PREFIX)$(PATH)

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
	$(call wasm_validate,examples/wasm/build/studio.osp.wasm examples/wasm/build/studio.ospml.wasm)
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
_test_runtime_incremental: _runtime
	@runtime_mtime() { stat -c %Y "$$1" 2>/dev/null || stat -f %m "$$1"; }; \
	  before="$$(runtime_mtime $(RTB)/libfiber_runtime.a)"; \
	  $(MAKE) _runtime >/dev/null; \
	  after="$$(runtime_mtime $(RTB)/libfiber_runtime.a)"; \
	  if [ "$$before" != "$$after" ]; then \
	    echo "ERROR: unchanged native runtime rebuilt and invalidated the test cache"; \
	    exit 1; \
	  fi

_runtime:
	@$(MKDIR) $(RTB)
	@set -e; config_tmp="$(NATIVE_RUNTIME_CONFIG).$$$$"; \
	  { printf '%s\n' \
	      "CC=$(CC)" "AR=$(AR)" "A=$(A)" "B=$(B)" "WARN_MAX=$(WARN_MAX)" \
	      "OSSL=$(OSSL)" "FIB_OBJ=$(FIB_OBJ)" "HTTP_OBJ=$(HTTP_OBJ)" \
	      "FIB_OBJ_GC=$(FIB_OBJ_GC)" "HTTP_OBJ_GC=$(HTTP_OBJ_GC)" \
	      "FIB_OBJ_ARC=$(FIB_OBJ_ARC)" "HTTP_OBJ_ARC=$(HTTP_OBJ_ARC)"; \
	    command -v "$(firstword $(CC))" 2>/dev/null || true; \
	    $(CC) --version 2>&1 | sed -n '1p' || true; \
	    command -v "$(firstword $(AR))" 2>/dev/null || true; \
	    $(AR) --version 2>&1 | sed -n '1p' || true; \
	    pkg-config --cflags openssl 2>/dev/null || true; \
	    pkg-config --modversion openssl 2>/dev/null || true; \
	  } >"$$config_tmp"; \
	  if cmp -s "$$config_tmp" "$(NATIVE_RUNTIME_CONFIG)"; then \
	    rm -f "$$config_tmp"; \
	  else \
	    mv "$$config_tmp" "$(NATIVE_RUNTIME_CONFIG)"; \
	  fi; \
	  for archive in $(NATIVE_RUNTIME_ARCHIVES); do \
	    if [ ! -s "$$archive" ]; then rm -f "$(NATIVE_RUNTIME_STAMP)"; break; fi; \
	  done
	@$(MAKE) --no-print-directory -s $(NATIVE_RUNTIME_STAMP)

$(NATIVE_RUNTIME_STAMP): $(NATIVE_RUNTIME_INPUTS) $(NATIVE_RUNTIME_CONFIG) Makefile
	@echo "==> building C runtime archives ($(RTB)/lib*_runtime.a)"
	@cd compiler && set -e && $(MKDIR) bin lib bin/gc bin/arc && \
	  $(CC) $(B) runtime/memory_runtime.c       -o bin/memory_runtime.o && \
	  $(CC) $(B) runtime/memory_gc.c            -o bin/memory_gc.o && \
	  $(CC) $(B) runtime/memory_arc.c           -o bin/memory_arc.o && \
	  $(CC) $(B) -include runtime/osp_gc_shim.h runtime/list_runtime.c     -o bin/gc/list_runtime.o && \
	  $(CC) $(B) -include runtime/osp_gc_shim.h runtime/map_runtime.c      -o bin/gc/map_runtime.o && \
	  $(CC) $(B) -include runtime/osp_gc_shim.h runtime/map_runtime_hamt.c -o bin/gc/map_runtime_hamt.o && \
	  $(CC) $(B) -include runtime/osp_arc_shim.h runtime/list_runtime.c        -o bin/arc/list_runtime.o && \
	  $(CC) $(B) -include runtime/osp_arc_shim.h runtime/map_runtime.c         -o bin/arc/map_runtime.o && \
	  $(CC) $(B) -include runtime/osp_arc_shim.h runtime/map_runtime_hamt.c    -o bin/arc/map_runtime_hamt.o && \
	  $(CC) $(A) -include runtime/osp_arc_shim.h runtime/string_runtime.c      -o bin/arc/string_runtime.o && \
	  $(CC) $(A) -include runtime/osp_arc_shim.h runtime/string_runtime_list.c -o bin/arc/string_runtime_list.o && \
	  $(CC) $(B) -include runtime/osp_arc_shim.h runtime/json_runtime.c        -o bin/arc/json_runtime.o && \
	  $(CC) $(A) -include runtime/osp_arc_shim.h runtime/file_runtime.c        -o bin/arc/file_runtime.o && \
	  $(CC) -c -fPIC -O2 $(WARN_MAX) -Wpedantic -std=c11 -D_GNU_SOURCE runtime/fiber_runtime.c -o bin/fiber_runtime.o && \
	  $(CC) $(A) runtime/system_runtime.c       -o bin/system_runtime.o && \
	  $(CC) $(A) runtime/file_runtime.c         -o bin/file_runtime.o && \
	  $(CC) $(A) runtime/effects_runtime.c      -o bin/effects_runtime.o && \
	  $(CC) $(A) runtime/effects_coro.c         -o bin/effects_coro.o && \
	  $(CC) $(A) runtime/string_runtime.c       -o bin/string_runtime.o && \
	  $(CC) $(A) runtime/string_runtime_list.c  -o bin/string_runtime_list.o && \
	  $(CC) $(B) runtime/list_runtime.c         -o bin/list_runtime.o && \
	  $(CC) $(B) runtime/map_runtime.c          -o bin/map_runtime.o && \
	  $(CC) $(B) runtime/map_runtime_hamt.c     -o bin/map_runtime_hamt.o && \
	  $(CC) $(B) runtime/json_runtime.c         -o bin/json_runtime.o && \
	  $(CC) $(B) runtime/ffi_runtime.c          -o bin/ffi_runtime.o && \
	  $(CC) $(B) runtime/term_runtime.c         -o bin/term_runtime.o && \
	  $(CC) $(B) runtime/random_runtime.c       -o bin/random_runtime.o && \
	  $(CC) $(B) runtime/gpu_runtime.c          -o bin/gpu_runtime.o && \
	  $(CC) $(B) runtime/test_runtime.c         -o bin/test_runtime.o && \
	  $(CC) $(B) runtime/coverage_runtime.c     -o bin/coverage_runtime.o && \
	  $(CC) $(B) runtime/profiler_runtime.c     -o bin/profiler_runtime.o && \
	  $(CC) $(B) runtime/profiler_sampler.c     -o bin/profiler_sampler.o && \
	  $(CC) -c -fPIC -O2 -D_FORTIFY_SOURCE=2 -fstack-protector-strong $(WARN_MAX) \
	        -Wformat -Werror=format-security -Werror=implicit-function-declaration \
	        -Werror=incompatible-pointer-types -Werror=int-conversion -Warray-bounds -ftrapv \
	        -fno-delete-null-pointer-checks -fno-strict-overflow -fno-strict-aliasing -fPIE \
	        -DWITH_OPENSSL $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` \
	        runtime/http_shared.c -o bin/http_shared.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_client_runtime.c      -o bin/http_client_runtime.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_server_request.c     -o bin/http_server_request.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_server_response.c    -o bin/http_server_response.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_server_runtime.c      -o bin/http_server_runtime.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/websocket_client_runtime.c -o bin/websocket_client_runtime.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/websocket_server_runtime.c -o bin/websocket_server_runtime.o && \
	  $(AR) rcs bin/libfiber_runtime.a $(FIB_OBJ) && \
	  $(AR) rcs bin/libhttp_runtime.a  $(HTTP_OBJ) && \
	  $(AR) rcs bin/libfiber_runtime_gc.a $(FIB_OBJ_GC) && \
	  $(AR) rcs bin/libhttp_runtime_gc.a  $(HTTP_OBJ_GC) && \
	  $(AR) rcs bin/libfiber_runtime_arc.a $(FIB_OBJ_ARC) && \
	  $(AR) rcs bin/libhttp_runtime_arc.a  $(HTTP_OBJ_ARC) && \
	  cp bin/libfiber_runtime.a bin/libhttp_runtime.a bin/libfiber_runtime_gc.a bin/libhttp_runtime_gc.a bin/libfiber_runtime_arc.a bin/libhttp_runtime_arc.a lib/
	@touch $@

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
	    $(WASM_CC) $(WASM_CFLAGS) runtime/$$u.c -o bin/wasm/$$u.o || exit 1; \
	  done && \
	  $(WASM_AR) rcs bin/libosprey_runtime_wasm.a $(addprefix bin/wasm/,$(addsuffix .o,$(WASM_RT_SRC))) && \
	  cp bin/libosprey_runtime_wasm.a lib/

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
	set -o pipefail && cargo llvm-cov --workspace --profile ci --lcov --output-path lcov.info 2>&1 | tee test.log

# Per-crate enforcement: every rust crate is gated
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
# The container suites link memory_runtime.o — the DEFAULT backend's no-op
# hooks — so they test container semantics with reference counting neutralised;
# memory_arc_tests covers the counting itself.
OSSL_CFLAGS = $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""`
OSSL_LIBS   = `pkg-config --libs openssl 2>/dev/null || echo "-lssl -lcrypto"`
RT_THREADS  = runtime/fiber_runtime.c runtime/system_runtime.c runtime/file_runtime.c runtime/effects_runtime.c runtime/effects_coro.c \
              runtime/profiler_runtime.c runtime/profiler_sampler.c
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
  memory_pool_tests memory_runtime_tests memory_golden_tests gpu_runtime_tests \
  list_tests map_tests string_runtime_tests json_runtime_tests \
  effects_runtime_tests builtins_runtime_tests test_system_runtime \
  test_http_length_validation http_server_send_tests http_server_request_tests \
  fiber_runtime_tests http_runtime_tests profiler_runtime_tests \
  coverage_runtime_tests
C_SRC_memory_gc_stack_root_tests = runtime/memory_gc_stack_root_tests.c runtime/memory_gc.c
C_LIBS_memory_gc_stack_root_tests = -pthread
C_SRC_memory_arc_tests = runtime/memory_arc_tests.c runtime/memory_arc.c
C_LIBS_memory_arc_tests = -pthread
C_SRC_memory_gc_tests = runtime/memory_gc_tests.c runtime/memory_gc.c
C_LIBS_memory_gc_tests = -pthread
C_SRC_memory_pool_tests = runtime/memory_pool_tests.c
C_SRC_memory_runtime_tests = runtime/memory_runtime_tests.c runtime/memory_runtime.c
C_SRC_memory_golden_tests = runtime/memory_golden_tests.c runtime/memory_arc.c runtime/gpu_runtime.c
C_LIBS_memory_golden_tests = -pthread
C_SRC_gpu_runtime_tests = runtime/gpu_runtime_tests.c runtime/gpu_runtime.c runtime/memory_runtime.c
C_SRC_list_tests = runtime/list_tests.c runtime/list_runtime.c runtime/memory_runtime.c
C_SRC_map_tests = runtime/map_tests.c runtime/map_runtime.c runtime/map_runtime_hamt.c runtime/memory_runtime.c
C_SRC_string_runtime_tests = runtime/string_runtime_tests.c runtime/string_runtime.c runtime/string_runtime_list.c runtime/memory_runtime.c
C_SRC_json_runtime_tests = runtime/json_runtime_tests.c runtime/json_runtime.c
C_LIBS_json_runtime_tests = -pthread
C_SRC_effects_runtime_tests = runtime/effects_runtime_tests.c runtime/effects_runtime.c runtime/effects_coro.c runtime/memory_arc.c runtime/gpu_runtime.c runtime/profiler_runtime.c runtime/profiler_sampler.c
C_LIBS_effects_runtime_tests = -pthread
C_SRC_builtins_runtime_tests = runtime/builtins_runtime_tests.c runtime/ffi_runtime.c runtime/random_runtime.c runtime/term_runtime.c runtime/test_runtime.c
C_SRC_test_system_runtime = runtime/test_system_runtime.c runtime/system_runtime.c runtime/file_runtime.c runtime/memory_runtime.c
C_LIBS_test_system_runtime = -pthread
C_FLAGS_test_http_length_validation = $(OSSL_CFLAGS)
C_SRC_test_http_length_validation = runtime/test_http_length_validation.c
C_FLAGS_http_server_send_tests = $(OSSL_CFLAGS)
C_SRC_http_server_send_tests = runtime/http_server_send_tests.c runtime/memory_runtime.c
C_LIBS_http_server_send_tests = -pthread
C_FLAGS_http_server_request_tests = $(OSSL_CFLAGS)
C_SRC_http_server_request_tests = runtime/http_server_request_tests.c runtime/memory_runtime.c
C_LIBS_http_server_request_tests = -pthread
C_SRC_fiber_runtime_tests = runtime/fiber_runtime_tests.c runtime/memory_runtime.c $(RT_THREADS)
C_LIBS_fiber_runtime_tests = -pthread
C_FLAGS_http_runtime_tests = $(OSSL_CFLAGS)
C_SRC_http_runtime_tests = runtime/http_runtime_tests.c runtime/http_client_runtime.c runtime/http_server_runtime.c runtime/http_server_request.c runtime/http_server_response.c runtime/http_shared.c runtime/websocket_client_runtime.c runtime/websocket_server_runtime.c runtime/string_runtime.c runtime/memory_runtime.c runtime/random_runtime.c $(RT_THREADS)
C_LIBS_http_runtime_tests = -pthread $(OSSL_LIBS)
C_PROFILE_profiler_runtime_tests = $(PROF_T)
C_SRC_profiler_runtime_tests = runtime/profiler_runtime_tests.c runtime/profiler_runtime.c runtime/profiler_sampler.c
C_LIBS_profiler_runtime_tests = -pthread
C_SRC_coverage_runtime_tests = runtime/coverage_runtime_tests.c runtime/coverage_runtime.c

# Build + run one suite (expanded inside a `cd compiler` shell, hence bin/).
C_SUITE_CMD = echo "--- $(1)" && $(CC) $(or $(C_PROFILE_$(1)),$(T)) $(C_FLAGS_$(1)) $(C_SRC_$(1)) $(C_LIBS_$(1)) -o bin/$(1) && ./bin/$(1)

_test_gc_stack_root:
	@echo "==> [c-runtime] GC caller-stack root regression..."
	@cd compiler && $(call C_SUITE_CMD,memory_gc_stack_root_tests)

_test_c_runtime:
	@echo "==> [c-runtime] memory/gpu/effects/json/builtins/system/string/HTTP/fiber suites..."
	@cd compiler && set -e; $(foreach s,$(C_TEST_SUITES),$(call C_SUITE_CMD,$(s)) && ) true

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
C_SHIPPED_UNITS = $(sort $(basename $(notdir $(FIB_OBJ) $(HTTP_OBJ) \
                    $(FIB_OBJ_GC) $(HTTP_OBJ_GC) $(FIB_OBJ_ARC) $(HTTP_OBJ_ARC))))
# The exemptions, and why: term_runtime.c and test_runtime.c run every case in a
# FORKED CHILD whose gcov counters are never flushed back, so gcov reports 0%
# against passing assertions — gate them once the harness calls __gcov_dump in
# the child.
C_COV_EXEMPT ?= term_runtime test_runtime
GCOV_TOOL ?= $(shell if $(CC) --version 2>/dev/null | grep -qi clang; then \
    if command -v xcrun >/dev/null 2>&1; then echo "xcrun llvm-cov gcov"; \
    else echo "llvm-cov gcov"; fi; \
  else echo gcov; fi)

_coverage_check_c_runtime:
	@echo "==> [c-runtime] per-library line coverage (gcov)..."
	@rm -rf compiler/bin/cov
	@set -e; cd compiler/bin && mkdir -p cov && cd cov; \
	$(foreach s,$(C_TEST_SUITES),mkdir -p $(s) && (cd $(s) && $(CC) $(or $(C_PROFILE_$(s)),$(T)) --coverage $(C_FLAGS_$(s)) $(addprefix ../../../,$(C_SRC_$(s))) $(C_LIBS_$(s)) -o $(s) && { ./$(s) >/dev/null 2>&1 || true; } && { $(GCOV_TOOL) *.gcda > summary.txt 2>/dev/null || true; }) && ) true
	@command -v jq >/dev/null || { echo "[c] FAIL: jq is required to read $(COVERAGE_THRESHOLDS_FILE); a gate that cannot run must not report success"; exit 1; }
	@libs=$$(jq -r '.projects | to_entries[] | select(.value.language=="c") | .key' "$(COVERAGE_THRESHOLDS_FILE)"); \
	if [ -z "$$libs" ]; then echo "[c] FAIL: no C entries in $(COVERAGE_THRESHOLDS_FILE) -- the gate would pass vacuously"; exit 1; fi; \
	fail=0; \
	for n in $(C_SHIPPED_UNITS); do \
	  printf '%s\n' $(C_COV_EXEMPT) $$libs | grep -qx "$$n" || { \
	    echo "[c] FAIL: runtime/$$n.c ships in a native archive but is neither gated in $(COVERAGE_THRESHOLDS_FILE) nor in C_COV_EXEMPT -- an ungated library cannot regress visibly"; fail=1; }; \
	done; \
	for lib in $$libs; do \
	  thr=$$(jq -r --arg l "$$lib" '.projects[$$l].threshold' "$(COVERAGE_THRESHOLDS_FILE)"); \
	  best=$$(for f in compiler/bin/cov/*/summary.txt; do \
	    [ -f "$$f" ] || continue; \
	    sed -n "\#File '.*/runtime/$$lib\.c'#{n;s/Lines executed:\([0-9.]*\)% of .*/\1/p;}" "$$f"; \
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

# Assertion-driven language corpus: recursively runs both *.test.osp and
# *.test.ospml suites. These inspect internal values rather than stdout goldens.
# Some loopback integration twins intentionally use the same fixed port so their
# Default and ML sources lower to identical IR; one worker prevents port races.
_test_language_corpus:
	@echo "==> [language] Default + ML assertion corpus..."
	@OSPREY_TEST_JOBS=1 ./$(BIN) test tests

# _test_goldens: byte-exact stdout comparison against the .expectedoutput
# goldens. Distinct from the target above, which runs the same corpus through
# the built-in TAP runner and so only ever observes whether the in-language
# assertions passed. An assertion can only state what someone thought to write
# down as a value; the goldens pin the ENTIRE observable output — print ORDER,
# exact formatting, and the TAP tally that exposes a deleted assertion.
_test_goldens:
	@echo "==> [language] golden stdout comparison..."
	@OSPREY_TEST_JOBS=1 zsh crates/run_test_corpus.sh default

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
_test_vscode_extension: _vsix_bundle
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

## gpu-demo: Render the gpu* kernel demo (fractal, shaded sphere, composite)
#            Runs on the CPU: [GPU-BACKEND-HOST] is the only backend, so the
#            gpu* builtins are a dispatch surface, not silicon. For pixels on
#            the actual GPU, see `make graphics`.
#            FLAVOR=ml renders the ML twin, which prints byte-identical output.
gpu-demo: build
	@echo "==> rendering gpu* kernel demo on the host backend (CPU)"
	./$(BIN) tests/core/gpu/raster.test.$(if $(filter ml,$(FLAVOR)),ospml,osp) --run

## graphics: Build the platform graphics bridge and run the animated GPU demo
#            Osprey drives the scene; the GPU shades every pixel. macOS renders
#            it through Metal, Windows through Direct3D 12 — and the Osprey
#            sources are the same files on both, which is the entire point.
#            SCENE=kali|opal|character selects one of the three scenes; every
#            one is a one-file project sharing examples/graphics/base/base.osp
#            and one named fragment entry in the platform shader library.
#            GFX_WARN overrides the C warning set. See $(GFX_DIR)/README.md.
GFX_DIR := examples/graphics
GFX_WARN ?= -Wall -Werror
SCENE ?= kali

ifeq ($(OS),Windows_NT)
# Direct3D 12. Built with the same MinGW UCRT64 toolchain that builds the C
# runtime on Windows, so `// @link: ospgfx` in base/base.osp resolves the import
# library below exactly as it resolves the dylib on macOS.
#
# UNVERIFIED. This branch, the two C files it compiles and base.hlsl were all
# written on a macOS machine with no Windows toolchain and have never been
# built or run. $(GFX_DIR)/README.md says what would establish that they work.
GFX_SHADER := $(GFX_DIR)/base.hlsl
GFX_SRC    := $(GFX_DIR)/ospgfx_d3d12.c $(GFX_DIR)/ospgfx_d3d12_setup.c
GFX_DLL    := $(GFX_DIR)/ospgfx.dll
GFX_LIB    := $(GFX_DIR)/libospgfx.dll.a
GFX_SYSLIB := -ld3d12 -ldxgi -ld3dcompiler -ldxguid -luser32
# Every entry point the run-time compiler will be asked for, with its target
# profile. `make graphics-shader` compiles all four so a typo in one scene's
# fragment cannot hide behind another scene running fine.
GFX_ENTRIES := osp_vertex:vs_5_0 osp_fragment:ps_5_0 \
	osp_fragment_opal:ps_5_0 osp_fragment_character:ps_5_0

ifeq ($(MSYSTEM),)
# Native PowerShell has neither bash for the recipes below nor a MinGW cc.
graphics graphics-shader:
	@Write-Host "make graphics needs the MSYS2 UCRT64 shell -- see $(GFX_DIR)/README.md"
else
$(GFX_LIB): $(GFX_SRC) $(GFX_DIR)/ospgfx_d3d12.h
	@echo "==> building Osprey -> Direct3D 12 graphics bridge"
	$(CC) -shared -O2 $(GFX_WARN) -o $(GFX_DLL) $(GFX_SRC) \
		-Wl,--out-implib,$(GFX_LIB) $(GFX_SYSLIB)

# The bridge compiles the shader at run time, so an error in it would otherwise
# only surface as a window that refuses to open. fxc is the same compiler
# D3DCompile is, so checking here checks exactly what the bridge will do; the
# Windows SDK ships it, but not on PATH, so the check skips when it is absent.
graphics-shader:
	@if command -v fxc.exe >/dev/null 2>&1; then \
		for entry in $(GFX_ENTRIES); do \
			echo "==> checking $(GFX_SHADER) $${entry%%:*}"; \
			fxc.exe -nologo -T $${entry##*:} -E $${entry%%:*} \
				-Fo $(GFX_DIR)/.shadercheck.cso $(GFX_SHADER) || exit 1; \
		done; \
		$(RM) $(GFX_DIR)/.shadercheck.cso; \
	else \
		echo "==> skipping shader check (no fxc.exe on PATH)"; \
	fi

# The DLL lives beside its source rather than beside the compiled scene, so the
# loader is pointed at it for the length of the run.
graphics: build $(GFX_LIB) graphics-shader
	@echo "==> launching Osprey graphics demo: $(SCENE) (close the window to exit)"
	PATH="$(CURDIR)/$(GFX_DIR):$$PATH" ./$(BIN) $(GFX_DIR)/$(SCENE) --run
endif

else
GFX_LIB := $(GFX_DIR)/libospgfx.dylib
GFX_SHADER := $(GFX_DIR)/base.metal

$(GFX_LIB): $(GFX_DIR)/ospgfx.m
	@echo "==> building Osprey -> macOS graphics bridge"
	clang -dynamiclib -fobjc-arc -O2 $(GFX_WARN) \
		-install_name $(CURDIR)/$(GFX_LIB) \
		-framework Cocoa -framework Metal -framework QuartzCore \
		-o $(GFX_LIB) $(GFX_DIR)/ospgfx.m

# The bridge compiles the shader at run time, so a syntax error in it would
# otherwise only surface as a window that refuses to open. Check it up front
# when the Metal toolchain is installed, and skip quietly when it is not.
graphics-shader:
	@if xcrun -sdk macosx --find metal >/dev/null 2>&1; then \
		echo "==> checking $(GFX_SHADER)"; \
		xcrun -sdk macosx metal -c $(GFX_SHADER) -o /dev/null; \
	else \
		echo "==> skipping shader check (no Metal toolchain)"; \
	fi

graphics: build $(GFX_LIB) graphics-shader
	@echo "==> launching Osprey graphics demo: $(SCENE) (close the window to exit)"
	./$(BIN) $(GFX_DIR)/$(SCENE) --run
endif

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

## partial-bench: Re-run only the Osprey implementations and merge those
##        measurements into the existing results. Every non-Osprey result is
##        preserved byte-for-byte at the data-record level.
partial-bench: build
	@BENCH_PARTIAL=1 zsh benchmarks/run.sh $(BENCH_FILTER)

## vsix-rebuild-reinstall: Clean → build → reinstall the Osprey VSCode
##      extension in place, bundling the freshly-built Rust compiler as `osprey`.
##      Touches ONLY the Osprey extension ($(EXT_ID)) in the DEFAULT profile —
##      never another extension, never another VSCode profile. macOS only.
##      ONE `code` invocation (install --force, no separate uninstall) so the
##      running VSCode reconciles its extension host exactly once, not twice.
vsix-rebuild-reinstall:
	$(MAKE) _vsix_clean
	$(MAKE) build
	$(MAKE) _vsix_bundle
	$(MAKE) _vsix_package
	$(MAKE) _vsix_install

# _rebuild-install-vsix: deprecated private alias of `vsix-rebuild-reinstall`.
_rebuild-install-vsix: vsix-rebuild-reinstall

# --- vsix sub-steps ---------------------------------------------------------
_vsix_clean:
	$(MAKE) clean
	cd $(EXT_DIR) && $(RM) bin osprey-*.vsix

_vsix_build:
	$(MAKE) build

# Stage the freshly-built Rust binary AND the C runtime archives where the
# extension expects its bundled compiler (bin/<os>-<arch>/), so the VSIX runs
# against THIS build. The compiler locates its runtime archives next to its own
# executable (find_runtime_lib in osprey-cli), so every native runtime variant
# must sit beside the bundled `osprey` for `--run --memory=<mode>` to link.
_vsix_bundle:
	@OS=$$(uname -s | tr '[:upper:]' '[:lower:]'); \
	case "$$OS" in darwin) OS=darwin;; linux) OS=linux;; *) OS=win32;; esac; \
	ARCH=$$(uname -m); case "$$ARCH" in arm64|aarch64) ARCH=arm64;; *) ARCH=x64;; esac; \
	DEST="$(EXT_DIR)/bin/$$OS-$$ARCH"; $(MKDIR) "$$DEST"; \
	cp $(BIN) "$$DEST/osprey"; \
	cp $(RTB)/libfiber_runtime.a $(RTB)/libhttp_runtime.a \
	   $(RTB)/libfiber_runtime_gc.a $(RTB)/libhttp_runtime_gc.a \
	   $(RTB)/libfiber_runtime_arc.a $(RTB)/libhttp_runtime_arc.a "$$DEST/"; \
	echo "  bundled $(BIN) + all native runtime archives -> $$DEST/"

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
