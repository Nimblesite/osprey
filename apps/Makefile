# Every app: open it, build it, test it. The desktop apps are per OS, in apps/macos and apps/windows. Each target runs its recipe in the root Makefile, where it is the
# private `_` target of the same name; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ..

.PHONY: python-app-ports bank bank-web bank-test bank-e2e graphics graphics-smoke gpt2 gpt2-train ios ios-test macos macos-test mobile-ios mobile-ios-test mobile-domain-test mobile-test android android-test bank-mobile-domain-test bank-android bank-android-test bank-ios bank-ios-test bank-mobile-test workbench-proof-assets workbench-test workbench-native-test workbench-render-review

## python-app-ports: Verify the classifier, vector search and text evaluation ports.
python-app-ports:
	$(MAKE) -C $(ROOT) _python-app-ports

## bank: Rebuild and run the Talon Bank showcase for manual testing.
##       Opens http://127.0.0.1:18790 (dashboard) / /api/accounts (JSON API).
##       The hold marker keeps the server up; Ctrl-C removes it and exits.
bank:
	$(MAKE) -C $(ROOT) _bank

## bank-web: Regenerate the embedded React host + Osprey WebAssembly client.
##           Requires Node and a WASI sysroot; the generated Osprey Bundle is
##           committed so ordinary native/CI builds do not need either tool.
bank-web:
	$(MAKE) -C $(ROOT) _bank-web

## bank-test: Native Osprey unit tests for the Talon Bank pure domain layer,
##            run through the built-in `osprey test` harness (TAP output).
bank-test:
	$(MAKE) -C $(ROOT) _bank-test

## bank-e2e: Browser end-to-end tests for the Talon Bank modules showcase
##           (examples/projects/modules) — real Chromium via Playwright drives
##           the compiled osprey binary serving its HTTP API and web UI.
##           The server's hold marker must not outlive the run: a leaked one
##           makes every later bank-*-test refuse to start.
bank-e2e:
	$(MAKE) -C $(ROOT) _bank-e2e

## graphics: Build the platform graphics host and run the animated GPU demo
#            Osprey drives the scene; the GPU shades every pixel. macOS renders
#            it through Metal in a Swift host linked to the Mac slice, Windows
#            through a Direct3D 12 DLL linked to the native executable — and the
#            Osprey sources are the same files on both, which is the entire point.
#            SCENE=kali|opal|character selects one of the three scenes; every
#            one is a one-file project sharing examples/graphics/base/base.osp
#            and one named fragment entry in the platform shader library.
#            GFX_WARN overrides the C warning set. See $(GFX_DIR)/README.md.
graphics:
	$(MAKE) -C $(ROOT) _graphics

## graphics-smoke: Render every scene for GFX_SMOKE_FRAMES frames and require
##                 each to report them. Needs a desktop session and a Metal GPU.
graphics-smoke:
	$(MAKE) -C $(ROOT) _graphics-smoke

## gpt2: Run GPT-2 (usage: make gpt2 FLAVOR=default|python PROMPT="..." TOKENS=20 BACKEND=gpu|lists)
## BACKEND=gpu runs the model on the GPU surface: every kernel and product on GPU=auto|metal|opencl, or on the CPU with GPU=host; BACKEND=lists runs the same model over host lists.
## The published weights and merges download once into examples/gpt2/weights (ignored by git).
gpt2:
	$(MAKE) -C $(ROOT) _gpt2

## gpt2-train: One training step of GPT-2 on the prompt: loss, gradients, SGD, loss again (usage: make gpt2-train PROMPT="..." BACKEND=gpu|lists)
gpt2-train:
	$(MAKE) -C $(ROOT) _gpt2-train

## ios: Build the iPhone and simulator libraries and SwiftUI example apps.
##      Requires macOS, Xcode and both iOS SDKs. Device output is unsigned.
ios:
	$(MAKE) -C $(ROOT) _ios

## ios-test: Validate C ABI calls, language goldens and the SwiftUI simulator app.
ios-test:
	$(MAKE) -C $(ROOT) _ios-test

## macos: Build the Mac runtime, the compiler and every macOS example: the
##        storage app and the three graphics scenes, each a Swift host
##        linked to the Mac slice. Requires an Apple silicon Mac with Xcode.
##        [IOS-TARGET-MACOS]
macos:
	$(MAKE) -C $(ROOT) _macos

## macos-test: The iPhone C ABI fixture, the whole corpus against the same
##             goldens and skip manifest, and the storage example, on this Mac.
##             The storage domain suite runs natively, so it needs `_runtime`.
macos-test:
	$(MAKE) -C $(ROOT) _macos-test

## mobile-ios: see the root Makefile's `_mobile-ios`.
mobile-ios:
	$(MAKE) -C $(ROOT) _mobile-ios

## mobile-ios-test: see the root Makefile's `_mobile-ios-test`.
mobile-ios-test:
	$(MAKE) -C $(ROOT) _mobile-ios-test

## mobile-domain-test: see the root Makefile's `_mobile-domain-test`.
mobile-domain-test:
	$(MAKE) -C $(ROOT) _mobile-domain-test

## mobile-test: see the root Makefile's `_mobile-test`.
mobile-test:
	$(MAKE) -C $(ROOT) _mobile-test

## android: see the root Makefile's `_android`.
android:
	$(MAKE) -C $(ROOT) _android

## android-test: see the root Makefile's `_android-test`.
android-test:
	$(MAKE) -C $(ROOT) _android-test

## bank-mobile-domain-test: see the root Makefile's `_bank-mobile-domain-test`.
bank-mobile-domain-test:
	$(MAKE) -C $(ROOT) _bank-mobile-domain-test

## bank-android: see the root Makefile's `_bank-android`.
bank-android:
	$(MAKE) -C $(ROOT) _bank-android

## bank-android-test: see the root Makefile's `_bank-android-test`.
bank-android-test:
	$(MAKE) -C $(ROOT) _bank-android-test

## bank-ios: see the root Makefile's `_bank-ios`.
bank-ios:
	$(MAKE) -C $(ROOT) _bank-ios

## bank-ios-test: see the root Makefile's `_bank-ios-test`.
bank-ios-test:
	$(MAKE) -C $(ROOT) _bank-ios-test

## bank-mobile-test: see the root Makefile's `_bank-mobile-test`.
bank-mobile-test:
	$(MAKE) -C $(ROOT) _bank-mobile-test

## workbench-proof-assets: Stage and byte-check the 13 bundled proof exports and
##                         2 upstream licences; any extra, missing or changed file fails.
workbench-proof-assets:
	$(MAKE) -C $(ROOT) _workbench-proof-assets

## workbench-test: Imported-mathematics cases (Nat.add_comm tree, id_imp assumption
##                 boundary) under GC and ARC; one passing case each. The full app
##                 suite passes 108 cases. Pixel acceptance: docs/designs/proof-workbench/design-qa.md.
workbench-test:
	$(MAKE) -C $(ROOT) _workbench-test

## workbench-native-test: Build the Mac Workbench host, then `0 failures` from its
##                        Swift suite. Fetch Git LFS lesson assets first.
workbench-native-test:
	$(MAKE) -C $(ROOT) _workbench-native-test

## workbench-render-review: Render a captured region tree natively (full page).
##   TREE defaults to the browser suite's slate board; expect a 1488x1102 image.
##   Matching dimensions do not prove pixel fidelity.
workbench-render-review:
	$(MAKE) -C $(ROOT) _workbench-render-review
