# Android native app-logic archives. Platform networking stays in the host.
ANDROID_RT_SRC ?= $(MOBILE_RT_SRC)
# Which slice the corpus runs on: the attached device or emulator decides.
ANDROID_GOLDEN_TARGET ?= android

.PHONY: _runtime_android _runtime_android_arm64 _runtime_android_x64 _android _android-test _test_android_goldens
_runtime_android: _runtime_android_arm64 _runtime_android_x64
_runtime_android_arm64: _runtime_android_arm64_arc
	@bash scripts/android-runtime.sh aarch64-linux-android26 $(RTB)/libosprey_runtime_android_arm64.a $(B) -- $(ANDROID_RT_SRC)
_runtime_android_x64: _runtime_android_x64_arc
	@bash scripts/android-runtime.sh x86_64-linux-android26 $(RTB)/libosprey_runtime_android_x64.a $(B) -- $(ANDROID_RT_SRC)

define android_arc_runtime
.PHONY: _runtime_android_$(1)_arc
_runtime_android_$(1)_arc:
	@OSPREY_RUNTIME_SHIMS="$(MOBILE_ARC_SHIM_SRC)" bash scripts/android-runtime.sh $(2) \
		$(RTB)/libosprey_runtime_android_$(1)_arc.a $(B) -- $(MOBILE_ARC_RT_SRC)
endef
$(eval $(call android_arc_runtime,arm64,aarch64-linux-android26))
$(eval $(call android_arc_runtime,x64,x86_64-linux-android26))

## _test_android_goldens: the whole corpus on the attached device, held to the
## byte-exact output the native backend produces, with rejections pinned in
## tests/MOBILE_UNPORTABLE.txt — the same file ios-sim is held to, because the
## two targets share one C ABI implementation and therefore one set of holes.
_test_android_goldens:
	@echo "==> [$(ANDROID_GOLDEN_TARGET)] golden stdout comparison on the device..."
	@OSPREY_TARGET=$(ANDROID_GOLDEN_TARGET) zsh crates/run_test_corpus.sh
