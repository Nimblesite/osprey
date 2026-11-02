# Android Application Target [ANDROID-TARGET]

Osprey application logic compiles through LLVM into a native Android archive and generated C header. An Android application links the archive through a small JNI bridge and provides the lifecycle, native rendering, and platform services. The same source can target [iOS](0038-iOSTarget.md); the [reactive mobile application spec](../applications/0040-ReactiveMobileApplications.md) defines the shared application boundary.

Compiler and language rules are separate from integration tooling. See [Android code generation](../compiler/AndroidCodegen.md) for the extracted contract.

## Verification [ANDROID-VERIFICATION]

`make -C apps android-test` checks the shared C ABI fixture, the whole `tests/` corpus, and the actual application's deterministic reactive/SQLite smoke test. The fixture and the corpus are the same ones iOS runs: [`scripts/mobile-abi.osp`](../../../scripts/mobile-abi.osp) states the boundary contract once, and `make _test_android_goldens` builds every accepted corpus program as an entry-only library, links it with the NDK, pushes it to the attached device and holds its stdout to the byte-exact native golden. Rejections are pinned in the shared [`MOBILE_UNPORTABLE.txt`](../../../tests/MOBILE_UNPORTABLE.txt).

The slice follows the hardware: `OSPREY_TARGET=android` runs whichever ABI the attached device reports, so an ARM64 phone and an x86-64 emulator need the same command, and naming a slice that the device cannot run is an error rather than a per-program failure. The separate live smoke performs real GitHub requests. Execution is recorded per ABI; a packaged ABI is not evidence that it executed. Hosted CI is configured to execute the x86-64 slice on an API 34 emulator for every pull request: the fixture, the whole corpus, and the application smoke's fresh and restored phases, ending in `OSPREY_ANDROID_SMOKE_OK`. The ARM64 slice previously passed the same checks on a local ARM64 emulator; no hosted runner executes it. Those platform execution results predate the callable handler migration; current application evidence and commands are recorded in [the validation section](../../../examples/mobile/README.md#validation).

Implementation lives in [android.rs](../../../crates/osprey-cli/src/android.rs), the shared [mobile ABI generator](../../../crates/osprey-cli/src/ios_abi.rs), [target capability validator](../../../crates/osprey-cli/src/target_capabilities.rs), and [Android runtime/build scripts](../../../scripts/android.mk). Application sources and runnable commands are in [examples/mobile](../../../examples/mobile/README.md).

## Host build integration

`make _runtime_android` builds both runtime archives. `make -C apps android` builds the compiler and the shared mobile application's debug APK. The sample links the C ABI and JNI adapter into `libosprey_inbox.so` for both ABIs and packages them in the APK. APK construction and Android deployment belong to the native host build, outside the Osprey compiler.
