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
	      "CC=$(CC)" "AR=$(AR)" "A=$(A)" "B=$(B)" "WARN_MAX=$(WARN_MAX)" "GPU_DEVICE_FLAGS=$(GPU_DEVICE_FLAGS)" \
	      "OSSL=$(OSSL)" "OSPREY_VERSION=$(OSPREY_VERSION)" "FIB_OBJ=$(FIB_OBJ)" "HTTP_OBJ=$(HTTP_OBJ)" \
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

GPU_SHADER_SRC := compiler/runtime/gpu_f64.metal compiler/runtime/gpu_math64.metal compiler/runtime/gpu_math_ext.metal compiler/runtime/gpu_device_lib.metal
compiler/bin/gpu_shader_source.h: $(GPU_SHADER_SRC) scripts/embed_gpu_shader.awk
	@$(MKDIR) compiler/bin
	awk -f scripts/embed_gpu_shader.awk $(GPU_SHADER_SRC) > $@

compiler/bin/gpu_metal.o: compiler/runtime/gpu_metal.m compiler/bin/gpu_shader_source.h compiler/runtime/gpu_device.h Makefile scripts/make/config.mk scripts/make/runtime.mk
	$(CC) $(A) -fobjc-arc $(GPU_DEVICE_FLAGS) $< -o $@

compiler/bin/gpu_opencl.o: compiler/runtime/gpu_opencl.c compiler/bin/gpu_shader_source.h compiler/runtime/gpu_opencl_api.h Makefile scripts/make/config.mk scripts/make/runtime.mk
	$(CC) $(B) $< -o $@

$(NATIVE_RUNTIME_STAMP): $(NATIVE_RUNTIME_INPUTS) $(NATIVE_RUNTIME_CONFIG) Makefile scripts/make/config.mk scripts/make/runtime.mk $(GPU_DEVICE_NATIVE)
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
	  $(CC) $(B) -include runtime/osp_arc_shim.h runtime/json_text.c           -o bin/arc/json_text.o && \
	  $(CC) $(B) -include runtime/osp_arc_shim.h runtime/json_codec.c          -o bin/arc/json_codec.o && \
	  $(CC) $(A) -include runtime/osp_arc_shim.h runtime/file_runtime.c        -o bin/arc/file_runtime.o && \
	  $(CC) -c -fPIC -O2 $(FRAME_POINTERS) $(WARN_MAX) -Wpedantic -std=c11 -D_GNU_SOURCE runtime/fiber_runtime.c -o bin/fiber_runtime.o && \
	  $(CC) $(A) runtime/system_runtime.c       -o bin/system_runtime.o && \
	  $(CC) $(A) runtime/file_runtime.c         -o bin/file_runtime.o && \
	  $(CC) $(A) runtime/effects_runtime.c      -o bin/effects_runtime.o && \
	  $(CC) $(A) runtime/effects_coro.c         -o bin/effects_coro.o && \
	  $(CC) $(A) runtime/string_runtime.c       -o bin/string_runtime.o && \
	  $(CC) $(A) runtime/string_runtime_list.c  -o bin/string_runtime_list.o && \
	  $(CC) $(B) runtime/float_spelling.c       -o bin/float_spelling.o && \
	  $(CC) $(B) runtime/list_runtime.c         -o bin/list_runtime.o && \
	  $(CC) $(B) runtime/map_runtime.c          -o bin/map_runtime.o && \
	  $(CC) $(B) runtime/map_runtime_hamt.c     -o bin/map_runtime_hamt.o && \
	  $(CC) $(B) runtime/json_runtime.c         -o bin/json_runtime.o && \
	  $(CC) $(B) runtime/json_text.c            -o bin/json_text.o && \
	  $(CC) $(B) runtime/json_codec.c           -o bin/json_codec.o && \
	  $(CC) $(B) runtime/ffi_runtime.c          -o bin/ffi_runtime.o && \
	  $(CC) $(B) runtime/term_runtime.c         -o bin/term_runtime.o && \
	  $(CC) $(B) runtime/random_runtime.c       -o bin/random_runtime.o && \
	  $(CC) $(B) $(GPU_DEVICE_FLAGS) runtime/gpu_runtime.c -o bin/gpu_runtime.o && \
	  $(CC) $(B) runtime/gpu_host.c             -o bin/gpu_host.o && \
	  $(CC) $(B) runtime/tensor_file.c          -o bin/tensor_file.o && \
	  $(CC) $(B) runtime/tensor_list.c          -o bin/tensor_list.o && \
	  $(CC) $(B) runtime/test_runtime.c         -o bin/test_runtime.o && \
	  $(CC) $(B) runtime/coverage_runtime.c     -o bin/coverage_runtime.o && \
	  $(CC) $(B) runtime/profiler_runtime.c     -o bin/profiler_runtime.o && \
	  $(CC) $(B) runtime/profiler_sampler.c     -o bin/profiler_sampler.o && \
	  $(CC) $(B) runtime/profiler_store.c       -o bin/profiler_store.o && \
	  $(CC) $(B) runtime/profiler_dump.c        -o bin/profiler_dump.o && \
	  $(CC) $(B) -DOSP_RUNTIME_VERSION='"$(OSPREY_VERSION)"' runtime/profiler_stream.c -o bin/profiler_stream.o && \
	  $(CC) $(B) runtime/profiler_endpoint.c     -o bin/profiler_endpoint.o && \
	  $(CC) -c -fPIC -O2 $(FRAME_POINTERS) -D_FORTIFY_SOURCE=2 -fstack-protector-strong $(WARN_MAX) \
	        -Wformat -Werror=format-security -Werror=implicit-function-declaration \
	        -Werror=incompatible-pointer-types -Werror=int-conversion -Warray-bounds -ftrapv \
	        -fno-delete-null-pointer-checks -fno-strict-overflow -fno-strict-aliasing \
	        -DWITH_OPENSSL $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` \
	        runtime/http_shared.c -o bin/http_shared.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_client_runtime.c      -o bin/http_client_runtime.o && \
  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_client_response.c     -o bin/http_client_response.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_server_request.c     -o bin/http_server_request.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_server_response.c    -o bin/http_server_response.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/http_server_runtime.c      -o bin/http_server_runtime.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/websocket_client_runtime.c -o bin/websocket_client_runtime.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/websocket_server_runtime.c -o bin/websocket_server_runtime.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/tls_client_runtime.c        -o bin/tls_client_runtime.o && \
	  $(CC) $(A) $(OSSL) `pkg-config --cflags openssl 2>/dev/null || echo ""` runtime/file_server_runtime.c       -o bin/file_server_runtime.o && \
	  $(RM) bin/libfiber_runtime.a bin/libhttp_runtime.a bin/libfiber_runtime_gc.a bin/libhttp_runtime_gc.a bin/libfiber_runtime_arc.a bin/libhttp_runtime_arc.a && \
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
# (the `--memory=none` passthrough) and the ARC twin
# libosprey_runtime_wasm_arc.a that `--memory=arc` links [WASM-TARGET-MEMORY].
_runtime_wasm:
	@if [ -z "$(WASI_SYSROOT)" ]; then \
	  echo "ERROR: no WASI sysroot found. Install it with 'brew install lld wasi-libc'"; \
	  echo "       (macOS) or the wasi-sdk, or set WASI_SYSROOT=/path/to/wasi-sysroot."; \
	  exit 1; fi
	@echo "==> building wasm runtime archives ($(WASM_TARGET), sysroot $(WASI_SYSROOT))"
	@cd compiler && set -e && $(MKDIR) bin/wasm bin/wasm/arc lib && \
	  for u in $(WASM_RT_SRC); do \
	    $(WASM_CC) $(WASM_CFLAGS) runtime/$$u.c -o bin/wasm/$$u.o || exit 1; \
	  done && \
	  $(WASM_AR) rcs bin/libosprey_runtime_wasm.a $(addprefix bin/wasm/,$(addsuffix .o,$(WASM_RT_SRC))) && \
	  for u in $(WASM_ARC_RT_SRC); do \
	    shim=""; case " $(WASM_ARC_SHIM_SRC) " in *" $$u "*) shim="-include runtime/osp_arc_shim.h";; esac; \
	    $(WASM_CC) $(WASM_CFLAGS) $$shim runtime/$$u.c -o bin/wasm/arc/$$u.o || exit 1; \
	  done && \
	  $(WASM_AR) rcs bin/$(WASM_ARC_LIB) $(addprefix bin/wasm/arc/,$(addsuffix .o,$(WASM_ARC_RT_SRC))) && \
	  cp bin/libosprey_runtime_wasm.a bin/$(WASM_ARC_LIB) lib/

# The runtime bitcode `osprey hunt` links under KLEE [SYMEX-RUNTIME-BITCODE]:
# the portable runtime subset plus the mock C library, compiled with KLEE's
# clang and linked into one module. Skips loudly without the toolchain, so
# `make build` stays green on a machine without KLEE. The closed-module check
# [SYMEX-MOCK-CLOSED] fails the build on any undefined symbol KLEE does not
# itself provide: that is a libc call the mock forgot.
_runtime_klee:
	@if [ -z "$(KLEE_PREFIX)" ] || [ ! -x "$(KLEE_CC)" ]; then \
	  echo "(KLEE toolchain not found — symbolic GC/ARC runtimes not built)"; exit 0; fi; \
	set -e; mkdir -p compiler/bin; \
	$(KLEE_LLVM_BIN)/clang++ -fPIC -fno-exceptions -fno-rtti -shared \
	  $$($(KLEE_LLVM_BIN)/llvm-config --cxxflags) compiler/llvm/gc_roots.cpp \
	  -o $(KLEE_ROOT_PLUGIN) $$($(KLEE_LLVM_BIN)/llvm-config --ldflags --libs --system-libs); \
	cd compiler; \
	for backend in gc arc; do \
	  echo "==> building symbolic $$backend runtime"; mkdir -p bin/klee/$$backend; \
	  case $$backend in gc) shims="$(KLEE_GC_SHIMS)";; arc) shims="$(KLEE_ARC_SHIMS)";; esac; \
	  objects=""; \
	  for u in $(KLEE_RT_SRC) memory_$$backend; do \
	    shim=""; case " $$shims " in *" $$u "*) shim="-include runtime/osp_$${backend}_shim.h";; esac; \
	    object=bin/klee/$$backend/$$(basename $$u).bc; \
	    $(KLEE_CC) $(KLEE_CFLAGS) $$shim runtime/$$u.c -o $$object; objects="$$objects $$object"; \
	  done; \
	  runtime=bin/libosprey_runtime_klee_$$backend.bc; \
	  $(KLEE_LINK) $$objects -o $$runtime; \
	  $(KLEE_LLVM_BIN)/opt -disable-simplify-libcalls -vectorize-loops=false -vectorize-slp=false -passes='default<O2>' $$runtime -o $$runtime; \
	  if [ "$$backend" = gc ]; then \
	    $(KLEE_LLVM_BIN)/opt -load-pass-plugin=bin/libosprey_gc_roots.so \
	      -passes=osprey-gc-roots -verify-each $$runtime -o $$runtime; fi; \
	  $(KLEE_LLVM_BIN)/opt -load-pass-plugin=bin/libosprey_gc_roots.so \
	      -passes=osprey-runtime-prepared -verify-each $$runtime -o $$runtime; \
	  $(MAKE) -C "$(CURDIR)" --no-print-directory -s _klee_closure \
	    RUNTIME="$$(pwd)/$$runtime" KLEE_BACKEND=$$backend; \
	done

# [SYMEX-MOCK-CLOSED] A closed KLEE module needs no symbol KLEE does not
# provide. RUNTIME is the linked bitcode; KLEE_BACKEND only labels the error.
# Each stage reports its own failure: a broken nm or awk is an error even
# though it prints no symbols, and only grep's exit 1 (no matches) means the
# closure is empty. A swallowing `|| true` here would accept an empty result
# from a tool that never ran.
_klee_closure:
	@set -e; status=0; symbols="$$("$(KLEE_NM)" --undefined-only "$(RUNTIME)")" \
	  || { echo "ERROR: $(KLEE_NM) failed on $(RUNTIME)"; exit 1; }; \
	  names="$$(printf '%s\n' "$$symbols" | awk '{print $$2}')" \
	  || { echo "ERROR: cannot read symbol names from $(RUNTIME)"; exit 1; }; \
	  open="$$(printf '%s\n' "$$names" | grep -Ev '$(KLEE_PROVIDED)')" || status=$$?; \
	  if [ "$$status" -gt 1 ]; then echo "ERROR: cannot filter provided symbols from $(RUNTIME)"; exit 1; fi; \
	  if [ -n "$$open" ]; then echo "ERROR: symbolic $(KLEE_BACKEND) runtime is not closed:"; echo "$$open"; exit 1; fi
