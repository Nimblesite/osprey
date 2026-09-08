# iOS device and simulator archives, using Xcode's SDK and Apple clang.
# Processes and HTTP/OpenSSL have no implementation in this target. Files,
# fibers and substituting effects use the existing native runtime. [IOS-TARGET-CAPABILITIES]
IOS_RT_SRC ?= $(filter-out system_runtime,$(basename $(notdir $(FIB_OBJ))))

.PHONY: ios ios-test _runtime_ios _runtime_ios_sim _test_ios _test_ios_goldens

_runtime_ios:
	@bash scripts/ios-runtime.sh iphoneos arm64-apple-ios15.0 \
		$(RTB)/libosprey_runtime_ios.a $(B) -DOSPREY_IOS -- $(IOS_RT_SRC)

_runtime_ios_sim:
	@bash scripts/ios-runtime.sh iphonesimulator arm64-apple-ios15.0-simulator \
		$(RTB)/libosprey_runtime_ios_sim.a $(B) -DOSPREY_IOS -- $(IOS_RT_SRC)

## ios: Build the iPhone and simulator libraries and SwiftUI example apps.
##      Requires macOS, Xcode and both iOS SDKs. Device output is unsigned.
ios: _runtime_ios _runtime_ios_sim
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/ios/build.sh ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/ios/build.sh ios-sim

## ios-test: Validate C ABI calls, language goldens and the SwiftUI simulator app.
ios-test: ios
	$(MAKE) _test_ios

_test_ios: _test_ios_goldens
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/test-ios.sh
	OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_IOS_SKIP_BUILD=1 bash examples/ios/run.sh --smoke

## _test_ios_goldens: the whole corpus, the same goldens, the mobile C ABI.
## Every program that the ios-sim target accepts is built as a library, linked
## into a C host and run in an iPhone simulator, then held to the byte-exact
## output the native backend produces. The rest report as named skips pinned in
## tests/IOS_UNPORTABLE.txt. Without this the target was gated by seven
## hand-picked programs, which cannot notice a boundary that truncates a
## string, loses a bool's high bits or miscompiles arithmetic inside an archive.
_test_ios_goldens:
	@echo "==> [ios-sim] golden stdout comparison in the simulator..."
	@OSPREY_TARGET=ios-sim zsh crates/run_test_corpus.sh

.PHONY: mobile-ios mobile-ios-test mobile-domain-test mobile-test
mobile-ios: _runtime_ios _runtime_ios_sim
	cargo build --release -p osprey-cli
	bash examples/mobile/ios/run.sh --build ios
	bash examples/mobile/ios/run.sh --build ios-sim

mobile-ios-test: mobile-ios
	OSPREY_IOS_SKIP_BUILD=1 bash examples/mobile/ios/run.sh --smoke

mobile-domain-test: _runtime
	python3 scripts/test-mobile-tools.py
	cargo build --release -p osprey-cli
	$(BIN) examples/mobile/inbox/test --run

mobile-test: mobile-domain-test mobile-ios-test android-test
