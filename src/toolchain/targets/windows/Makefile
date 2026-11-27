# The desktop apps on Windows, each its own MSIX package in an AppContainer on the WinUI host
# [WINUI-HOST-SANDBOX], as apps.json beside this file describes them. Run from PowerShell with GNU make
# on the PATH; src/toolchain/hosts/winui/app.ps1 builds and registers each. macOS: macos.
# Each app's tests: src/toolchain/targets/windows/tests, and with no host at all, src/toolchain/targets/tests.
ROOT ?= ../../../..
APP_PS1 ?= pwsh -NoProfile -File $(ROOT)/src/toolchain/hosts/winui/app.ps1

.PHONY: agent bank storage flock graphics painting signal-flow probe casting tally

## graphics: Build the Direct3D 12 graphics host and run the animated GPU demo from the MSYS2 UCRT64
##           shell. The Osprey sources are the same files macOS draws through Metal; SCENE=kali|opal|character
##           picks the scene. See src/osprey/examples/graphics/README.md.
graphics:
	$(MAKE) -C $(ROOT) _graphics

## agent: Open Osprey Agent, the coding agent on @osprey/native, working in PROJECT (default: here).
##        It runs unsandboxed, as it runs commands in that folder; its README says why. Add a key in Settings.
PROJECT ?= $(CURDIR)
agent:
	$(APP_PS1) -App agent -Arguments "$(PROJECT)"

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

## casting: Open the casting player in its sandbox: it finds Cast devices on the private network
##          with its DnsServiceBrowse helper and plays an MP3 on one. Needs MSYS2's OpenSSL.
casting:
	$(APP_PS1) -App casting

## bank: Open Talon Bank, the same Osprey Native app as the web, Android and iPhone, in its sandbox. Start its
##       API first (`make -C src/toolchain/targets/web bank`); the package is exempted to reach this machine.
bank:
	$(APP_PS1) -App talon

## tally: Open Tally, the shared-memory view example ([NATIVE-SHM-VIEW]), in its sandbox.
tally:
	$(APP_PS1) -App tally
