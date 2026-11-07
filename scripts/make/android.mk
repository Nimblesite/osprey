# Android native app-logic archives. Platform networking stays in the host.
ANDROID_RT_SRC ?= $(MOBILE_RT_SRC)
# Which slice the corpus runs on: the attached device or emulator decides.
ANDROID_GOLDEN_TARGET ?= android

.PHONY: _runtime_android _runtime_android_arm64 _runtime_android_x64 _test_android_goldens
_runtime_android: _runtime_android_arm64 _runtime_android_x64
_runtime_android_arm64: _runtime_android_arm64_arc
	@bash scripts/mobile/android-runtime.sh aarch64-linux-android26 $(RTB)/libosprey_runtime_android_arm64.a $(B) -- $(ANDROID_RT_SRC)
_runtime_android_x64: _runtime_android_x64_arc
	@bash scripts/mobile/android-runtime.sh x86_64-linux-android26 $(RTB)/libosprey_runtime_android_x64.a $(B) -- $(ANDROID_RT_SRC)

define android_arc_runtime
.PHONY: _runtime_android_$(1)_arc
_runtime_android_$(1)_arc:
	@OSPREY_RUNTIME_SHIMS="$(MOBILE_ARC_SHIM_SRC)" bash scripts/mobile/android-runtime.sh $(2) \
		$(RTB)/libosprey_runtime_android_$(1)_arc.a $(B) -- $(MOBILE_ARC_RT_SRC)
endef
$(eval $(call android_arc_runtime,arm64,aarch64-linux-android26))
$(eval $(call android_arc_runtime,x64,x86_64-linux-android26))

## _test_android_goldens: the whole corpus on the attached device, held to the
## byte-exact output the native backend produces, with rejections pinned in
## src/osprey/tests/MOBILE_UNPORTABLE.txt — the same file ios-sim is held to, because the
## two targets share one C ABI implementation and therefore one set of holes.
_test_android_goldens:
	@echo "==> [$(ANDROID_GOLDEN_TARGET)] golden stdout comparison on the device..."
	@OSPREY_TARGET=$(ANDROID_GOLDEN_TARGET) zsh src/toolchain/crates/run_test_corpus.sh

## _android-inbox: The inbox app for Android.
## _android-bank: Talon Bank for Android.
.PHONY: _android-inbox _android-bank
_android-inbox: _runtime_android
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/mobile/android/build.sh

_android-bank: _runtime_android
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_ANDROID_SKIP_RUNTIME=1 bash src/osprey/examples/projects/modules/mobile/android/build.sh
