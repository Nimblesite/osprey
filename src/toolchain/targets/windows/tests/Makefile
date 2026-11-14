# Each Windows app's tests [WINUI-HOST-SELF-TEST] [WINUI-HOST-GOLDENS]: the app built, packaged and
# run in its sandbox by src/toolchain/hosts/winui/app.ps1, driven by its self-test.json, and every picture compared
# with its screenshot goldens in <app>/goldens/windows/<display scale percent>. One target runs one
# app. Run from PowerShell with GNU make on the PATH. Tests with no host at all, the same on every
# platform: tests.
ROOT ?= ../../../../..
APP_PS1 ?= pwsh -NoProfile -File $(ROOT)/src/toolchain/hosts/winui/app.ps1
APPS := casting flock painting probe signal-flow storage tally

.PHONY: $(APPS) $(APPS:%=%-headless) $(APPS:%=%-update) graphics

## graphics: Render every graphics scene for GFX_SMOKE_FRAMES frames through Direct3D 12 and require
##           each to report them. Needs a desktop session.
graphics:
	$(MAKE) -C $(ROOT) _graphics-smoke

## storage: Storage's self-test in a real window on screen, each picture compared with its goldens and
##          with what the screen shows. The same goes for casting, flock, painting, probe, signal-flow and tally.
$(APPS):
	$(APP_PS1) -App $@ -Test

## storage-headless: Storage's self-test with its window off screen, each picture compared with its
##                   goldens. The same goes for every app above.
$(APPS:%=%-headless): %-headless:
	$(APP_PS1) -App $* -Test -Headless

## storage-update: Rerun Storage on screen and rewrite its goldens for this display's scale. Look at
##                 each picture in target/winui-shots before committing. The same goes for every app.
$(APPS:%=%-update): %-update:
	$(APP_PS1) -App $* -Test -Update
