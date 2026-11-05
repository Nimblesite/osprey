# Every app: open it, build it, test it. The desktop apps are per OS, in src/toolchain/targets/macos and src/toolchain/targets/windows. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

## casting-ios: Install and launch the shared-GPUI casting player in an iPhone simulator.
##            Shared viewport fitting and service integration are in progress; see src/osprey/examples/casting/mobile.md.
.PHONY: casting-ios casting-android
casting-ios:
	$(MAKE) -C src/toolchain/targets/ios casting

## casting-android: Build and open casting on the selected Android emulator or device.
##                  The shared-GPUI Android backend is in progress; see src/osprey/examples/casting/mobile.md.
casting-android:
	$(MAKE) -C src/toolchain/targets/android casting

.PHONY: python-app-ports bank bank-web bank-test bank-e2e graphics graphics-smoke gpt2 gpt2-train ios ios-test macos macos-test mobile-ios mobile-ios-test mobile-domain-test mobile-test android android-test bank-mobile-domain-test bank-android bank-android-test bank-ios bank-ios-test bank-mobile-test workbench-proof-assets workbench-test workbench-native-test workbench-render-review

## python-app-ports: Verify the classifier, vector search and text evaluation ports.
python-app-ports: build
	$(MAKE) _test_python_app_ports

## bank: Rebuild and run the Talon Bank showcase for manual testing.
##       Opens http://127.0.0.1:18790 (dashboard) / /api/accounts (JSON API).
##       The hold marker keeps the server up; Ctrl-C removes it and exits.
bank: _bank-web
	@echo "==> Talon Bank live on http://127.0.0.1:18790  (Ctrl-C to stop)"
	@touch /tmp/talon_bank.hold
	@trap 'rm -f /tmp/talon_bank.hold' EXIT INT TERM; \
	  (if command -v open >/dev/null && command -v curl >/dev/null; then \
	    attempts=0; \
	    until accounts="$$(curl -fsS http://127.0.0.1:18790/api/accounts 2>/dev/null)" && [[ "$$accounts" == *'"Priya Sharma"'* ]]; do \
	      attempts=$$((attempts + 1)); [ "$$attempts" -ge 200 ] && exit 0; sleep 0.1; \
	    done; \
	    open http://127.0.0.1:18790; \
	  fi) & \
	  ./$(BIN) src/osprey/examples/projects/modules --run

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
##           (src/osprey/examples/projects/modules) — real Chromium via Playwright drives
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
#            one is a one-file project sharing src/osprey/examples/graphics/base/base.osp
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
## The published weights and merges download once into src/osprey/examples/llms/gpt2/weights (ignored by git).
gpt2: build $(GPT2_DIR)/weights/model.safetensors $(GPT2_DIR)/weights/merges.txt
	@case "$(FLAVOR)" in default|python) ;; *) echo "gpt2 FLAVOR must be default or python" >&2; exit 1 ;; esac
	@printf '%s\n%s\n%s\n' "$(PROMPT)" "$(TOKENS)" "$(BACKEND)" | OSPREY_GPU=$(GPU) ./$(BIN) $(GPT2_GENERATOR) --run --memory=arc

## gpt2-train: One training step of GPT-2 on the prompt: loss, gradients, SGD, loss again (usage: make gpt2-train PROMPT="..." BACKEND=gpu|lists)
gpt2-train: build $(GPT2_DIR)/weights/model.safetensors $(GPT2_DIR)/weights/merges.txt
	@test "$(FLAVOR)" = default || { echo "gpt2-train FLAVOR must be default" >&2; exit 1; }
	@printf '%s\n%s\n' "$(PROMPT)" "$(BACKEND)" | OSPREY_GPU=$(GPU) ./$(BIN) $(GPT2_DIR)/train --run --memory=arc

## ios: Build the iPhone and simulator libraries and SwiftUI example apps.
##      Requires macOS, Xcode and both iOS SDKs. Device output is unsigned.
ios:
	$(MAKE) -C $(ROOT) _ios

## ios-test: Validate C ABI calls, language goldens and the SwiftUI simulator app.
ios-test: _ios
	$(MAKE) _test_ios

## macos: Build the Mac runtime, the compiler and every macOS example: the
##        storage app and the three graphics scenes, each a Swift host
##        linked to the Mac slice. Requires an Apple silicon Mac with Xcode.
##        [IOS-TARGET-MACOS]
macos:
	$(MAKE) -C $(ROOT) _macos

## macos-test: The iPhone C ABI fixture, the whole corpus against the same
##             goldens and skip manifest, and the storage example, on this Mac.
##             The storage domain suite runs natively, so it needs `_runtime`.
macos-test: _macos _runtime
	$(MAKE) _test_macos

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
	python3 scripts/proof/workbench-proof-assets.py target/workbench-proof-assets
	python3 scripts/proof/workbench-proof-assets.py target/workbench-proof-assets --check

## workbench-test: Imported-mathematics cases (Nat.add_comm tree, id_imp assumption
##                 boundary) under GC and ARC; one passing case each. The full app
##                 suite passes 108 cases. Pixel acceptance: docs/designs/proof-workbench/design-qa.md.
workbench-test:
	$(MAKE) -C $(ROOT) _workbench-test

## workbench-native-test: Build the Mac Workbench host, then `0 failures` from its
##                        Swift suite. Fetch Git LFS lesson assets first.
workbench-native-test: build
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/workbench/hosts/apple/run.sh --build
	swift test --package-path src/osprey/workbench/hosts/apple

## workbench-render-review: Render a captured region tree natively (full page).
##   TREE defaults to the browser suite's slate board; expect a 1488x1102 image.
##   Matching dimensions do not prove pixel fidelity.
workbench-render-review:
	OSPREY_RENDER_REVIEW_FULL_PAGE=1 bash src/osprey/workbench/hosts/apple/render-review.sh $(WORKBENCH_TREE) 1488 1058 target/native-slate.png
endif
