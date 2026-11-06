# Every app: open it, build it, test it. The desktop apps are per OS, in src/toolchain/targets/macos and src/toolchain/targets/windows. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../..
WORKBENCH_TREE ?= target/workbench-vsix/slate-board.native.json
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

bank-web: export PATH := $(WASM_PATH_PREFIX)$(PATH)

.PHONY: python-app-ports bank bank-web bank-e2e graphics graphics-smoke gpt2 gpt2-train ios ios-test macos macos-test mobile-ios mobile-ios-test mobile-test android android-test bank-android bank-android-test bank-ios bank-ios-test bank-mobile-test workbench-proof-assets workbench-native-test workbench-render-review

## python-app-ports: Verify the classifier, vector search and text evaluation ports.
python-app-ports: build
	$(MAKE) _test_python_app_ports

## bank: Rebuild and run the Talon Bank showcase for manual testing.
##       Opens http://127.0.0.1:18790 (dashboard) / /api/accounts (JSON API).
##       The hold marker keeps the server up; Ctrl-C removes it and exits.
bank: bank-web
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
bank-web: build _runtime_wasm $(BANK_WEB_NODE_DEPS)
	@echo "==> Building Talon Bank browser application..."
	cd src/osprey/examples/projects/modules/web && npm run build

## bank-e2e: Browser end-to-end tests for the Talon Bank modules showcase
##           (src/osprey/examples/projects/modules) — real Chromium via Playwright drives
##           the compiled osprey binary serving its HTTP API and web UI.
##           The server's hold marker must not outlive the run: a leaked one
##           makes every later bank-*-test refuse to start.
bank-e2e: bank-web $(BANK_E2E_NODE_DEPS)
	@echo "==> Bank e2e (Playwright)..."
	cd src/osprey/examples/projects/modules/e2e && npx playwright install chromium && npx playwright test
	@test ! -e /tmp/talon_bank.hold || { echo "FAIL: bank e2e left /tmp/talon_bank.hold behind; its webServer was stopped without running its trap"; exit 1; }

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
ios: _runtime_ios _runtime_ios_sim
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/ios/build.sh ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/ios/build.sh ios-sim

## ios-test: Validate C ABI calls, language goldens and the SwiftUI simulator app.
ios-test: ios
	$(MAKE) _test_ios

## macos: Build the Mac runtime, the compiler and every macOS example: the
##        storage app and the three graphics scenes, each a Swift host
##        linked to the Mac slice. Requires an Apple silicon Mac with Xcode.
##        [IOS-TARGET-MACOS]
macos: _runtime_macos
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/macos/run.sh --build
	for scene in kali opal character; do \
		OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/graphics/macos/run.sh --build $$scene || exit 1; \
	done

## macos-test: The iPhone C ABI fixture, the whole corpus against the same
##             goldens and skip manifest, and the storage example, on this Mac.
##             The storage domain suite runs natively, so it needs `_runtime`.
macos-test: macos _runtime
	$(MAKE) _test_macos

## mobile-ios: Build the inbox app for iOS devices and the simulator.
mobile-ios: _runtime_ios _runtime_ios_sim
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	bash src/osprey/examples/mobile/ios/run.sh --build ios
	bash src/osprey/examples/mobile/ios/run.sh --build ios-sim

## mobile-ios-test: Smoke-test the built inbox app on iOS.
mobile-ios-test: mobile-ios
	OSPREY_IOS_SKIP_BUILD=1 bash src/osprey/examples/mobile/ios/run.sh --smoke

## mobile-test: The inbox app's domain, iOS and Android tests.
mobile-test: mobile-ios-test android-test
	$(MAKE) -C src/toolchain/targets/tests inbox

## android: Build the inbox app for Android.
android: _runtime_android
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/mobile/android/build.sh

## android-test: The Android goldens, tools and smoke run for the inbox app.
# The goldens need the runtime archives `android` builds, so they are ordered by
# a recipe line rather than listed as a sibling prerequisite: under `make -j`
# siblings may run at once, and the corpus would race the archive it links.
android-test: android
	$(MAKE) _test_android_goldens
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/mobile/test-android.sh
	OSPREY_ANDROID_SKIP_BUILD=1 bash src/osprey/examples/mobile/android/run.sh --smoke

## bank-android: Build Talon Bank for Android.
bank-android: _runtime_android
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_ANDROID_SKIP_RUNTIME=1 bash src/osprey/examples/projects/modules/mobile/android/build.sh

## bank-android-test: Talon Bank against its live API on Android.
bank-android-test: bank-android _runtime
	OSPREY_ANDROID_SKIP_BUILD=1 python3 scripts/mobile/bank-mobile-test.py -- bash src/osprey/examples/projects/modules/mobile/android/test.sh

## bank-ios: Build Talon Bank for iOS devices and the simulator.
bank-ios: _runtime_ios _runtime_ios_sim
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/projects/modules/mobile/ios/run.sh --build ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/projects/modules/mobile/ios/run.sh --build ios-sim

## bank-ios-test: Talon Bank against its live API on iOS.
bank-ios-test: bank-ios _runtime
	OSPREY_IOS_SKIP_BUILD=1 python3 scripts/mobile/bank-mobile-test.py -- bash src/osprey/examples/projects/modules/mobile/ios/test.sh

## bank-mobile-test: Talon Bank's shared, Android and iOS tests.
bank-mobile-test:
	$(MAKE) -C src/toolchain/targets/tests bank-mobile
	$(MAKE) -C src/toolchain/targets bank-android-test
	$(MAKE) -C src/toolchain/targets bank-ios-test

## workbench-proof-assets: Stage and byte-check the 13 bundled proof exports and
##                         2 upstream licences; any extra, missing or changed file fails.
workbench-proof-assets:
	python3 scripts/proof/workbench-proof-assets.py target/workbench-proof-assets
	python3 scripts/proof/workbench-proof-assets.py target/workbench-proof-assets --check

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
