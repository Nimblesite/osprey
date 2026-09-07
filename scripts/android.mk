# Android native app-logic archives. Platform networking stays in the host.
ANDROID_RT_SRC ?= $(filter-out system_runtime,$(basename $(notdir $(FIB_OBJ))))
.PHONY: _runtime_android _runtime_android_arm64 _runtime_android_x64 android android-test
_runtime_android: _runtime_android_arm64 _runtime_android_x64
_runtime_android_arm64:
	@bash scripts/android-runtime.sh aarch64-linux-android26 $(RTB)/libosprey_runtime_android_arm64.a $(B) -- $(ANDROID_RT_SRC)
_runtime_android_x64:
	@bash scripts/android-runtime.sh x86_64-linux-android26 $(RTB)/libosprey_runtime_android_x64.a $(B) -- $(ANDROID_RT_SRC)
android: _runtime_android
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/mobile/android/build.sh
android-test: android
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/test-android.sh
	OSPREY_ANDROID_SKIP_BUILD=1 bash examples/mobile/android/run.sh --smoke
