# The desktop apps on macOS, each in the GPUI host. Recipes run from the repo root through scripts/make/forward.mk;
# `make help` at the root lists these too. Windows: windows.
# Each app's tests: src/toolchain/targets/macos/tests, and with no host at all, src/toolchain/targets/tests.
# The Flock and painting measurements: src/toolchain/benchmarks.
ROOT ?= ../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: agent bank casting examples flock graphics painting qwen-server qwen-server-quick qwen-client signal-flow prism daw-gpui daw-render daw-play storage tally probe

.PHONY: pocket-orchestra _pocket-orchestra-models
## pocket-orchestra: Open Light Orchestra: this Mac plays audio, phones join by QR and WebSocket.
pocket-orchestra: _runtime_macos
	$(COMPILER_BUILD)
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/toolchain/hosts/swift/orchestra/build.sh
	open "$(CURDIR)/target/orchestra/Light Orchestra.app"

.PHONY: _pocket-orchestra-quick
# _pocket-orchestra-quick: Build and open Light Orchestra with an existing compiler (OSPREY_BIN, default target/debug/osprey).
_pocket-orchestra-quick:
	bash src/toolchain/hosts/swift/orchestra/build.sh
	open "$(CURDIR)/target/orchestra/Light Orchestra.app"

# _pocket-orchestra-models: Generate the native audio models from typeDiagram; set TYPEDIAGRAM_DIST.
_pocket-orchestra-models:
	node scripts/make/osprey-schema.mjs "$(TYPEDIAGRAM_DIST)" src/toolchain/hosts/swift/orchestra/audio.td src/toolchain/hosts/swift/orchestra/AudioModels.swift orchestra

## agent: Open Osprey Agent, the coding agent on @osprey/native, on the SwiftUI host. PROJECT names the
##        folder it works in; the key comes from DEEPSEEKKEY in the repository's .env.
PROJECT ?= $(CURDIR)
agent:
	$(MAKE) -C $(ROOT) build _runtime_macos
	bash $(ROOT)/src/osprey/examples/agent/run.sh "$(PROJECT)"

## bank: Open Talon Bank, the same Osprey Native app as the web, Android and iPhone, in a SwiftUI window.
##       Start its API first (`make -C src/toolchain/targets/web bank`).
bank:
	OSPREY_SWIFT_APP=$(BANK_APP) bash src/toolchain/hosts/swift/view.sh

## tally: Open Tally, the counter whose whole interface crosses the bridge as shared memory, in a
##        SwiftUI window. Its look is decided in Osprey; the host holds no style. [NATIVE-SHM-VIEW]
tally:
	$(MAKE) -C $(ROOT) _tally-swift

## casting: Open the casting player: pick a file or stream and cast it to a TV
##          found on the network, with the HLS visualiser. [CAST-DESIGN-NATIVE]
casting: _runtime_macos _runtime
	$(COMPILER_BUILD)
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
	$(COMPILER_BUILD)
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/flock/run.sh

## painting: Open the Osprey port of GPUI's painting example.
painting: _runtime_macos
	$(COMPILER_BUILD)
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/gpui/painting/run.sh

## qwen-server: Rebuild the compiler and the runtime, then serve Qwen3.8-27B from the real weights on Metal at
##       http://127.0.0.1:11435 (QWEN_PORT), answering the Ollama and OpenAI routes; Ctrl-C stops it. The
##       GGUF is pulled first if it is missing (QWEN_QUANT, UD-Q4_K_XL by default). Unload Ollama's
##       resident model first (ollama stop <model>): on 36 GB the two do not fit together. [LLM-SERVING]
qwen-server: _runtime
	$(COMPILER_BUILD)
	$(MAKE) -C $(ROOT) _qwen-serve

## qwen-server-quick: The same server without rebuilding anything but the server: assumes
##                    target/release/osprey and the runtime are already fresh.
qwen-server-quick:
	$(MAKE) -C $(ROOT) _qwen-serve

## qwen-client: Chat with the running qwen-server (start it first, in another terminal) through the
##              ollama command: type a message, /bye to leave. Needs the ollama CLI (brew install ollama).
qwen-client:
	$(MAKE) -C $(ROOT) _qwen-client

## signal-flow: Open the signal-flow editor, a UI machine with the four canvas
##              skins of src/osprey/packages/signal_flow. [UI-MACHINE-FLOW]
signal-flow: _runtime_macos
	$(COMPILER_BUILD)
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/packages/signal_flow/app/run.sh

## daw-gpui: Open the daw's Prism patch studio on the GPUI host instead of FerroUI.
##        [PRISM-ROUTING-EDITOR] [PRISM-ROUTING-AUDIO]
daw-gpui: _runtime_macos
	$(COMPILER_BUILD)
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/daw/run.sh

## prism: Install Prism.app in /Applications so it opens from Spotlight. Set SONG to open a project after installing.
##        [DAW-PROJECT] [PRISM-ROUTING-TIMELINE]
prism: SONG =
prism: _runtime_macos _daw-renderer
	PRISM_PROJECT="$(SONG)" OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/toolchain/hosts/ferroui/scripts/prism.sh --install

## storage: Rebuild the compiler, the Mac runtime and the Osprey Storage app,
##          then open it: where your disk space goes, folder by folder.
storage: _runtime_macos
	$(COMPILER_BUILD)
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/macos/run.sh

## daw-render: Render a song project with the daw (a JSON piano roll, its intro recording named inside) with
##               @osprey/synth to target/daw/<project>/song.wav. SONG picks the project. Needs ffmpeg
##               and xxd; rendering is offline and slow. [SYNTH-ARRANGEMENT-PLAYER]
SONG ?= src/osprey/examples/daw/tracks/phosphor-drift.json
SONG_DIR = target/daw/$(basename $(notdir $(SONG)))
SONG_WAV = $(SONG_DIR)/song.wav
daw-render: _daw-renderer
	target/daw/daw "$(SONG)" "$(SONG_DIR)"

# Shared song renderer for the interactive DAW and the offline bounce; ARC, as GC scans of the held mix tripled render time.
.PHONY: _daw-renderer
_daw-renderer:
	$(COMPILER_BUILD)
	mkdir -p target/daw
	$(BIN) build src/osprey/examples/daw/render --memory=arc -o target/daw/daw

## daw-play: Play SONG's rendered WAV with afplay; it never renders (run daw-render first).
daw-play:
	@test -f "$(SONG_WAV)" || { echo "daw-play: $(SONG_WAV) is missing; run make -C src/toolchain/targets/macos daw-render"; exit 1; }
	timeout 3600 afplay "$(SONG_WAV)"

## probe: Open the GPUI host's probe application (counter, note, load).
probe: _runtime_macos
	$(COMPILER_BUILD)
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/toolchain/hosts/gpui/run.sh

endif
