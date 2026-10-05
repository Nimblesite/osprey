# Native Talon clients share the browser's Osprey application and live API.
.PHONY: bank-mobile-domain-test bank-android bank-android-test bank-ios bank-ios-test bank-mobile-test

bank-mobile-domain-test: _runtime
	cargo build --release -p osprey-cli
	$(BIN) examples/projects/modules/mobile/app/test --run

bank-android: _runtime_android
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_ANDROID_SKIP_RUNTIME=1 bash examples/projects/modules/mobile/android/build.sh

bank-android-test: bank-android _runtime
	OSPREY_ANDROID_SKIP_BUILD=1 python3 scripts/bank-mobile-test.py -- bash examples/projects/modules/mobile/android/test.sh

bank-ios: _runtime_ios _runtime_ios_sim
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/projects/modules/mobile/ios/run.sh --build ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/projects/modules/mobile/ios/run.sh --build ios-sim

bank-ios-test: bank-ios _runtime
	OSPREY_IOS_SKIP_BUILD=1 python3 scripts/bank-mobile-test.py -- bash examples/projects/modules/mobile/ios/test.sh

bank-mobile-test: bank-mobile-domain-test
	$(MAKE) bank-android-test
	$(MAKE) bank-ios-test
