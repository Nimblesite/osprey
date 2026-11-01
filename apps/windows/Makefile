# The desktop apps on Windows, each its own MSIX package in an AppContainer on the WinUI host
# [WINUI-HOST-SANDBOX], as ../windows.json describes them. Run from PowerShell with GNU make on the
# PATH; hosts/winui/app.ps1 builds and registers each. macOS: apps/macos.
ROOT ?= ../..
APP_PS1 ?= pwsh -NoProfile -File $(ROOT)/hosts/winui/app.ps1
# HEADLESS=-Headless draws the self-tests' windows off screen; by default they are on screen.
HEADLESS ?=

.PHONY: storage flock painting signal-flow probe test

## storage: Open Osprey Storage, where your disk space goes folder by folder, in its sandbox.
##          It reads the whole drive once Settings > Privacy & security > File system allows it.
storage:
	$(APP_PS1) -App storage

## flock: Open Flock, the boids herding game, in its sandbox.
flock:
	$(APP_PS1) -App flock

## painting: Open the Osprey port of GPUI's painting example in its sandbox.
painting:
	$(APP_PS1) -App painting

## signal-flow: Open the signal-flow editor, a UI machine with four canvas skins, in its sandbox.
signal-flow:
	$(APP_PS1) -App signal-flow

## probe: Open the hosts' probe application (counter, note, load) in its sandbox.
probe:
	$(APP_PS1) -App probe

## test: Every app's self-test in a real window inside its sandbox (HEADLESS=-Headless: off
##       screen), each picture compared with the goldens in <app>/goldens/windows.
test:
	$(APP_PS1) -App storage -Test $(HEADLESS)
	$(APP_PS1) -App flock -Test $(HEADLESS)
	$(APP_PS1) -App painting -Test $(HEADLESS)
	$(APP_PS1) -App signal-flow -Test $(HEADLESS)
	$(APP_PS1) -App probe -Test $(HEADLESS)
