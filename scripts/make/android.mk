# Android native app-logic archives. Platform networking stays in the host.
# Which slice the corpus runs on: the attached device or emulator decides.
ANDROID_GOLDEN_TARGET ?= android

.PHONY: _runtime_android _runtime_android_arm64 _runtime_android_x64 _test_android_goldens
_runtime_android: _runtime_android_arm64 _runtime_android_x64
_runtime_android_arm64: _runtime_android_arm64_arc _runtime_android_arm64_ownership
_runtime_android_x64: _runtime_android_x64_arc _runtime_android_x64_ownership

# One tracked-heap archive per slice and backend: $(3) is `arc` or `ownership`
# [GC-ARC-PERCEUS] [MEM-OWNERSHIP-STATIC]; $(4) names its unit list.
define android_tracked_runtime
.PHONY: _runtime_android_$(1)_$(3)
_runtime_android_$(1)_$(3):
	@OSPREY_RUNTIME_SHIMS="$(MOBILE_ARC_SHIM_SRC)" bash scripts/mobile/android-runtime.sh $(2) \
		$(RTB)/libosprey_runtime_android_$(1)_$(3).a $(B) -- $($(4))
endef
$(eval $(call android_tracked_runtime,arm64,aarch64-linux-android26,arc,MOBILE_ARC_RT_SRC))
$(eval $(call android_tracked_runtime,x64,x86_64-linux-android26,arc,MOBILE_ARC_RT_SRC))
$(eval $(call android_tracked_runtime,arm64,aarch64-linux-android26,ownership,MOBILE_OWNERSHIP_RT_SRC))
$(eval $(call android_tracked_runtime,x64,x86_64-linux-android26,ownership,MOBILE_OWNERSHIP_RT_SRC))

## _test_android_goldens: the whole corpus on the attached device, held to the
## byte-exact output the native backend produces, with rejections pinned in
## src/osprey/tests/MOBILE_UNPORTABLE.txt — the same file ios-sim is held to, because the
## two targets share one C ABI implementation and therefore one set of holes.
_test_android_goldens:
	@echo "==> [$(ANDROID_GOLDEN_TARGET)] golden stdout comparison on the device..."
	@OSPREY_TARGET=$(ANDROID_GOLDEN_TARGET) zsh src/toolchain/crates/run_test_corpus.sh

## _android-inbox: The inbox app for Android.
.PHONY: _android-inbox
_android-inbox: _runtime_android
	cargo build --manifest-path src/toolchain/Cargo.toml --release -p osprey-cli
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/mobile/android/build.sh
