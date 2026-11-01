# The desktop apps on macOS, each in the GPUI host. Each target runs its recipe in the root Makefile,
# where it is a private `_` target; `make help` at the root lists these too. Windows: apps/windows.
ROOT ?= ../..

.PHONY: casting flock painting signal-flow synth storage probe bench compare test

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

## synth: Open the Prism synth: the signal-flow editor's Prism skin playing each
##        edit through the speakers. [UI-MACHINE-FLOW-SYNTH]
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

## test: The native and signal-flow packages, the synth, Flock and painting
##       suites, the host's unit tests and clippy, and the probe and Flock
##       recorded-event smokes.
test:
	$(MAKE) -C $(ROOT) _gpui-test
