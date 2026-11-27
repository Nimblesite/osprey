# iOS device, simulator and Mac archives, using Xcode's SDK and Apple clang.
# Processes and HTTP/OpenSSL have no implementation in this target. Files,
# fibers and substituting effects use the existing native runtime. The Mac
# slice compiles the same sources with the same flags, so a Swift host links
# one boundary on every Apple platform. [IOS-TARGET-CAPABILITIES] [IOS-TARGET-MACOS]

.PHONY: _runtime_ios _runtime_ios_sim _runtime_macos _test_ios _test_ios_goldens _test_macos

_runtime_ios: _runtime_ios_arc _runtime_ios_ownership

_runtime_ios_sim: _runtime_ios_sim_arc _runtime_ios_sim_ownership

# The Mac slice also carries the Metal device adapter, so a desktop host such as
# GPUI can run Osprey kernels on the GPU ([GPU-DEVICE], [GPUI-HOST-TARGETS]).
MACOS_GPU_FLAGS ?= -DOSPREY_GPU_METAL=1 -Isrc/toolchain/compiler/bin

_runtime_macos: _runtime_macos_arc _runtime_macos_ownership src/toolchain/compiler/bin/gpu_shader_source.h

# One tracked-heap archive per slice and backend: $(5) is `arc` or `ownership`
# [GC-ARC-PERCEUS] [MEM-OWNERSHIP-STATIC]; $(6) names its unit list.
define apple_tracked_runtime
.PHONY: _runtime_$(1)_$(5)
_runtime_$(1)_$(5): $(if $(4),src/toolchain/compiler/bin/gpu_shader_source.h)
	@OSPREY_RUNTIME_SHIMS="$(MOBILE_ARC_SHIM_SRC)" OSPREY_RUNTIME_OBJC="$(4)" bash scripts/mobile/ios-runtime.sh $(2) $(3) \
		$(RTB)/libosprey_runtime_$(1)_$(5).a $(B) -DOSPREY_IOS $(if $(4),$(MACOS_GPU_FLAGS)) -- $($(6))
endef
$(eval $(call apple_tracked_runtime,ios,iphoneos,arm64-apple-ios15.0,,arc,MOBILE_ARC_RT_SRC))
$(eval $(call apple_tracked_runtime,ios_sim,iphonesimulator,arm64-apple-ios15.0-simulator,,arc,MOBILE_ARC_RT_SRC))
$(eval $(call apple_tracked_runtime,macos,macosx,arm64-apple-macos12.0,gpu_metal,arc,MOBILE_ARC_RT_SRC))
$(eval $(call apple_tracked_runtime,ios,iphoneos,arm64-apple-ios15.0,,ownership,MOBILE_OWNERSHIP_RT_SRC))
$(eval $(call apple_tracked_runtime,ios_sim,iphonesimulator,arm64-apple-ios15.0-simulator,,ownership,MOBILE_OWNERSHIP_RT_SRC))
$(eval $(call apple_tracked_runtime,macos,macosx,arm64-apple-macos12.0,gpu_metal,ownership,MOBILE_OWNERSHIP_RT_SRC))

_test_macos:
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash scripts/mobile/test-macos.sh
	@echo "==> [macos] golden stdout comparison on this Mac..."
	@OSPREY_TARGET=macos zsh src/toolchain/crates/run_test_corpus.sh
	$(BIN) src/osprey/examples/macos/storage/test --run
	OSPREY_MACOS_SKIP_BUILD=1 bash src/osprey/examples/macos/run.sh --smoke

_test_ios: _test_ios_goldens _test_ios_apps

## _test_ios_apps: the runtime memory checks, the C ABI fixture and the SwiftUI example app's smoke.
_test_ios_apps:
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

## _ios-examples: The iPhone and simulator libraries and the SwiftUI example apps. Device output is unsigned.
## _ios-inbox: The inbox app for iOS devices and the simulator.
## _macos-examples: The Mac runtime, the compiler and every macOS example: the storage app and the three
##                  graphics scenes, each a Swift host linked to the Mac slice. [IOS-TARGET-MACOS]
.PHONY: _ios-examples _ios-inbox _macos-examples
_ios-examples: _runtime_ios _runtime_ios_sim
	$(COMPILER_BUILD)
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/ios/build.sh ios
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/ios/build.sh ios-sim

_ios-inbox: _runtime_ios _runtime_ios_sim
	$(COMPILER_BUILD)
	bash src/osprey/examples/mobile/ios/run.sh --build ios
	bash src/osprey/examples/mobile/ios/run.sh --build ios-sim

_macos-examples: _runtime_macos
	$(COMPILER_BUILD)
	OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/macos/run.sh --build
	for scene in kali opal character; do \
		OSPREY_BIN="$(CURDIR)/$(BIN)" bash src/osprey/examples/graphics/macos/run.sh --build $$scene || exit 1; \
	done
