# Graphics: settings and the platform hosts behind `make -C apps graphics` and
# `make -C apps graphics-smoke`, which the root Makefile documents.
GFX_DIR := examples/graphics
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
	@Write-Host "make -C apps graphics needs the MSYS2 UCRT64 shell -- see $(GFX_DIR)/README.md"
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

# GPT-2 settings and the one-time weight download behind gpt2 and gpt2-train.
GPT2_DIR    := examples/gpt2
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
	cd website && npm run dev

# _website-build: Build static site
_website-build: $(WEBSITE_NODE_DEPS)
	cd website && npm run build

# _test_bench_tools: Unit-test the benchmark result merger and the README figure
# generator. Every benchmark run stages and merges validated replacements, so
# the checks guard the recorded data set against failed or incomplete runs;
# `update_readme` is what keeps every
# quoted binarytrees figure in step with that data.
_test_bench_tools:
	@echo "==> Bench tooling tests..."
	python3 benchmarks/test_merge_results.py

## ui-test: The UI test framework's own suite and the apps it drives by accessible
##          label alone, casting and signal flow, with no host or window. [UI-MACHINE-TEST]
.PHONY: ui-test
UI_SUITES := packages/uitest/tests examples/casting/test packages/signal_flow/tests
ui-test: _runtime
	cargo build --release -p osprey-cli
	for suite in $(UI_SUITES); do $(BIN) test $$suite || exit 1; done
