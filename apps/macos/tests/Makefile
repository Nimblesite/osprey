# Each macOS app's tests on its real host [UI-MACHINE-SELF-TEST]. One target runs one app: <app> in a
# real window, <app>-headless with the same script and no window to see. Records go to target/.
# Tests with no host at all, the same on every platform: apps/tests.
ROOT ?= ../../..
GPUI_APPS := casting flock painting probe signal-flow

app_casting := examples/casting
script_casting := examples/casting
app_flock := examples/gpui/flock
script_flock := examples/gpui/flock
app_painting := examples/gpui/painting
script_painting := examples/gpui/painting
app_probe := hosts/gpui
script_probe := hosts/gpui/probe
app_signal-flow := packages/signal_flow/app
script_signal-flow := packages/signal_flow/app

.PHONY: $(GPUI_APPS) $(GPUI_APPS:%=%-headless) agent-headless agent-live storage-headless synth-headless

## casting: The casting player's self-test in a real GPUI window; casting-headless runs it with no
##          window. The same goes for flock, painting, probe and signal-flow.
$(GPUI_APPS):
	$(MAKE) -C $(ROOT) _gpui-app-self-test APP_DIR=$(app_$@) SCRIPT_DIR=$(script_$@) GPUI_WINDOW=

$(GPUI_APPS:%=%-headless): %-headless:
	$(MAKE) -C $(ROOT) _gpui-app-self-test APP_DIR=$(app_$*) SCRIPT_DIR=$(script_$*)

## synth-headless: The synth's recorded events through the GPUI host's C boundary, with no window.
synth-headless:
	$(MAKE) -C $(ROOT) _gpui-synth-smoke

## storage-headless: The Mac storage app's smoke test through its Swift host.
storage-headless:
	$(MAKE) -C $(ROOT) _storage-smoke

## agent-headless: Osprey Agent's whole UI on the SwiftUI host, the model a test double: its process
##                 adapter's tests, a real coding task and every control, against its goldens.
agent-headless:
	$(MAKE) -C $(ROOT) _agent-self-test

## agent-live: Osprey Agent in a real window with real DeepSeek calls, checked by acceptance tests the
##             agent never sees and an LLM judge. Spends tokens; the key is DEEPSEEKKEY in .env.
agent-live:
	$(MAKE) -C $(ROOT) _agent-live-test
