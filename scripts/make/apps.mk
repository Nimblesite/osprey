# Graphics: settings and the platform hosts behind `make -C src/toolchain/targets/macos graphics` (or windows) and
# `make -C src/toolchain/targets/macos/tests graphics` (or src/toolchain/targets/windows/tests).
GFX_DIR := src/osprey/examples/graphics
GFX_WARN ?= -Wall -Werror
SCENE ?= kali

ifeq ($(OS),Windows_NT)
# Direct3D 12. Built with the same MinGW UCRT64 toolchain that builds the C
# runtime on Windows, so `// @link(windows): ospgfx` in base/base.osp resolves
# the import library below. The bridge compiles with gcc -Wall -Werror and all
# four shader entries compile with fxc; `make _graphics-smoke` renders every scene.
GFX_SHADER := $(GFX_DIR)/base.hlsl
GFX_SRC    := $(GFX_DIR)/ospgfx_d3d12.c $(GFX_DIR)/ospgfx_d3d12_setup.c $(GFX_DIR)/ospgfx_d3d12_window.c
GFX_DLL    := $(GFX_DIR)/ospgfx.dll
GFX_LIB    := $(GFX_DIR)/libospgfx.dll.a
GFX_SYSLIB := -ld3d12 -ldxgi -ld3dcompiler -ldxguid -luser32
# Every entry point the run-time compiler will be asked for, with its target
# profile. `make _graphics-shader` compiles all four so a typo in one scene's
# fragment cannot hide behind another scene running fine.
GFX_ENTRIES := osp_vertex:vs_5_0 osp_fragment:ps_5_0 \
	osp_fragment_opal:ps_5_0 osp_fragment_character:ps_5_0
GFX_SCENES := kali opal character
GFX_SMOKE_FRAMES ?= 60
# A scene that has not drawn its frames by then fails rather than holding the run.
GFX_SMOKE_SECONDS ?= 300
GFX_OUT := target/gfx

ifeq ($(MSYSTEM),)
# Native PowerShell has neither bash for the recipes below nor a MinGW cc.
_graphics _graphics-shader _graphics-smoke:
	@Write-Host "make -C src/toolchain/targets/windows graphics needs the MSYS2 UCRT64 shell -- see $(GFX_DIR)/README.md"
else
$(GFX_LIB): $(GFX_SRC) $(GFX_DIR)/ospgfx_d3d12.h $(GFX_DIR)/ospgfx_abi.h
	@echo "==> building Osprey -> Direct3D 12 graphics bridge"
	$(CC) -shared -O2 $(GFX_WARN) -o $(GFX_DLL) $(GFX_SRC) \
		-Wl,--out-implib,$(GFX_LIB) $(GFX_SYSLIB)

# The bridge compiles the shader at run time, so an error in it would otherwise
# only surface as a window that refuses to open. fxc is the same compiler
# D3DCompile is, so checking here checks exactly what the bridge will do. The
# Windows SDK ships it off PATH, so the newest SDK's copy is used when PATH has
# none, and the check skips only when neither exists.
_graphics-shader:
	@fxc=$$(command -v fxc.exe || ls -d "/c/Program Files (x86)/Windows Kits/10/bin/"10.*/x64/fxc.exe 2>/dev/null | sort -V | tail -1); \
	if [ -n "$$fxc" ]; then \
		for entry in $(GFX_ENTRIES); do \
			echo "==> checking $(GFX_SHADER) $${entry%%:*}"; \
			"$$fxc" -nologo -T $${entry##*:} -E $${entry%%:*} \
				-Fo $(GFX_DIR)/.shadercheck.cso $(GFX_SHADER) || exit 1; \
		done; \
		$(RM) $(GFX_DIR)/.shadercheck.cso; \
	else \
		echo "==> skipping shader check (no fxc.exe on PATH or in the Windows SDK)"; \
	fi

# The DLL lives beside its source rather than beside the compiled scene, so the
# loader is pointed at it for the length of the run.
_graphics: build $(GFX_LIB) _graphics-shader
	@echo "==> launching Osprey graphics demo: $(SCENE) (close the window to exit)"
	PATH="$(CURDIR)/$(GFX_DIR):$$PATH" ./$(BIN) $(GFX_DIR)/$(SCENE) --run

# _graphics-smoke: every scene compiled to $(GFX_OUT) with the compiler at $(BIN)
# and run for GFX_SMOKE_FRAMES frames on Direct3D 12; each must report them. A
# scene's exit status does not say whether it drew, so its report line decides.
# The window needs a desktop session: in session 0 the swap chain is refused.
_graphics-smoke: $(GFX_LIB) _graphics-shader
	@mkdir -p $(GFX_OUT)
	@for scene in $(GFX_SCENES); do \
		./$(BIN) $(GFX_DIR)/$$scene --compile --no-warnings -o $(GFX_OUT)/$$scene.exe || exit 1; \
		report=$$(PATH="$(CURDIR)/$(GFX_DIR):$$PATH" OSPREY_GFX_FRAMES=$(GFX_SMOKE_FRAMES) timeout $(GFX_SMOKE_SECONDS) $(GFX_OUT)/$$scene.exe) || exit 1; \
		echo "$$report"; \
		case "$$report" in *": $(GFX_SMOKE_FRAMES) frames in "*) ;; \
			*) echo "graphics-smoke: $$scene did not draw $(GFX_SMOKE_FRAMES) frames" >&2; exit 1 ;; esac; \
	done
endif

else
GFX_SHADER := $(GFX_DIR)/base.metal
GFX_SCENES := kali opal character
GFX_SMOKE_FRAMES ?= 60

# The bridge compiles the shader at run time, so a syntax error in it would
# otherwise only surface as a window that refuses to open. Check it up front
# when the Metal toolchain is installed, and skip quietly when it is not. Xcode
# 26 and later ship `metal` as a stub until the toolchain component downloads,
# so availability is asked of the compiler itself rather than of `xcrun --find`.
_graphics-shader:
	@if xcrun -sdk macosx metal --version >/dev/null 2>&1; then \
		echo "==> checking $(GFX_SHADER)"; \
		xcrun -sdk macosx metal -c $(GFX_SHADER) -o /dev/null; \
	else \
		echo "==> skipping shader check (no Metal toolchain; xcodebuild -downloadComponent MetalToolchain installs it)"; \
	fi

# The scene compiles for the Mac slice and links into the Swift Metal host in
# $(GFX_DIR)/macos, the way every macOS example joins Osprey and Swift.
# [IOS-TARGET-MACOS]
_graphics: build _runtime_macos _graphics-shader
	@echo "==> launching Osprey graphics demo: $(SCENE) (close the window to exit)"
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash $(GFX_DIR)/macos/run.sh $(SCENE)

# _graphics-smoke: every scene for GFX_SMOKE_FRAMES frames on Metal.
_graphics-smoke: build _runtime_macos _graphics-shader
	@for scene in $(GFX_SCENES); do \
		report=$$(OSPREY_GFX_FRAMES=$(GFX_SMOKE_FRAMES) OSPREY_BIN="$(CURDIR)/$(BIN)" \
			bash $(GFX_DIR)/macos/run.sh $$scene) || exit 1; \
		echo "$$report"; \
		case "$$report" in *": $(GFX_SMOKE_FRAMES) frames in "*) ;; \
			*) echo "graphics-smoke: $$scene did not draw $(GFX_SMOKE_FRAMES) frames" >&2; exit 1 ;; esac; \
	done
endif

## _bank-web: Regenerate Talon Bank's embedded React host and Osprey WebAssembly client. Requires Node and
##            a WASI sysroot; the generated Osprey Bundle is committed so native and CI builds need neither.
.PHONY: _bank-web
_bank-web: export PATH := $(WASM_PATH_PREFIX)$(PATH)
_bank-web: build _runtime_wasm $(BANK_WEB_NODE_DEPS)
	@echo "==> Building Talon Bank browser application..."
	cd src/osprey/examples/projects/modules/web && npm run build

# GPT-2 settings and the one-time weight download behind gpt2 and gpt2-train.
GPT2_DIR    := src/osprey/examples/llms/gpt2
GPT2_SOURCE := https://huggingface.co/openai-community/gpt2/resolve/main
PROMPT      ?= The capital of France is
TOKENS      ?= 20
BACKEND     ?= gpu
GPU         ?= auto
FLAVOR      ?= default
GPT2_GENERATOR := $(GPT2_DIR)/$(if $(filter python,$(FLAVOR)),python/generate,generate)

$(GPT2_DIR)/weights/%:
	@$(MKDIR) $(GPT2_DIR)/weights
	@echo "==> downloading $* from openai-community/gpt2"
	curl -fsSL --retry 3 -o $@ $(GPT2_SOURCE)/$*

# _uninstall: Remove osprey + runtime archives from the system
_uninstall:
	cargo uninstall osprey-cli 2>/dev/null || true
	sudo rm -f /usr/local/lib/libfiber_runtime.a /usr/local/lib/libhttp_runtime.a
	@echo "==> uninstalled."

# _website-dev: Start local website development server
_website-dev: $(WEBSITE_NODE_DEPS)
	cd src/toolchain/website && npm run dev

# _website-build: Build static site
_website-build: $(WEBSITE_NODE_DEPS)
	cd src/toolchain/website && npm run build

# _test_bench_tools: Unit-test the benchmark result merger and the README figure
# generator. Every benchmark run stages and merges validated replacements, so
# the checks guard the recorded data set against failed or incomplete runs;
# `update_readme` is what keeps every
# quoted binarytrees figure in step with that data.
_test_bench_tools:
	@echo "==> Bench tooling tests..."
	python3 src/toolchain/benchmarks/test_merge_results.py
	python3 src/toolchain/benchmarks/test_instruction_gate.py

## _gpui-self-test: Every desktop app's self-test.json, the script the WinUI host runs in a real
##                  window, on the GPUI host. Without a window by default; GPUI_WINDOW= opens one.
##                  [UI-MACHINE-SELF-TEST]
.PHONY: _gpui-self-test
GPUI_WINDOW ?= --headless
GPUI_SCRIPTED := src/toolchain/hosts/gpui:src/toolchain/hosts/gpui/probe src/osprey/examples/gpui/flock:src/osprey/examples/gpui/flock src/osprey/examples/gpui/painting:src/osprey/examples/gpui/painting src/osprey/packages/signal_flow/app:src/osprey/packages/signal_flow/app src/osprey/examples/casting:src/osprey/examples/casting
_gpui-self-test: _gpui-apps
	for pair in $(GPUI_SCRIPTED); do \
		app=$${pair%%:*}; script=$${pair##*:}; \
		OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_GPUI_PREBUILT=1 bash $$app/run.sh --self-test "$(CURDIR)/$$script/self-test.json" "$(CURDIR)/target/gpui-self-test/$$(basename $$script)" $(GPUI_WINDOW) || exit 1; \
	done

## _gpui-app-self-test: One app's self-test.json on the GPUI host. [UI-MACHINE-SELF-TEST]
.PHONY: _gpui-app-self-test
_gpui-app-self-test: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash $(APP_DIR)/run.sh --self-test "$(CURDIR)/$(SCRIPT_DIR)/self-test.json" "$(CURDIR)/target/gpui-self-test/$(notdir $(SCRIPT_DIR))" $(GPUI_WINDOW)

## _app-suite: One app's host-free suites, named by SUITES.
.PHONY: _app-suite
_app-suite: _runtime
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	for suite in $(SUITES); do $(BIN) test $$suite || exit 1; done

## _tally-reference: Compile Tally to the Mac's C ABI archive, link it under its reference host and
##                   run that host: it drives every control and reads each view from the shared frame.
.PHONY: _tally-reference
_tally-reference: _runtime _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	mkdir -p target/tally
	$(BIN) src/osprey/examples/tally --compile --target=macos --memory=arc --quiet --no-warnings -o target/tally/libTally.a
	$(CC) -Wall -Wextra -Werror -Isrc/toolchain/compiler/runtime -Itarget/tally src/osprey/examples/tally/host/reference.c target/tally/libTally.a -o target/tally/reference
	target/tally/reference

## _tally-swift: Build Tally under the SwiftUI view host and run it; VIEW_ARGS=--view-check checks it
##               with no window.
.PHONY: _tally-swift
_tally-swift: _runtime _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_SWIFT_APP=src/osprey/examples/tally bash src/toolchain/hosts/swift/view.sh $(VIEW_ARGS)

## _qwen-serve: Compile the Qwen server and serve the GGUF pull.sh names (pulled first if missing,
##              QWEN_QUANT picks the quant) on Metal at QWEN_PORT. Uses the compiler and runtime as built.
## _qwen-suite: The Qwen model suite under MEMORY, on the device QWEN_GPU names (the host when empty),
##              its stdout equal to the golden byte for byte; on metal at least one Metal dispatch must
##              complete, or the run proves nothing about the device.
## _qwen-lanes: The Qwen server suite's LANES (gc, arc: the fixture table; real: the pulled GGUF) through
##              the real ollama and openai clients in QWEN_PYTHON's environment.
## _qwen-client: The ollama command's interactive chat against the server at QWEN_PORT.
QWEN_DIR := src/osprey/examples/llms/qwen
QWEN_OUT := target/llms/qwen-model
QWEN_SERVER := target/llms/qwen-server/server-arc
QWEN_PORT ?= 11435
QWEN_MODEL := qwen3.8:27b
QWEN_PYTHON ?= $(HOME)/.cache/osprey-llm/oracle-venv/bin/python
QWEN_RUN = $(QWEN_OUT)/$(or $(QWEN_GPU),host)-$(MEMORY)
.PHONY: _qwen-serve _qwen-suite _qwen-lanes _qwen-client
_qwen-serve:
	mkdir -p $(dir $(QWEN_SERVER))
	./$(BIN) $(QWEN_DIR)/server --compile --memory=arc -o $(QWEN_SERVER)
	GGUF=$$(sh $(QWEN_DIR)/pull.sh) && printf '%s\n%s\n' "$(QWEN_PORT)" "$$GGUF" | OSPREY_GPU=metal $(QWEN_SERVER)

_qwen-client:
	OLLAMA_HOST=127.0.0.1:$(QWEN_PORT) ollama run $(QWEN_MODEL)

_qwen-suite:
	mkdir -p $(QWEN_OUT)
	$(if $(QWEN_GPU),OSPREY_GPU=$(QWEN_GPU)) $(BIN) $(QWEN_DIR)/model/tests --run --quiet --memory=$(MEMORY) > $(QWEN_RUN).out 2> $(QWEN_RUN).err
	cmp $(QWEN_RUN).out $(QWEN_DIR)/model/tests/expected_output.txt
	test "$(QWEN_GPU)" != metal || grep -q '^\[gpu\] metal:.*completed dispatch' $(QWEN_RUN).err || { echo 'qwen: no Metal dispatch completed; the suite ran on the host' >&2; exit 1; }

_qwen-lanes:
	$(if $(filter real pydantic ui,$(LANES)),GGUF=$$(sh $(QWEN_DIR)/pull.sh) &&) OSPREY_QWEN_GGUF="$$GGUF" OSPREY_COMPILER="$(CURDIR)/$(BIN)" $(QWEN_PYTHON) -I $(QWEN_DIR)/server/tests/clients.cpython $(LANES)

## _ui-test: The UI test framework's own suite and the apps it drives by accessible
##           label alone, casting, signal flow and the synth, with no host or window. [UI-MACHINE-TEST]
.PHONY: _ui-test
UI_SUITES := src/osprey/packages/uitest/tests src/osprey/examples/casting/test src/osprey/packages/signal_flow/tests src/osprey/examples/synth/test
_ui-test: _runtime
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	for suite in $(UI_SUITES); do $(BIN) test $$suite || exit 1; done

.PHONY: _gpui-test _gpui-smoke 

_gpui-test: _runtime_macos _runtime
	python3 src/toolchain/hosts/gpui/dependencies.py
	python3 src/toolchain/hosts/gpui/dependencies_test.py
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	for suite in src/osprey/packages/native/tests src/osprey/packages/signal_flow/tests src/osprey/examples/synth/test src/osprey/examples/gpui/flock/test src/osprey/examples/gpui/painting/test; do \
		$(BIN) test $$suite || exit 1; \
	done
	$(MAKE) _gpui-smoke _gpui-self-test
	cd src/toolchain/hosts/gpui && cargo test --release && cargo clippy --release -- -D warnings

# Compile every app's archive once for the smoke and self-test targets. The synth wrapper bundles
# and opens the app after its inner run.sh returns, so compile its archive directly instead.
.PHONY: _gpui-apps
_gpui-apps:
	pids=; \
	for app in src/toolchain/hosts/gpui src/osprey/examples/gpui/flock src/osprey/examples/gpui/painting src/osprey/packages/signal_flow/app src/osprey/examples/casting; do \
		OSPREY_BIN="$(CURDIR)/$(BIN)" bash $$app/run.sh --compile & \
		pids="$$pids $$!"; \
	done; \
	OSPREY_GPUI_APP="$(CURDIR)/src/osprey/examples/synth" OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/toolchain/hosts/gpui/run.sh --compile & \
	pids="$$pids $$!"; \
	status=0; \
	for pid in $$pids; do wait $$pid || status=1; done; \
	exit $$status

# [UI-MACHINE-SELF-TEST] the scripts run in `_gpui-self-test`.
# Every desktop app started in the GPUI host without a window, its smoke.jsonl replayed through the
# real C boundary; the probe's has no script and taps its Add button. [GPUI-HOST-VERIFICATION]
_gpui-smoke: _gpui-apps
	for app in src/toolchain/hosts/gpui src/osprey/examples/gpui/flock src/osprey/examples/gpui/painting src/osprey/packages/signal_flow/app src/osprey/examples/synth src/osprey/examples/casting; do OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_GPUI_PREBUILT=1 bash $$app/run.sh --smoke || exit 1; done
