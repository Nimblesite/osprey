# The Windows apps' automated tests [WINUI-HOST-SELF-TEST] [WINUI-HOST-GOLDENS]: each app built,
# packaged and run in its sandbox by hosts/winui/app.ps1, driven by its self-test.json, and every
# picture compared with its screenshot goldens in <app>/goldens/windows/<display scale percent>.
# Full runs put the window on screen and check what the screen shows too; headless runs draw it
# off screen and check WinUI's own pictures. Run from PowerShell with GNU make on the PATH.
ROOT ?= ../../..
APP_PS1 ?= pwsh -NoProfile -File $(ROOT)/hosts/winui/app.ps1
APPS ?= storage flock painting signal-flow probe casting
# HEADLESS=-Headless runs a single app's target off screen; `headless` does it for every app.
HEADLESS ?=

.PHONY: full headless update $(APPS)

## full: Every app's self-test in a real window on screen, each picture compared with its goldens
##       and with what the screen shows. Keeps going past a failure: make -k full.
full: $(APPS)

## headless: Every app's self-test with its window off screen, each picture compared with its goldens.
headless:
	$(MAKE) full HEADLESS=-Headless

## update: Rerun every app on screen and rewrite its goldens for this display's scale. Look at each
##         picture in target/winui-shots before committing them.
update:
	$(foreach app,$(APPS),$(APP_PS1) -App $(app) -Test -Update &&) true

## storage: Storage's self-test (HEADLESS=-Headless: off screen); the same goes for
##          flock, painting, signal-flow, probe and casting.
$(APPS):
	$(APP_PS1) -App $@ -Test $(HEADLESS)
