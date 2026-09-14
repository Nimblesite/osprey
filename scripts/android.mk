# Android native app-logic archives. Platform networking stays in the host.
ANDROID_RT_SRC ?= $(filter-out system_runtime,$(basename $(notdir $(FIB_OBJ))))
# Which slice the corpus runs on: the attached device or emulator decides.
ANDROID_GOLDEN_TARGET ?= android

.PHONY: _runtime_android _runtime_android_arm64 _runtime_android_x64 android android-test _test_android_goldens
_runtime_android: _runtime_android_arm64 _runtime_android_x64
_runtime_android_arm64:
	@bash scripts/android-runtime.sh aarch64-linux-android26 $(RTB)/libosprey_runtime_android_arm64.a $(B) -- $(ANDROID_RT_SRC)
_runtime_android_x64:
	@bash scripts/android-runtime.sh x86_64-linux-android26 $(RTB)/libosprey_runtime_android_x64.a $(B) -- $(ANDROID_RT_SRC)
android: _runtime_android
	cargo build --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash examples/mobile/android/build.sh
# The goldens need the runtime archives `android` builds, so they are ordered by
# a recipe line rather than listed as a sibling prerequisite: under `make -j`
# siblings may run at once, and the corpus would race the archive it links.
android-test: android
	$(MAKE) _test_android_goldens
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/test-android.sh
	OSPREY_ANDROID_SKIP_BUILD=1 bash examples/mobile/android/run.sh --smoke

## _test_android_goldens: the whole corpus on the attached device, held to the
## byte-exact output the native backend produces, with rejections pinned in
## tests/MOBILE_UNPORTABLE.txt — the same file ios-sim is held to, because the
## two targets share one C ABI implementation and therefore one set of holes.
_test_android_goldens:
	@echo "==> [$(ANDROID_GOLDEN_TARGET)] golden stdout comparison on the device..."
	@OSPREY_TARGET=$(ANDROID_GOLDEN_TARGET) zsh crates/run_test_corpus.sh
