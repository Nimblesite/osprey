# The desktop apps on macOS, each in the GPUI host. Recipes run from the repo root through scripts/make/forward.mk;
# `make help` at the root lists these too. Windows: windows.
# Each app's tests: macos/tests, and with no host at all, tests.
# The Flock and painting measurements: src/toolchain/benchmarks.
ROOT ?= ..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: agent casting examples flock graphics painting qwen qwen-quick signal-flow synth storage probe workbench-render-review

## agent: Open Osprey Agent, the coding agent on @osprey/native, on the SwiftUI host. PROJECT names the
##        folder it works in; the key comes from DEEPSEEKKEY in the repository's .env.
PROJECT ?= $(CURDIR)
agent:
	$(MAKE) -C $(ROOT) build _runtime_macos
	bash $(ROOT)/src/osprey/examples/agent/run.sh "$(PROJECT)"

## casting: Open the casting player: pick a file or stream and cast it to a TV
##          found on the network, with the HLS visualiser. [CAST-DESIGN-NATIVE]
casting: _runtime_macos _runtime
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/casting/run.sh

## examples: Build the Mac runtime, the compiler and every macOS example: the storage app and the
##           three graphics scenes, each a Swift host linked to the Mac slice. Requires an Apple silicon
##           Mac with Xcode. [IOS-TARGET-MACOS]
examples: _macos-examples

## graphics: Build the Metal graphics host and run the animated GPU demo. Osprey drives the scene; the
##           GPU shades every pixel through Metal in a Swift host linked to the Mac slice, and the Osprey
##           sources are the same files Windows draws through Direct3D 12. SCENE=kali|opal|character picks
##           the scene; GFX_WARN overrides the C warning set. See src/osprey/examples/graphics/README.md.
graphics:
	$(MAKE) _graphics

## flock: Open Flock, the boids herding game. [GPUI-HOST-SCENE]
flock: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/flock/run.sh

## painting: Open the Osprey port of GPUI's painting example.
painting: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/painting/run.sh

## qwen: Rebuild the compiler and the runtime, then serve Qwen3.8-27B from the real weights on Metal at
##       http://127.0.0.1:11435 (QWEN_PORT), answering the Ollama and OpenAI routes; Ctrl-C stops it. The
##       GGUF is pulled first if it is missing (QWEN_QUANT, UD-Q4_K_XL by default). Unload Ollama's
##       resident model first (ollama stop <model>): on 36 GB the two do not fit together. [LLM-SERVING]
qwen: _runtime
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	$(MAKE) -C $(ROOT) _qwen-serve

## qwen-quick: The same server without rebuilding anything but the server: assumes target/release/osprey
##             and the runtime are already fresh.
qwen-quick:
	$(MAKE) -C $(ROOT) _qwen-serve

## signal-flow: Open the signal-flow editor, a UI machine with the four canvas
##              skins of src/osprey/packages/signal_flow. [UI-MACHINE-FLOW]
signal-flow: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/packages/signal_flow/app/run.sh

## synth: Open the Prism synth: the signal-flow editor's Prism skin with a
##        keyboard that plays the patch through the speakers. [UI-MACHINE-FLOW-SYNTH]
synth: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/synth/run.sh

## storage: Rebuild the compiler, the Mac runtime and the Osprey Storage app,
##          then open it: where your disk space goes, folder by folder.
storage: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/macos/run.sh

## probe: Open the GPUI host's probe application (counter, note, load).
probe: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/toolchain/hosts/gpui/run.sh

## workbench-render-review: Render a captured region tree natively (full page).
##   TREE defaults to the browser suite's slate board; expect a 1488x1102 image.
##   Matching dimensions do not prove pixel fidelity.
WORKBENCH_TREE ?= target/workbench-vsix/slate-board.native.json
workbench-render-review:
	OSPREY_RENDER_REVIEW_FULL_PAGE=1 bash src/osprey/workbench/hosts/apple/render-review.sh $(WORKBENCH_TREE) 1488 1058 target/native-slate.png
endif
