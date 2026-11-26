# Embedded targets: delivery plan [TARGET-EMBEDDED-PLAN]

## 🚨 DIRECTIVE: PERFORMANCE OPTIMISATION NECESSARY, ESPECIALLY ON EMBEDDED DEVICES 🚨

Optimise the code generated for embedded targets continuously, profiled on the device, in parallel with every item below ([DEVICE-PERF]).


The [embedded targets specification](../specs/compiler/EmbeddedTargets.md) defines the requirements. The first consumer is the [Prism device](synth/prism.md).

## Cross-compiled ARM Linux executables [TARGET-EMBEDDED-LINUX]

- [ ] Failing CLI tests first: `--target=linux-arm64 --sysroot <dir> --compile` from macOS and Linux x86-64 produces an `aarch64` ELF executable; a missing or unusable sysroot is a typed error naming it.
- [ ] Build the hardened C runtime for `aarch64-unknown-linux-gnu` against the sysroot, cached by the hash of sources, flags and sysroot.
- [ ] Link with LLD against the sysroot; no host library may leak into the link.
- [ ] Run the test corpus under ARC on a GitHub-hosted ARM Linux runner in the CI test phase, byte-for-byte against the goldens, with coverage.
- [ ] Run a macOS cross-built executable on the ARM runner as part of the same phase.
- [ ] GPU kernels on this target are a compile error naming the feature, with a `failscompilation/` program.

## Real-time code [TARGET-EMBEDDED-REALTIME]

- [ ] Language specification: decide and specify the source spelling for a real-time function, with a regression and a `failscompilation/` program, before any code.
- [ ] Lower real-time functions with Clang's `nonblocking` effect; mark the runtime functions they may call.
- [ ] Compile error for a real-time call graph that reaches an allocation, free, blocking lock, fiber operation, I/O or such a handler, naming the call path.
- [ ] Real-time test build with `-fsanitize=realtime`; the synth render journeys run under it.
- [ ] Release of a value whose count reaches zero inside the callback is deferred to the end of the block, with a test that proves no free happens inside.

## Bare metal [TARGET-EMBEDDED-BARE]

Not started. Begins after the Prism device passes its hardware journeys on the embedded Linux target.
