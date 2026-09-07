# iOS device and simulator archives, using Xcode's SDK and Apple clang.
# Processes and HTTP/OpenSSL have no implementation in this target. Files,
# fibers and substituting effects use the existing native runtime. [IOS-TARGET-CAPABILITIES]
IOS_RT_SRC ?= $(filter-out system_runtime,$(basename $(notdir $(FIB_OBJ))))

.PHONY: ios ios-test _runtime_ios _runtime_ios_sim _test_ios

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

_test_ios:
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/test-ios.sh
	OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_IOS_SKIP_BUILD=1 bash examples/ios/run.sh --smoke

.PHONY: mobile-ios mobile-ios-test mobile-domain-test mobile-test
mobile-ios: _runtime_ios _runtime_ios_sim
	cargo build --release -p osprey-cli
	bash examples/mobile/ios/run.sh --build ios
	bash examples/mobile/ios/run.sh --build ios-sim

mobile-ios-test: mobile-ios
	OSPREY_IOS_SKIP_BUILD=1 bash examples/mobile/ios/run.sh --smoke

mobile-domain-test: _runtime
	cargo build --release -p osprey-cli
	$(BIN) examples/mobile/inbox/test --run

mobile-test: mobile-domain-test mobile-ios-test android-test
