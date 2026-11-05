# The desktop apps on macOS, each in the GPUI host. Each target runs its recipe in the root Makefile,
# where it is a private `_` target; `make help` at the root lists these too. Windows: src/toolchain/targets/windows.
# Each app's tests: src/toolchain/targets/macos/tests, and with no host at all, src/toolchain/targets/tests.
ROOT ?= ../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: agent casting flock painting signal-flow synth storage probe bench compare

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

## flock: Open Flock, the boids herding game. [GPUI-HOST-SCENE]
flock: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/flock/run.sh

## painting: Open the Osprey port of GPUI's painting example.
painting: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/painting/run.sh

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

## bench: Run Flock for 20 seconds and print fps, dispatch, paint and envelope
##        size. Keep the window in front.
bench: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/flock/run.sh --bench 20

## compare: Pixel-diff the painting port against GPUI's original, with and
##          without synthetic strokes. Needs Screen Recording and Accessibility
##          permission for the terminal.
compare: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/painting/reference/compare.sh
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/painting/reference/compare.sh --draw
endif
