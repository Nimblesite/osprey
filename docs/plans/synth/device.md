# Prism device: delivery plan [DEVICE-PLAN]

The [Prism device specification](../../specs/synth/PrismDevice.md) defines the engine, firmware, controls, link and factory. The board and enclosure have their own [hardware plan](hardware.md), and the compiler work has the [embedded targets plan](../embedded-targets.md). Research and citations: [hardware research](../../research/synthesis/hardware.md).

The order is fixed: macros work in Prism ([instrument plan](instrument.md)), then the same engine runs headless as a virtual device, then cross-compiled for ARM Linux, then on the prototype, then on the Prism board. Each phase ships something usable on its own.

## Gate: the engine must be fast enough [DEVICE-PLAN-SPEED]

The device runs the same engine as Prism. The Pi 5's CPU is enough for a decent stereo patch, and published instruments prove it ([DEVICE-RESEARCH-COMPUTE]). Osprey's engine is not yet: it renders 0.37 s of audio per second on a Mac under load ([DAW-TRANSPORT-LIVE-SPEED] in the [daw plan](daw.md)), and the profile is mostly reference counting and allocation. A Cortex-A76 core is slower than an Apple core, and the device also has a 1.33 ms deadline per block.

- [ ] The daw's live-speed items are done: 1.5× real time or better on the codex song on the Mac.
- [ ] The render callback passes the real-time checks: no allocation, free, blocking lock or I/O, statically and under RealtimeSanitizer ([TARGET-EMBEDDED-REALTIME]).
- [ ] Measure per-processor costs on a Raspberry Pi 5 for the budget table ([DEVICE-ENGINE-BUDGET]). The firmware and hardware phases do not start until the acceptance patch ([DEVICE-HW-COMPUTE]) fits within 70% of a 64-frame block at 48 kHz on one core.

## Device profiles and the virtual device [DEVICE-ENGINE-VIRTUAL]

- [ ] **Profile model ([DEVICE-PROFILE]).** Move [device.td](../../designs/prism-device/device.td) into its package, generate Osprey types, and write the reference profile as data.
- [ ] **Engine program ([DEVICE-ENGINE]).** An Osprey program linking only `@osprey/synth`: load a `PatchDocument`, check roles, nodes and budget, render. It sits beside the other apps as `make -C src/toolchain/targets/<platform> prism-device`, with tests in each platform's tests Makefile and a headless suite in `src/toolchain/targets/tests`.
- [ ] **Controls ([DEVICE-CONTROLS]).** Knob filter, hysteresis, pickup and jump, switches, footswitch toggle, hold and long press, written in Osprey; proofs per [DEVICE-PROOFS].
- [ ] **Link ([DEVICE-LINK]).** tdbin frames over TCP with the 1 MiB bound: Hello, live control, deploy, refusals, get patch. The virtual device listens on localhost only.
- [ ] **Devices panel ([DEVICE-LINK-SHOW]).** Prism lists devices, opens the playing patch or fetches an unknown one, and deploys with the profile check shown first.
- [ ] **Virtual device journeys ([DEVICE-VERIFY]).** In CI: many deploys across slots, every refusal kind, malformed and oversized frames, every control moved with macro and audio assertions, click-free swaps, injected faults.

## Firmware [DEVICE-OS]

Built on the work of Elk Audio OS, Zynthian and PiPedal ([DEVICE-RESEARCH-PRIOR]); mainline PREEMPT_RT rather than Xenomai ([DEVICE-RESEARCH-RT]).

- [ ] Buildroot tree with pinned sources: mainline PREEMPT_RT kernel, BusyBox, ALSA, USB gadget, the engine as the only service. Image built by `make`, shipped as a CI artifact, never committed.
- [ ] The Buildroot staging directory is the sysroot for `--target=linux-arm64 --sysroot` ([TARGET-EMBEDDED-LINUX-CROSS]).
- [ ] Read-only squashfs root, atomic data partition, A/B images with the 10 s health check and fallback.
- [ ] Image signing with the release key; the updater refuses unsigned images.
- [ ] Isolated audio core, `SCHED_FIFO`, locked memory, direct ALSA at 64 frames; 32 and 128 offered, nothing that can exceed 10 ms ([DEVICE-HW-LATENCY-LIMITS]).
- [ ] USB composite gadget (MIDI + network link), USB MIDI host, TRS MIDI UART; one merged, timestamped event stream.
- [ ] Watchdog restart with the bypass relay held.
- [ ] QEMU boot test in CI: the image boots, the engine starts and answers Hello on the link (no audio).

## Factory [DEVICE-FACTORY]

- [ ] Product bundle built by `make` from a project file and a profile.
- [ ] Provisioning: flash the eMMC over USB-C, write the serial number, run the device test; pass or fail with no partial image.
- [ ] Device test: loopback audio, driven knob wipers, MIDI loopback, bypass with power off; a report per serial number.

## Open questions [DEVICE-PLAN-OPEN]

- **Stabilisation mode.** The repository is in stabilisation mode (no new features). The [instrument plan](instrument.md) completes an already specified feature; the device, firmware, hardware and embedded target work are new features and start only when the user lifts stabilisation for them.
- **Cost.** ARM runner minutes and any hardware runner are new spend; hardware journeys stay local until a runner is approved.
