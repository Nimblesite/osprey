# The desktop apps on macOS, each in the GPUI host. Each target runs its recipe in the root Makefile,
# where it is a private `_` target; `make help` at the root lists these too. Windows: apps/windows.
# Each app's tests: apps/macos/tests, and with no host at all, apps/tests.
ROOT ?= ../..

.PHONY: agent casting flock painting signal-flow synth storage probe bench compare

## agent: Open Osprey Agent, the coding agent on @osprey/native, on the SwiftUI host. PROJECT names the
##        folder it works in; the key comes from DEEPSEEKKEY in the repository's .env.
PROJECT ?= $(CURDIR)
agent:
	$(MAKE) -C $(ROOT) build _runtime_macos
	bash $(ROOT)/examples/agent/run.sh "$(PROJECT)"

## casting: Open the casting player: pick a file or stream and cast it to a TV
##          found on the network, with the HLS visualiser. [CAST-DESIGN-NATIVE]
casting:
	$(MAKE) -C $(ROOT) _casting

## flock: Open Flock, the boids herding game. [GPUI-HOST-SCENE]
flock:
	$(MAKE) -C $(ROOT) _gpui-flock

## painting: Open the Osprey port of GPUI's painting example.
painting:
	$(MAKE) -C $(ROOT) _gpui-painting

## signal-flow: Open the signal-flow editor, a UI machine with the four canvas
##              skins of packages/signal_flow. [UI-MACHINE-FLOW]
signal-flow:
	$(MAKE) -C $(ROOT) _gpui-signal-flow

## synth: Open the Prism synth: the signal-flow editor's Prism skin with a
##        keyboard that plays the patch through the speakers. [UI-MACHINE-FLOW-SYNTH]
synth:
	$(MAKE) -C $(ROOT) _gpui-synth

## storage: Rebuild the compiler, the Mac runtime and the Osprey Storage app,
##          then open it: where your disk space goes, folder by folder.
storage:
	$(MAKE) -C $(ROOT) _run-mac-hd

## probe: Open the GPUI host's probe application (counter, note, load).
probe:
	$(MAKE) -C $(ROOT) _gpui-probe

## bench: Run Flock for 20 seconds and print fps, dispatch, paint and envelope
##        size. Keep the window in front.
bench:
	$(MAKE) -C $(ROOT) _gpui-bench

## compare: Pixel-diff the painting port against GPUI's original, with and
##          without synthetic strokes. Needs Screen Recording and Accessibility
##          permission for the terminal.
compare:
	$(MAKE) -C $(ROOT) _gpui-compare
