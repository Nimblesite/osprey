# The desktop apps on Windows, each its own MSIX package in an AppContainer on the WinUI host
# [WINUI-HOST-SANDBOX], as ../windows.json describes them. Run from PowerShell with GNU make on the
# PATH; hosts/winui/app.ps1 builds and registers each. macOS: apps/macos.
ROOT ?= ../..
APP_PS1 ?= pwsh -NoProfile -File $(ROOT)/hosts/winui/app.ps1

.PHONY: flock painting signal-flow test

## flock: Open Flock, the boids herding game, in its sandbox.
flock:
	$(APP_PS1) -App flock

## painting: Open the Osprey port of GPUI's painting example in its sandbox.
painting:
	$(APP_PS1) -App painting

## signal-flow: Open the signal-flow editor, a UI machine, in its sandbox.
signal-flow:
	$(APP_PS1) -App signal-flow

## test: Flock's, Painting's and Signal flow's self-tests, off screen inside their sandboxes,
##       each picture compared with the goldens in <app>/goldens/windows.
test:
	$(APP_PS1) -App flock -Test -Headless
	$(APP_PS1) -App painting -Test -Headless
	$(APP_PS1) -App signal-flow -Test -Headless
