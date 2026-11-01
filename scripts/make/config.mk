# agent-pmo:b636503
# =============================================================================
# Standard Makefile — osprey
# Cross-platform: Linux, macOS, Windows (via GNU Make)
# Primary language: Rust (crates/ workspace → the osprey compiler), with a
# pure-C runtime (compiler/runtime → lib*_runtime.a, linked by `osprey
# --run`) and TypeScript sub-projects (vscode-extension, webcompiler, website).
# =============================================================================

.PHONY: build test lint fmt clean ci setup run install _bench _bench-osprey _wasm _wasm-site _wasm-serve _bank _bank-web _bank-test _bank-e2e hawk hawk-install _gpt2 _gpt2-train \
	_registry-test _registry-serve _registry-serve-sample _registry-blockers _vsix-rebuild-reinstall \
	_language-test _deslop _gpu-demo _graphics-shader \
	_test_gc_stack_root _test_c_runtime _test_gpu_device _coverage_check_c_runtime _bank_test _test_bench_tools _test_registry _test_registry_blockers \
	_runtime_klee _test_hunt_mock \
	_vsix_clean _vsix_bundle _vsix_package _vsix_install _graphics _graphics-smoke

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

# The folder Makefiles `make help` lists after the root's own targets.
MAKE_FOLDERS ?= apps apps/macos apps/windows apps/windows/tests packages tests/hunt vscode-extension website benchmarks examples/proof

# Toolchain / paths. BIN: the built CLI. RTB: C-runtime archive output dir
# (osprey searches compiler/bin at --run time).
CC  ?= cc
AR  ?= ar
BIN ?= target/release/osprey
RTB ?= compiler/bin
# The release version a build is stamped with (release.yml exports it); the
# runtime reports it to a live-attach client [PROF-ATTACH-PROTOCOL].
OSPREY_VERSION ?= 0.0.0-dev
# cargo's parallel jobs, inherited by every recipe that builds Rust.
export CARGO_BUILD_JOBS ?= 4

# VSIX (VSCode extension) — macOS only. Bundles the Rust binary as `osprey`.
# All VSIX targets touch ONLY this extension id, ONLY in the default profile;
# they never enumerate VSCode profiles and never affect any other extension.
EXT_DIR        ?= vscode-extension
EXT_ID         ?= nimblesite.osprey
# The extension loads its bundled compiler from bin/<os>-<arch>/osprey
# (resolveBundledCompiler in client/src/extension.ts), so bundling, packaging
# and install-verification must all agree on one triple — computed here once.
# (make's $(shell ...) ends at the first unbalanced ")", so a shell `case` is
# not expressible here — the mapping is done with make's own functions.)
UNAME_S        ?= $(shell uname -s)
UNAME_M        ?= $(shell uname -m)
EXT_OS         ?= $(if $(filter Darwin,$(UNAME_S)),darwin,$(if $(filter Linux,$(UNAME_S)),linux,win32))
EXT_ARCH       ?= $(if $(filter arm64 aarch64,$(UNAME_M)),arm64,x64)
EXT_PLATFORM   ?= $(EXT_OS)-$(EXT_ARCH)

# ---------------------------------------------------------------------------
# Node dependency guard
# ---------------------------------------------------------------------------
# Every recipe that runs a tool out of a node subproject needs that project's
# node_modules, and nothing in this Makefile installed one. `make build` on a
# tree that had never run `make setup` reached `tsc -b` and died with
# "tsc: command not found" — `npm run compile` silently assumed the install had
# already happened somewhere else. A build target that only works after an
# undocumented manual step is not a build target, and it fails as a missing
# binary rather than as the missing install it actually is. The comment above
# `_bank_test` records the same trap being routed around in CI instead of
# fixed.
#
# npm writes node_modules/.package-lock.json as the last act of a successful
# install, so it is a truthful stamp: newer than package-lock.json exactly when
# the installed tree matches the lockfile. Make re-runs the install only when
# the lockfile moves or the tree is gone, so an up-to-date project costs one
# stat rather than a reinstall. `npm ci` is the right installer precisely
# because it refuses to paper over a package.json that disagrees with its lock.
EXT_NODE_DEPS         = $(EXT_DIR)/node_modules/.package-lock.json
WEBSITE_NODE_DEPS     = website/node_modules/.package-lock.json
WEBCOMPILER_NODE_DEPS = webcompiler/node_modules/.package-lock.json
BANK_WEB_NODE_DEPS    = examples/projects/modules/web/node_modules/.package-lock.json
BANK_E2E_NODE_DEPS    = examples/projects/modules/e2e/node_modules/.package-lock.json

# Build logs and coverage reports live under target/, never in the repo root.
LOG_DIR      = target/logs
COVERAGE_DIR = target/coverage
RUST_LCOV    = $(COVERAGE_DIR)/lcov.info
RUST_LOG     = $(LOG_DIR)/rust-test.log

%/node_modules/.package-lock.json: %/package-lock.json
	@echo "==> [$*] installing node dependencies (npm ci)..."
	npm --prefix $* ci

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
# -fno-omit-frame-pointer is load-bearing: the profiler captures stacks by a
# frame-pointer chain walk only [PROF-COLLECT-UNWIND], so every runtime frame
# must stay chained or a sample taken inside the runtime (the collector, a
# string allocation) loses its Osprey callers and is charged to nobody. gcc on
# x86-64 omits the pointer at -O2 by default; arm64 keeps it regardless.
FRAME_POINTERS ?= -fno-omit-frame-pointer
A    ?= -c -fPIC -O2 $(FRAME_POINTERS) -D_FORTIFY_SOURCE=2 -fstack-protector-strong $(WARN_MAX) -ftrapv -D_GNU_SOURCE
B    ?= $(A) -std=c11
OSSL ?= -DOPENSSL_SUPPRESS_DEPRECATED -DOPENSSL_API_COMPAT=30000 -Wno-deprecated-declarations
# C unit-test flag profile: the archive hardening core, but linking an
# executable (no -c/-fPIC/-fPIE). Named once so _test_c_runtime does not repeat
# the flag list per suite.
T    ?= -O2 -D_FORTIFY_SOURCE=2 -fstack-protector-strong $(WARN) -ftrapv -std=c11 -D_GNU_SOURCE
# Object lists for the archives (paths relative to compiler/, where `ar` runs).
# The profiler's objects, linked into every native runtime archive
# [PROF-ACTIVATE-ENV] [PROF-ATTACH-ENDPOINT].
PROF_OBJ = bin/profiler_runtime.o bin/profiler_store.o bin/profiler_dump.o bin/profiler_sampler.o bin/profiler_stream.o bin/profiler_endpoint.o
FIB_OBJ  ?= bin/memory_runtime.o bin/gpu_runtime.o bin/gpu_host.o bin/tensor_file.o bin/tensor_list.o bin/fiber_runtime.o bin/system_runtime.o bin/file_runtime.o bin/effects_runtime.o bin/effects_coro.o bin/string_runtime.o bin/string_runtime_list.o bin/float_spelling.o bin/list_runtime.o bin/map_runtime.o bin/map_runtime_hamt.o bin/json_runtime.o bin/json_text.o bin/json_codec.o bin/ffi_runtime.o bin/term_runtime.o bin/random_runtime.o bin/test_runtime.o bin/coverage_runtime.o $(PROF_OBJ)
HTTP_OBJ ?= bin/http_shared.o bin/http_client_runtime.o bin/http_client_response.o bin/http_server_request.o bin/http_server_response.o bin/http_server_runtime.o bin/websocket_client_runtime.o bin/websocket_server_runtime.o bin/tls_client_runtime.o bin/file_server_runtime.o bin/media_render_runtime.o $(FIB_OBJ)
# GC backend archives (osprey --memory=gc): the tracing collector replaces
# memory_runtime.o, and the value-container units are rebuilt with the malloc
# redirect (osp_gc_shim.h) so their nodes live in the managed heap. Everything
# else is the same object. Implements [GC-TRACE-CONSERVATIVE], spec 0018.
FIB_OBJ_GC  ?= bin/memory_gc.o bin/gpu_runtime.o bin/gpu_host.o bin/tensor_file.o bin/tensor_list.o bin/fiber_runtime.o bin/system_runtime.o bin/file_runtime.o bin/effects_runtime.o bin/effects_coro.o bin/string_runtime.o bin/string_runtime_list.o bin/float_spelling.o bin/gc/list_runtime.o bin/gc/map_runtime.o bin/gc/map_runtime_hamt.o bin/json_runtime.o bin/json_text.o bin/json_codec.o bin/ffi_runtime.o bin/term_runtime.o bin/random_runtime.o bin/test_runtime.o bin/coverage_runtime.o $(PROF_OBJ)
HTTP_OBJ_GC ?= bin/http_shared.o bin/http_client_runtime.o bin/http_client_response.o bin/http_server_request.o bin/http_server_response.o bin/http_server_runtime.o bin/websocket_client_runtime.o bin/websocket_server_runtime.o bin/tls_client_runtime.o bin/file_server_runtime.o bin/media_render_runtime.o $(FIB_OBJ_GC)
# ARC backend archives (osprey --memory=arc): Perceus reference counting
# replaces memory_runtime.o, and the value-producing units (containers +
# strings + JSON) are rebuilt with the allocation redirect (osp_arc_shim.h) so
# their nodes/buffers carry the 16-byte header and registry entry. Implements
# [GC-ARC-PERCEUS], spec 0018.
FIB_OBJ_ARC  ?= bin/memory_arc.o bin/gpu_runtime.o bin/gpu_host.o bin/tensor_file.o bin/tensor_list.o bin/fiber_runtime.o bin/system_runtime.o bin/arc/file_runtime.o bin/effects_runtime.o bin/effects_coro.o bin/arc/string_runtime.o bin/arc/string_runtime_list.o bin/float_spelling.o bin/arc/list_runtime.o bin/arc/map_runtime.o bin/arc/map_runtime_hamt.o bin/arc/json_runtime.o bin/arc/json_text.o bin/arc/json_codec.o bin/ffi_runtime.o bin/term_runtime.o bin/random_runtime.o bin/test_runtime.o bin/coverage_runtime.o $(PROF_OBJ)
HTTP_OBJ_ARC ?= bin/http_shared.o bin/http_client_runtime.o bin/http_client_response.o bin/http_server_request.o bin/http_server_response.o bin/http_server_runtime.o bin/websocket_client_runtime.o bin/websocket_server_runtime.o bin/tls_client_runtime.o bin/file_server_runtime.o bin/media_render_runtime.o $(FIB_OBJ_ARC)
ifeq ($(UNAME_S),Darwin)
GPU_DEVICE_OBJ ?= bin/gpu_metal.o
GPU_DEVICE_NATIVE ?= compiler/bin/gpu_metal.o
GPU_DEVICE_FLAGS ?= -DOSPREY_GPU_METAL=1
GPU_DEVICE_LIBS ?= -framework Foundation -framework Metal
FIB_OBJ += bin/gpu_metal.o
FIB_OBJ_GC += bin/gpu_metal.o
FIB_OBJ_ARC += bin/gpu_metal.o
endif
GPU_DEVICE_OBJ += bin/gpu_opencl.o
GPU_DEVICE_NATIVE += compiler/bin/gpu_opencl.o
GPU_DEVICE_FLAGS += -DOSPREY_GPU_OPENCL=1
GPU_DEVICE_LINK = $(addprefix $(CURDIR)/compiler/,$(GPU_DEVICE_OBJ))
FIB_OBJ += bin/gpu_opencl.o
FIB_OBJ_GC += bin/gpu_opencl.o
FIB_OBJ_ARC += bin/gpu_opencl.o
ifeq ($(UNAME_S),Linux)
GPU_DEVICE_LIBS += -ldl -pthread
endif
# The iOS and Android archives carry no device adapter: their gpu_runtime is
# compiled without the adapter flags, so every kernel and product runs the host
# oracle and a device selection is the no-adapter error [GPU-BACKEND-DEVICE].
MOBILE_RT_SRC ?= $(filter-out system_runtime $(basename $(notdir $(GPU_DEVICE_OBJ))),$(basename $(notdir $(FIB_OBJ))))
MOBILE_ARC_RT_SRC ?= memory_arc $(filter-out memory_runtime,$(MOBILE_RT_SRC))
MOBILE_ARC_SHIM_SRC ?= $(patsubst bin/arc/%.o,%,$(filter bin/arc/%,$(FIB_OBJ_ARC)))
NATIVE_RUNTIME_CONFIG ?= compiler/bin/.native-runtime-config
NATIVE_RUNTIME_STAMP ?= compiler/bin/.native-runtime.stamp
NATIVE_RUNTIME_INPUTS ?= $(filter-out compiler/runtime/%_tests.c compiler/runtime/test_http_length_validation.c compiler/runtime/test_openssl.c compiler/runtime/test_system_runtime.c compiler/runtime/test_file_runtime.c compiler/runtime/web_runtime.c,$(wildcard compiler/runtime/*.c)) $(wildcard compiler/runtime/*.h)
NATIVE_RUNTIME_INPUTS += $(wildcard compiler/runtime/*.m compiler/runtime/*.metal)
NATIVE_RUNTIME_ARCHIVES ?= compiler/bin/libfiber_runtime.a compiler/bin/libhttp_runtime.a compiler/bin/libfiber_runtime_gc.a compiler/bin/libhttp_runtime_gc.a compiler/bin/libfiber_runtime_arc.a compiler/bin/libhttp_runtime_arc.a compiler/lib/libfiber_runtime.a compiler/lib/libhttp_runtime.a compiler/lib/libfiber_runtime_gc.a compiler/lib/libhttp_runtime_gc.a compiler/lib/libfiber_runtime_arc.a compiler/lib/libhttp_runtime_arc.a

# WebAssembly (wasm32-wasip1) cross-build toolchain — opt-in via `make -C website wasm`.
# Compiles the portable C-runtime subset (no pthreads/sockets/OpenSSL/syscalls)
# to a wasm archive osprey links with `--target=wasm32`. See docs/specs/0022.
WASM_LLVM_BIN ?= $(shell for d in /opt/homebrew/opt/llvm/bin /usr/local/opt/llvm/bin; do [ -x "$$d/clang" ] && { echo "$$d"; break; }; done)
WASM_LLD_BIN  ?= $(shell for d in /opt/homebrew/opt/lld/bin /usr/local/opt/lld/bin "$(WASM_LLVM_BIN)"; do [ -n "$$d" ] && [ -x "$$d/wasm-ld" ] && { echo "$$d"; break; }; done)
WASM_PATH_PREFIX ?= $(shell for d in "$(WASM_LLVM_BIN)" "$(WASM_LLD_BIN)"; do [ -n "$$d" ] && printf "%s:" "$$d"; done)
WASM_CC      ?= $(if $(WASM_LLVM_BIN),$(WASM_LLVM_BIN)/clang,clang)
WASM_AR      ?= $(if $(WASM_LLVM_BIN),$(WASM_LLVM_BIN)/llvm-ar,$(shell command -v llvm-ar 2>/dev/null || ls /usr/bin/llvm-ar-* 2>/dev/null | sort -V | tail -n 1))
WASM_TARGET  ?= wasm32-wasip1
# WASI sysroot (libc + crt1). Override with WASI_SYSROOT=/path; else probe the
# Homebrew (macOS), wasi-sdk and common Linux locations in turn.
WASI_SYSROOT ?= $(shell for d in "$$OSPREY_WASI_SYSROOT" \
  /opt/homebrew/opt/wasi-libc/share/wasi-sysroot \
  /usr/local/opt/wasi-libc/share/wasi-sysroot \
  /opt/wasi-sdk/share/wasi-sysroot "$$WASI_SDK_PATH/share/wasi-sysroot" \
  /usr/share/wasi-sysroot; do [ -n "$$d" ] && [ -d "$$d" ] && { echo "$$d"; exit; }; done; \
  [ -f /usr/lib/wasm32-wasip1/crt1-command.o ] && echo /usr)
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
WASM_RT_SRC  ?= memory_runtime gpu_runtime gpu_host tensor_file tensor_list string_runtime string_runtime_list float_spelling list_runtime map_runtime map_runtime_hamt json_runtime json_text json_codec effects_runtime test_runtime coverage_runtime web_runtime profiler_runtime wasm_builtins_runtime system_runtime file_runtime random_runtime

# The ARC twin of that archive, linked by `--memory=arc` — the wasm32 default
# [WASM-TARGET-MEMORY]. Same unit set with memory_arc.c in place of the malloc
# passthrough, and the value-container units built through osp_arc_shim.h, so a
# runtime-minted string or node carries the Perceus header the native ARC
# archive gives it [GC-ARC-PERCEUS]. WASI is single-threaded, so ARC's fiber
# lock stays untripped, and memory_arc.c reads 4-byte pointer slots there.
WASM_ARC_LIB      ?= libosprey_runtime_wasm_arc.a
WASM_ARC_SHIM_SRC ?= string_runtime string_runtime_list list_runtime map_runtime map_runtime_hamt json_runtime json_text json_codec file_runtime
WASM_ARC_RT_SRC   ?= memory_arc $(filter-out memory_runtime,$(WASM_RT_SRC))

# Symbolic execution with KLEE [SYMEX-RUNTIME-BITCODE]. The portable runtime
# subset plus the mock C library compile to one bitcode module with the pinned
# KLEE's own clang (Homebrew's llvm@19), so the module parses in KLEE's LLVM.
# x86_64-unknown-linux-gnu on every host: KLEE's va_arg lowering is x86-64
# only, and the mock's own headers make the triple carry no libc. The build is
# skipped loudly when KLEE's toolchain is absent; `osprey hunt` then says so.
KLEE_PREFIX   ?= $(shell for d in "$$OSPREY_KLEE_PREFIX" /opt/homebrew/opt/klee /usr/local/opt/klee /home/linuxbrew/.linuxbrew/opt/klee; do [ -n "$$d" ] && [ -x "$$d/bin/klee" ] && { echo "$$d"; break; }; done)
KLEE_LLVM_BIN ?= $(shell for d in "$$OSPREY_KLEE_LLVM" /opt/homebrew/opt/llvm@19/bin /usr/local/opt/llvm@19/bin /home/linuxbrew/.linuxbrew/opt/llvm@19/bin /usr/lib/llvm-19/bin; do [ -n "$$d" ] && [ -x "$$d/clang" ] && { echo "$$d"; break; }; done)
KLEE_CC       ?= $(KLEE_LLVM_BIN)/clang
KLEE_LINK     ?= $(KLEE_LLVM_BIN)/llvm-link
KLEE_NM       ?= $(KLEE_LLVM_BIN)/llvm-nm
KLEE_RT_SRC   ?= $(filter-out memory_runtime web_runtime wasm_builtins_runtime,$(WASM_RT_SRC)) $(patsubst compiler/runtime/%.c,%,$(filter-out %_tests.c,$(wildcard compiler/runtime/klee/*.c)))
KLEE_CFLAGS   ?= --target=x86_64-unknown-linux-gnu -nostdlibinc -isystem runtime/klee/include \
  -emit-llvm -c -g -O0 -Xclang -disable-O0-optnone -std=c11 $(WARN_MAX) -U_FORTIFY_SOURCE -fno-stack-protector \
  -fsanitize=undefined -fno-sanitize=function -DOSP_SYMBOLIC -DOSP_FLOAT_SPELLING_PORTABLE -D_GNU_SOURCE '-D__thread=' '-D_Thread_local='
KLEE_RUNTIME_BC ?= compiler/bin/libosprey_runtime_klee_gc.bc
KLEE_GC_SHIMS := $(patsubst bin/gc/%.o,%,$(filter bin/gc/%,$(FIB_OBJ_GC)))
KLEE_ARC_SHIMS := $(patsubst bin/arc/%.o,%,$(filter bin/arc/%,$(FIB_OBJ_ARC)))
KLEE_ROOT_PLUGIN := compiler/bin/libosprey_gc_roots.so
# The mock C library built for the HOST, so its unit tests can run on any
# machine (the mock's own klee/klee.h stands in for KLEE's) [SYMEX-MOCK-TESTS].
HUNT_MOCK_SRC   := $(patsubst compiler/runtime/%.c,%,$(filter-out %_tests.c,$(wildcard compiler/runtime/klee/*.c)))
HUNT_MOCK_HOST_CFLAGS ?= -nostdinc -isystem $(shell $(CC) -print-file-name=include) -isystem runtime/klee/include -ffreestanding -fno-builtin -std=c11 -O1 -g $(WARN_MAX) \
  -U_FORTIFY_SOURCE -fno-stack-protector -DOSP_SYMBOLIC -DOSP_FLOAT_SPELLING_PORTABLE -D_GNU_SOURCE '-D__thread=' '-D_Thread_local='
# Symbols KLEE itself defines, so the closed-module check [SYMEX-MOCK-CLOSED]
# accepts them: its intrinsics, UBSan handlers, allocator and `--libc=klee`.
KLEE_PROVIDED := ^(klee_|__ubsan_handle_|__assert_fail$$|abort$$|atexit$$|exit$$|_exit$$|calloc$$|free$$|malloc$$|realloc$$|mem(cmp|cpy|move|set|chr)$$|str(len|cmp|ncmp|chr|rchr|cpy|ncpy|cat|tol|toul)$$|to(lower|upper)$$|atoi$$)
# `make -C website wasm-serve` static-host dir + port for the in-browser example.
WASM_SERVE_DIR  ?= examples/wasm
WASM_SERVE_PORT ?= 8080
