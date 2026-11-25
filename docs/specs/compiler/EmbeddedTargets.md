# Embedded targets [TARGET-EMBEDDED]

## 🚨 DIRECTIVE: PERFORMANCE OPTIMISATION NECESSARY, ESPECIALLY ON EMBEDDED DEVICES 🚨

Code compiled for an embedded target must be fast: the device's CPU is slower than a desktop's and its audio deadline is hard. Generated code for these targets is profiled on the device and optimised continuously, in parallel with all other work ([DEVICE-PERF]).


Status: specified, not built. The [embedded targets plan](../../plans/embedded-targets.md) tracks delivery. The first consumer is the [Prism device](../synth/PrismDevice.md); the research behind each choice is in [hardware research](../../research/synthesis/hardware.md).

An embedded target compiles an Osprey program to run on a small device with no compiler on it. It covers two kinds of device:

1. **Embedded Linux on 64-bit ARM** ([TARGET-EMBEDDED-LINUX]): a Raspberry Pi 5 or Compute Module 5 running a minimal Linux. This is the first target.
2. **Bare metal** ([TARGET-EMBEDDED-BARE]): no operating system at all. This is a later target.

The same rules apply as for every target. A program with errors never builds ([COMPILER-RED-FAILS]). Nothing panics, aborts or exits ([FAILURE-EXPLICIT]). The C runtime is built with the hardened flags. A feature the target cannot provide is a compile error that names the feature, as on WebAssembly ([WASM-TARGET]), never a stub.

## What exists today [TARGET-EMBEDDED-TODAY]

`--target=linux-arm64` already builds an **application archive** for a desktop host such as GPUI: the program plus the native ARC runtime in one static library with a C header ([GPUI-HOST-TARGETS]). It builds only on an ARM Linux machine, with that machine's own `clang`, because it bundles that machine's runtime. It cannot build an executable for a device from a Mac or an x86 machine. That is the gap this specification closes.

## Embedded Linux on 64-bit ARM [TARGET-EMBEDDED-LINUX]

```sh
osprey engine.ospml --compile --target=linux-arm64 --sysroot=path/to/sysroot -o prism-engine
```

- **Cross-compiled executables ([TARGET-EMBEDDED-LINUX-CROSS]).** With `--sysroot`, `--target=linux-arm64 --compile` builds an executable for `aarch64-unknown-linux-gnu` on any host the compiler runs on: macOS, Linux x86-64, Linux ARM and Windows. Clang compiles the generated IR and the C runtime for the target triple against the sysroot, and LLD links them. The sysroot is the device's C library and headers; for the Prism device it is a Raspberry Pi OS (Debian arm64) root, so the executable links exactly the library the device boots ([DEVICE-OS]). Without `--sysroot`, the target keeps today's behaviour: the host-built archive.
- **The runtime is built for the target ([TARGET-EMBEDDED-LINUX-RUNTIME]).** The C runtime is compiled from the same sources with the same hardened flags for the target triple, once per sysroot, and cached by the hash of its sources, flags and sysroot. Releases ship no prebuilt device runtimes, so nothing compiled lands in git ([REPO-NO-ARTIFACTS]).
- **ARC by default ([TARGET-EMBEDDED-LINUX-MEMORY]).** ARC frees memory at known points with no collector pauses, which a real-time device needs. `--memory=gc` is accepted but never used for an audio engine.
- **Capabilities ([TARGET-EMBEDDED-LINUX-CAPS]).** The full Linux runtime is available: threads, fibers, files, sockets, HTTP and processes. GPU kernels are refused with a compile error until a device GPU adapter exists.
- **Tested on the architecture ([TARGET-EMBEDDED-LINUX-VERIFY]).** The test corpus runs under ARC on a GitHub-hosted ARM Linux runner in the CI test phase, with output byte-for-byte equal to its golden. A cross-built executable from a macOS runner is run on the same ARM runner, so the cross path is tested, not only the native one.

## Real-time code [TARGET-EMBEDDED-REALTIME]

An audio callback must never allocate, free, take a lock that can block, do I/O or wait ([SYNTH-RENDER]). On a device with a 1.33 ms deadline, one allocation can cost an audible dropout. Osprey checks this with the tools LLVM already provides instead of by inspection ([DEVICE-RESEARCH-RTSAFE]):

- **Compile time.** A function the program exports as real-time is lowered with Clang's `nonblocking` function effect, and the runtime functions it may call carry the same attribute. A real-time function whose call graph reaches an allocation, a free, a blocking lock, a fiber operation, I/O or an effect handler that does any of these is a compile error that names the call and its path. The source spelling of "real-time" is decided in the [language specification](../language/README.md) before it is implemented; it must not be a comment or a naming convention.
- **Run time.** The real-time test build links Clang's RealtimeSanitizer (`-fsanitize=realtime`), and the device journeys run with it. A violation the static check missed fails the test with its stack.
- **ARC on the callback path.** Retain and release of values created before the callback are allowed, because they do not allocate. A release that would free memory inside the callback is a violation: the engine keeps such values alive until the block ends and frees them outside the callback.

## Bare metal [TARGET-EMBEDDED-BARE]

A later target for devices with no operating system: the Raspberry Pi through a bare-metal library in the style of MiniDexed on Circle, and Cortex-M7 boards such as the Daisy Seed, which reach sub-millisecond latency for about USD 21–30 ([DEVICE-RESEARCH-PRIOR]).

- Triples: `aarch64-none-elf` for the Raspberry Pi and `thumbv7em-none-eabihf` for Cortex-M7.
- No threads, fibers, files, sockets, processes or HTTP: each is a compile error naming the feature.
- Fixed memory: the program's heap is one region sized at link time; running out is a typed error, never a crash.
- Not started. It begins only after the embedded Linux target and the real-time checks pass on the Prism device.
