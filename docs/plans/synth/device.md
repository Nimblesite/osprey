# Prism device: delivery plan [DEVICE-PLAN]

## 🚨 DIRECTIVE: PERFORMANCE OPTIMISATION NECESSARY, ESPECIALLY ON EMBEDDED DEVICES 🚨 [DEVICE-PERF]

**Performance optimisation is not optional and never finished.** The engine that runs in Prism is the engine that runs on the device, and the device has a slower CPU and a hard 1.33 ms deadline per 64-frame block. Today the engine renders 0.37 s of audio per second on a Mac, mostly in reference counting and allocation ([DAW-TRANSPORT-LIVE-SPEED]). That must improve many times over, and it is worked on **in parallel with everything else**, not as a gate that holds other work back.

- Every change to the engine, the synth package or the code the compiler generates for them is measured for speed, and a change that makes the device slower is a defect.
- The render callback allocates, frees and blocks nothing ([TARGET-EMBEDDED-REALTIME]).
- Per-processor costs are measured on the device's own CPU ([DEVICE-ENGINE-BUDGET]), recorded, and only go down.
- The acceptance stereo patch must run at 70% or less of the deadline on one core ([DEVICE-HW-COMPUTE]). Until it does, that is the most important open item on the device.


The [Prism device specification](../../specs/synth/PrismDevice.md) defines the engine, firmware, controls, link and factory. The board and enclosure have their own [hardware plan](hardware.md), and the compiler work has the [embedded targets plan](../embedded-targets.md). Research and citations: [hardware research](../../research/synthesis/hardware.md).

Macros work in Prism first ([instrument plan](instrument.md)). The virtual device, the embedded target, the firmware, the prototype and the board then proceed in parallel, with performance optimisation running beside all of them ([DEVICE-PERF]). Each phase ships something usable on its own.

## Performance optimisation, in parallel with everything [DEVICE-PLAN-SPEED]

This track runs alongside every other phase and never stops ([DEVICE-PERF]). The device runs the same engine as Prism. The Pi 5's CPU is enough for a decent stereo patch, and published instruments prove it ([DEVICE-RESEARCH-COMPUTE]). Osprey's engine is not yet: it renders 0.37 s of audio per second on a Mac under load ([DAW-TRANSPORT-LIVE-SPEED] in the [daw plan](daw.md)), and the profile is mostly reference counting and allocation. A Cortex-A76 core is slower than an Apple core, and the device also has a 1.33 ms deadline per block. Nothing else waits for this track, and this track waits for nothing else.

- [ ] The daw's live-speed items are done: 1.5× real time or better on the codex song on the Mac.
- [ ] The render callback passes the real-time checks: no allocation, free, blocking lock or I/O, statically and under RealtimeSanitizer ([TARGET-EMBEDDED-REALTIME]).
- [ ] Measure per-processor costs on a Raspberry Pi 5 for the budget table ([DEVICE-ENGINE-BUDGET]) as soon as the prototype boots, and re-measure on every engine change.
- [ ] Profile the engine on the Pi 5 itself, not only on the Mac; fix the hottest costs first.
- [ ] The acceptance patch ([DEVICE-HW-COMPUTE]) fits within 70% of a 64-frame block at 48 kHz on one core.

## Device profiles and the virtual device [DEVICE-ENGINE-VIRTUAL]

- [ ] **Profile model ([DEVICE-PROFILE]).** Move [device.td](../../designs/prism-device/device.td) into its package, generate Osprey types, and write the reference profile as data.
- [ ] **Engine program ([DEVICE-ENGINE]).** An Osprey program linking only `@osprey/synth`: load a `PatchDocument`, check roles, nodes and budget, render. It sits beside the other apps as `make -C src/toolchain/targets/<platform> prism-device`, with tests in each platform's tests Makefile and a headless suite in `src/toolchain/targets/tests`.
- [ ] **Controls ([DEVICE-CONTROLS]).** Knob filter, hysteresis, pickup and jump, switches, footswitch toggle, hold and long press, written in Osprey; proofs per [DEVICE-PROOFS].
- [ ] **Link ([DEVICE-LINK]).** tdbin frames over TCP with the 1 MiB bound: Hello, live control, deploy, refusals, get patch. The virtual device listens on localhost only.
- [ ] **Devices panel ([DEVICE-LINK-SHOW]).** Prism lists devices, opens the playing patch or fetches an unknown one, and deploys with the profile check shown first.
- [ ] **Virtual device journeys ([DEVICE-VERIFY]).** In CI: many deploys across slots, every refusal kind, malformed and oversized frames, every control moved with macro and audio assertions, click-free swaps, injected faults.

## Firmware [DEVICE-OS]

Built on the work of Elk Audio OS, Zynthian and PiPedal ([DEVICE-RESEARCH-PRIOR]); mainline PREEMPT_RT rather than Xenomai ([DEVICE-RESEARCH-RT]).

- [ ] Raspberry Pi OS Lite (64-bit) image built with `rpi-image-gen` from a pinned package list: the Raspberry Pi real-time kernel, ALSA, USB gadget modules, the engine as the only service. Image built by `make`, shipped as a CI artifact, never committed. Confirm the real-time kernel package name and that it meets the jitter target ([DEVICE-RESEARCH-OS]).
- [ ] A Raspberry Pi OS (Debian arm64) root is the sysroot for `--target=linux-arm64 --sysroot` ([TARGET-EMBEDDED-LINUX-CROSS]).
- [ ] Read-only root with the OS's overlay file system, atomic data partition, A/B images through `tryboot` with the 10 s health check and fallback.
- [ ] Image signing with the release key; the updater refuses unsigned images.
- [ ] Isolated audio core, `SCHED_FIFO`, locked memory, direct ALSA at 64 frames; 32 and 128 offered, nothing that can exceed 10 ms ([DEVICE-HW-LATENCY-LIMITS]).
- [ ] One dual-role USB-C port ([DEVICE-HW-USB]): device role to a computer (USB MIDI + network link + firmware updates), host role to a USB MIDI keyboard, switched by the Type-C controller; TRS MIDI in and out on the UART; one merged, timestamped event stream.
- [ ] Watchdog restart with the bypass relay held.
- [ ] Engine test without hardware: the same arm64 executable runs in a stock Debian arm64 container (Docker on a Mac, GitHub's ARM Linux runner in CI), answers Hello on the link and takes a deployed patch (no audio timing). Flashing and boot are tested on the prototype with Raspberry Pi Imager.

## Factory [DEVICE-FACTORY]

- [ ] Product bundle built by `make` from a project file and a profile.
- [ ] Provisioning: flash the eMMC over USB-C, write the serial number, run the device test; pass or fail with no partial image.
- [ ] Device test: loopback audio, driven knob wipers, MIDI loopback, bypass with power off; a report per serial number.

## Open questions [DEVICE-PLAN-OPEN]

- **Stabilisation mode.** The repository is in stabilisation mode (no new features). The [instrument plan](instrument.md) completes an already specified feature; the device, firmware, hardware and embedded target work are new features and start only when the user lifts stabilisation for them.
- **Cost.** ARM runner minutes and any hardware runner are new spend; hardware journeys stay local until a runner is approved.
