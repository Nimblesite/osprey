# Each macOS app's tests on its real host [UI-MACHINE-SELF-TEST]. One target runs one app: <app> in a
# real window, <app>-headless with the same script and no window to see. Records go to target/.
# Tests with no host at all, the same on every platform: tests.
ROOT ?= ../../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile
GPUI_APPS := casting flock painting probe signal-flow

app_casting := src/osprey/examples/casting
script_casting := src/osprey/examples/casting
app_flock := src/osprey/examples/gpui/flock
script_flock := src/osprey/examples/gpui/flock
app_painting := src/osprey/examples/gpui/painting
script_painting := src/osprey/examples/gpui/painting
app_probe := src/toolchain/hosts/gpui
script_probe := src/toolchain/hosts/gpui/probe
app_signal-flow := src/osprey/packages/signal_flow/app
script_signal-flow := src/osprey/packages/signal_flow/app

.PHONY: $(GPUI_APPS) $(GPUI_APPS:%=%-headless) agent-headless agent-live agent-qwen examples graphics qwen qwen-headless storage-headless synth-headless tally tally-headless workbench _workbench_interactions _workbench_memory _workbench_linked

.PHONY: pocket-orchestra pocket-orchestra-headless
## pocket-orchestra-headless: Real native orchestra server and phone browser journeys, capturing PCM without an audio device.
pocket-orchestra-headless: _pocket-orchestra-ring
	bash src/toolchain/hosts/swift/orchestra/build.sh
	node src/toolchain/targets/macos/tests/pocket-orchestra.mjs

## pocket-orchestra: The same controller journeys with the native window, visible browsers and the Mac's audio device.
pocket-orchestra: _pocket-orchestra-ring
	bash src/toolchain/hosts/swift/orchestra/build.sh
	ORCHESTRA_DEVICE_TEST=1 node src/toolchain/targets/macos/tests/pocket-orchestra.mjs

# _pocket-orchestra-ring: Race the audio producer/callback under memory and undefined-behaviour sanitizers.
.PHONY: _pocket-orchestra-ring
_pocket-orchestra-ring:
	@mkdir -p target/orchestra
	clang -std=c11 -Wall -Wextra -Werror -fsanitize=address,undefined -g src/toolchain/targets/macos/tests/orchestra-ring.c src/toolchain/hosts/swift/orchestra/audio_ring.c -o target/orchestra/ring-test
	python3 -c 'import subprocess; subprocess.run(["target/orchestra/ring-test"], check=True, timeout=20)'

## tally-headless: Tally as a C library under its reference host, which seals each frame and reads
##                 every control in place from shared memory. [NATIVE-SHM-VIEW]
tally-headless:
	$(MAKE) -C $(ROOT) _tally-reference

## tally: Tally on the SwiftUI view host with no window: the host leases each frame, reads the
##        controls in place and checks every text, colour and size it would draw. [NATIVE-SHM-VIEW]
tally:
	$(MAKE) -C $(ROOT) _tally-swift VIEW_ARGS=--view-check

## casting: The casting player's self-test in a real GPUI window; casting-headless runs it with no
##          window. The same goes for flock, painting, probe and signal-flow.
$(GPUI_APPS):
	$(MAKE) -C $(ROOT) _gpui-app-self-test APP_DIR=$(app_$@) SCRIPT_DIR=$(script_$@) GPUI_WINDOW=

$(GPUI_APPS:%=%-headless): %-headless:
	$(MAKE) -C $(ROOT) _gpui-app-self-test APP_DIR=$(app_$*) SCRIPT_DIR=$(script_$*)

## examples: The iPhone C ABI fixture, the whole corpus against the same goldens and skip manifest,
##           and the storage example, on this Mac. The storage domain suite runs natively, so it needs
##           `_runtime`.
examples: _macos-examples _runtime
	$(MAKE) _test_macos

## graphics: Render every graphics scene for GFX_SMOKE_FRAMES frames and require each to report them.
##           Needs a desktop session and a Metal GPU.
graphics:
	$(MAKE) _graphics-smoke

## qwen: The Qwen server on the real weights and Metal: generate, chat with tools, the OpenAI route,
##       checkpoint reuse and the num_ctx limit, through the real ollama and openai clients; the GGUF is
##       pulled first if it is missing. Unload Ollama's resident model first.
qwen: _runtime
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	$(MAKE) -C $(ROOT) _qwen-lanes LANES=real

## qwen-headless: The Qwen model suite on the Metal device, equal to the golden byte for byte, with at
##                least one Metal dispatch completed (plan 0049 Phase 3).
qwen-headless: _runtime
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	$(MAKE) -C $(ROOT) _qwen-suite MEMORY=arc QWEN_GPU=metal

## daw-icon: Verify the macOS DAW launches from a bundle with the original Prism Dock icon.
.PHONY: daw-icon
daw-icon:
	bash src/toolchain/targets/macos/tests/prism-icon.sh

## daw-events-headless: The daw's event budget on the FerroUI host, with no window: dragging a Prism module
##                      and tapping Timeline lanes each answer within one 60 Hz display frame.
.PHONY: daw-events-headless
daw-events-headless: daw-icon _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	mkdir -p target/prism-app
	nice -n 10 $(BIN) src/osprey/examples/daw --compile --target=macos -o target/prism-app/libOspreyApp.a
	OSPREY_NATIVE_APP_DIR="$(CURDIR)/target/prism-app" OSPREY_RUNTIME_DIR="$(CURDIR)/src/toolchain/compiler/lib" \
	OSPREY_NATIVE_ABI="$(CURDIR)/src/toolchain/compiler/runtime/native_abi.h" \
	nice -n 10 cargo test --manifest-path src/toolchain/hosts/ferroui/Cargo.toml --release --features desktop \
	    --test prism_events --test daw_events -- --nocapture

## daw-playback-rate: Verify cold live rendering at 1.5× real time and report start time and longest block gap.
DAW_RATE_RENDERER ?= target/daw-profile/rate-renderer
DAW_RATE_SONG ?= src/osprey/examples/daw/tracks/codex-version.json
DAW_RATE_FROM ?= 0
DAW_RATE_BEATS ?= 2
.PHONY: daw-playback-rate
daw-playback-rate: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	mkdir -p target/daw-profile
	nice -n 10 $(BIN) build src/osprey/examples/daw/render --memory=arc -o "$(DAW_RATE_RENDERER)"
	node scripts/make/daw-playback-rate.mjs "$(DAW_RATE_RENDERER)" "$(DAW_RATE_SONG)" "$(DAW_RATE_FROM)" "$(DAW_RATE_BEATS)"

## daw-profile: Measure the daw and say where the time goes ([DAW-PERF]): milliseconds per kind of user action on
##              every page, then a 1 ms sample of the Timeline, Mix and Patch pages and of the song renderer, each
##              attributed to Osprey functions and the runtime calls they make. Reports in target/daw-profile.
.PHONY: daw-profile
daw-profile: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	bash scripts/make/daw-profile.sh

## daw-video: The daw in a real window, clicked and dragged like a user: a note's velocity and a Space send on the
##            Timeline, a Delay added and wired into a patch and auditioned, a send, bus and reverb on Mix, then
##            Play renders the edited bars. Each step must change the screen and Play must render audio. Writes target/daw-video/prism-journey.mp4 (with the bars as
##            its soundtrack) and the step screenshots. Needs screen recording and accessibility access. [DAW-VIDEO]
.PHONY: daw-video
daw-video: _runtime_macos
	$(MAKE) -C src/toolchain/targets/macos _daw-renderer
	bash scripts/make/daw-video.sh target/daw-video

## synth: Add, drag and connect module nodes, then close/reopen the keyboard in a native window.
.PHONY: synth
synth:
	$(MAKE) -C $(ROOT) _prism_ui_test GPUI_WINDOW=

## synth-headless: The same module and dock journey through the native host without a window.
synth-headless:
	$(MAKE) -C $(ROOT) _prism_ui_test GPUI_WINDOW=--headless

## storage-headless: The Mac storage app's smoke test through its Swift host.
# The Mac storage app through its Swift host.
storage-headless: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/macos/run.sh --smoke

## agent-headless: Osprey Agent's whole UI on the SwiftUI host, the model a test double: its process
##                 adapter's tests, a real coding task and every control, against its goldens.
# [UI-MACHINE-SELF-TEST] Osprey Agent's whole UI on the SwiftUI host, the model a test double, against its goldens.
agent-headless: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/agent/self-test.sh
	# The Agent's archive is linked, so the suites that drive the real Workbench run under `workbench`.
	swift test --package-path src/toolchain/hosts/swift --skip WorkbenchLayout

## agent-live: Osprey Agent in a real window with real DeepSeek calls, checked by acceptance tests the
##             agent never sees and an LLM judge. Spends tokens; the key is DEEPSEEKKEY in .env.
# The same script and harder tasks in a real window with real DeepSeek calls. Spends tokens.
agent-live: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/agent/self-test.sh --live

## agent-qwen: Local-only full E2E: actual Agent window and Osprey Qwen GGUF, real coding/tools,
##             independent project checks, screenshot goldens and direct Qwen judgement. No doubles.
agent-qwen: _runtime _runtime_macos_arc
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	$(MAKE) -C $(ROOT) _qwen-lanes LANES=ui

# The Workbench on the SwiftUI host with no window opens and draws a checked frame ([NATIVE-SHM-VIEW]),
# which also links its archive for the suites that drive the real Workbench.
_workbench_linked: _runtime _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_SWIFT_APP=src/osprey/workbench bash src/toolchain/hosts/swift/view.sh --open-check

# The 40,000-import memory soak: every import renders the whole app, so it has its own CI part.
WORKBENCH_SOAK = WorkbenchLayoutTests/testActualApplicationImportDispatchAndCancellationMemoryPlateau$$

## workbench: The Workbench opens and draws a checked frame with no window; then `0 failures` from all
##            suites that drive the real Workbench, including the import memory soak.
workbench: _workbench_linked
	# The rest of the host's suite runs once, under `agent-headless`.
	swift test --package-path src/toolchain/hosts/swift --filter WorkbenchLayout

# CI runs these two partitions in parallel, with every Workbench test in exactly one.
_workbench_interactions: _workbench_linked
	swift test --package-path src/toolchain/hosts/swift --filter WorkbenchLayout --skip '$(WORKBENCH_SOAK)'

# The real app imports lesson progress 20,000 times and cancels 20,000 reads;
# ARC objects, bytes and borrowed slices must stay at their baseline.
_workbench_memory: _workbench_linked
	swift test --package-path src/toolchain/hosts/swift --filter '$(WORKBENCH_SOAK)'
endif

.PHONY: synth-vst synth-vst-headless
## synth-vst: Prism's real VST discovery, parameter and routing journey in a native window.
synth-vst:
	$(MAKE) -C $(ROOT) _prism_vst_ui_test

## synth-vst-headless: The same Prism VST journey without a window, using real worker processes.
synth-vst-headless:
	$(MAKE) -C $(ROOT) _prism_vst_ui_test PRISM_TEST_MODE=--headless

## synth-vst-spectrum: Measure the real Dragonfly or TAL output in the built Prism host.
.PHONY: synth-vst-spectrum
synth-vst-spectrum:
	cd "$(ROOT)" && node src/toolchain/hosts/synth_vst/spectrum.mjs "$(VST3_FOLDER)" "$(or $(VST3_CASE),dragonfly)" "$(or $(VST3_UI),headless)"

## synth-vst-editors: Edit installed VST3 parameters and open, focus, close and reopen each native editor.
.PHONY: synth-vst-editors
synth-vst-editors:
	cd "$(ROOT)" && node src/toolchain/hosts/synth_vst/editors.mjs "$(VST3_FOLDER)"
