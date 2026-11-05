# iOS device, simulator and Mac archives, using Xcode's SDK and Apple clang.
# Processes and HTTP/OpenSSL have no implementation in this target. Files,
# fibers and substituting effects use the existing native runtime. The Mac
# slice compiles the same sources with the same flags, so a Swift host links
# one boundary on every Apple platform. [IOS-TARGET-CAPABILITIES] [IOS-TARGET-MACOS]
IOS_RT_SRC ?= $(MOBILE_RT_SRC)

.PHONY: _ios _macos _runtime_ios _runtime_ios_sim _runtime_macos _test_ios _test_ios_goldens _test_macos

_runtime_ios: _runtime_ios_arc
	@bash scripts/mobile/ios-runtime.sh iphoneos arm64-apple-ios15.0 \
		$(RTB)/libosprey_runtime_ios.a $(B) -DOSPREY_IOS -- $(IOS_RT_SRC)

_runtime_ios_sim: _runtime_ios_sim_arc
	@bash scripts/mobile/ios-runtime.sh iphonesimulator arm64-apple-ios15.0-simulator \
		$(RTB)/libosprey_runtime_ios_sim.a $(B) -DOSPREY_IOS -- $(IOS_RT_SRC)

# The Mac slice also carries the Metal device adapter, so a desktop host such as
# GPUI can run Osprey kernels on the GPU ([GPU-DEVICE], [GPUI-HOST-TARGETS]).
MACOS_GPU_FLAGS ?= -DOSPREY_GPU_METAL=1 -Isrc/toolchain/compiler/bin

_runtime_macos: _runtime_macos_arc src/toolchain/compiler/bin/gpu_shader_source.h
	@OSPREY_RUNTIME_OBJC=gpu_metal bash scripts/mobile/ios-runtime.sh macosx arm64-apple-macos12.0 \
		$(RTB)/libosprey_runtime_macos.a $(B) -DOSPREY_IOS $(MACOS_GPU_FLAGS) -- $(IOS_RT_SRC)

define apple_arc_runtime
.PHONY: _runtime_$(1)_arc
_runtime_$(1)_arc: $(if $(4),src/toolchain/compiler/bin/gpu_shader_source.h)
	@OSPREY_RUNTIME_SHIMS="$(MOBILE_ARC_SHIM_SRC)" OSPREY_RUNTIME_OBJC="$(4)" bash scripts/mobile/ios-runtime.sh $(2) $(3) \
		$(RTB)/libosprey_runtime_$(1)_arc.a $(B) -DOSPREY_IOS $(if $(4),$(MACOS_GPU_FLAGS)) -- $(MOBILE_ARC_RT_SRC)
endef
$(eval $(call apple_arc_runtime,ios,iphoneos,arm64-apple-ios15.0))
$(eval $(call apple_arc_runtime,ios_sim,iphonesimulator,arm64-apple-ios15.0-simulator))
$(eval $(call apple_arc_runtime,macos,macosx,arm64-apple-macos12.0,gpu_metal))

_test_macos:
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/mobile/test-macos.sh
	@echo "==> [macos] golden stdout comparison on this Mac..."
	@OSPREY_TARGET=macos zsh src/toolchain/crates/run_test_corpus.sh
	$(BIN) src/osprey/examples/macos/storage/test --run
	OSPREY_MACOS_SKIP_BUILD=1 bash src/osprey/examples/macos/run.sh --smoke

_test_ios: _test_ios_goldens
	/bin/bash scripts/mobile/test-runtime-memory.sh
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/mobile/test-ios.sh
	OSPREY_BIN="$(CURDIR)/$(BIN)" OSPREY_IOS_SKIP_BUILD=1 bash src/osprey/examples/ios/run.sh --smoke

## _test_ios_goldens: the whole corpus, the same goldens, the mobile C ABI.
## Every program that the ios-sim target accepts is built as a library, linked
## into a C host and run in an iPhone simulator, then held to the byte-exact
## output the native backend produces. The rest report as named skips pinned in
## src/osprey/tests/MOBILE_UNPORTABLE.txt — the same manifest Android is held to, because
## the two targets share one C ABI implementation and therefore one set of
## holes. Without this the target was gated by seven hand-picked programs,
## which cannot notice a boundary that truncates a string, loses a bool's high
## bits or miscompiles arithmetic inside an archive.
_test_ios_goldens:
	@echo "==> [ios-sim] golden stdout comparison in the simulator..."
	@OSPREY_TARGET=ios-sim zsh src/toolchain/crates/run_test_corpus.sh
