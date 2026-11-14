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

## synth-headless: The synth's recorded events through the GPUI host's C boundary, with no window.
# The synth's recorded events through the GPUI host, with no window.
synth-headless: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/synth/run.sh --smoke

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
