# Prism device and hardware: delivery plan [DEVICE-PLAN]

## 🚨 DIRECTIVE: PERFORMANCE OPTIMISATION NECESSARY, ESPECIALLY ON EMBEDDED DEVICES 🚨 [DEVICE-PERF]

**Performance optimisation is not optional and never finished.** The engine that runs in Prism is the engine that runs on the device, and the device has a slower CPU and a hard 1.33 ms deadline per 64-frame block. Today the engine renders 0.37 s of audio per second on a Mac, mostly in reference counting and allocation ([DAW-TRANSPORT-LIVE-SPEED]). That must improve many times over, and it is worked on **in parallel with everything else**, not as a gate that holds other work back.

- Every change to the engine, the synth package or the code the compiler generates for them is measured for speed, and a change that makes the device slower is a defect.
- The render callback allocates, frees and blocks nothing ([TARGET-EMBEDDED-REALTIME]).
- Per-processor costs are measured on the device's own CPU ([DEVICE-ENGINE-BUDGET]), recorded, and only go down.
- The acceptance stereo patch must run at 70% or less of the deadline on one core ([DEVICE-HW-COMPUTE]). Until it does, that is the most important open item on the device.

The [Prism device specification](../../specs/synth/PrismDevice.md) defines the engine, firmware, controls, link and factory. The board and enclosure are under [Hardware](#hardware-device-hw-plan) below, and the compiler work has the [embedded targets plan](../embedded-targets.md). Research and citations are in the [hardware specification](../../specs/synth/PrismDeviceHardware.md#research-findings-device-research).

Macros work in Prism first ([instrument plan](instrument.md)). Then the concept is tested on the Mac with the virtual device, before any hardware is bought ([DEVICE-VERIFY-CONCEPT]). After that the embedded target, the firmware, the prototype and the board proceed in parallel, with performance optimisation running beside all of them ([DEVICE-PERF]). Each phase ships something usable on its own.

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
- [ ] **Concept test on the Mac ([DEVICE-VERIFY-CONCEPT]).** `make -C src/toolchain/targets/macos prism-device` starts the virtual device; from Prism, deploy a source patch and an effect patch, play one from a MIDI keyboard and run audio through the other, turn every on-screen control, switch slots, and see the playing patch in the Devices panel. This is the gate for ordering parts.
- [ ] **Virtual device journeys ([DEVICE-VERIFY]).** In CI: many deploys across slots, every refusal kind, malformed and oversized frames, every control moved with macro and audio assertions, click-free swaps, injected faults.

## Firmware [DEVICE-OS]

Built on the work of Elk Audio OS, Zynthian and PiPedal ([DEVICE-RESEARCH-PRIOR]); the same OS and kernel they use by default ([DEVICE-RESEARCH-OS]).

- [ ] Raspberry Pi OS Lite (64-bit) image built with `rpi-image-gen` from a pinned package list: the standard Raspberry Pi kernel, ALSA, USB gadget modules, the engine as the only service. Image built by `make`, shipped as a CI artifact, never committed.
- [ ] A Raspberry Pi OS (Debian arm64) root is the sysroot for `--target=linux-arm64 --sysroot` ([TARGET-EMBEDDED-LINUX-CROSS]).
- [ ] Read-only root with the OS's overlay file system, atomic data partition, A/B images through `tryboot` with the 10 s health check and fallback.
- [ ] Image signing with the release key; the updater refuses unsigned images.
- [ ] Kernel swap kept as one line in the package list: build the same image with Raspberry Pi's real-time kernel only if the 24-hour journey records dropouts ([DEVICE-OS-KERNEL]).
- [ ] Isolated audio core, `SCHED_FIFO`, locked memory, direct ALSA at 64 frames; 32 and 128 offered, nothing that can exceed 10 ms ([DEVICE-HW-LATENCY-LIMITS]).
- [ ] One dual-role USB-C port ([DEVICE-HW-USB]): device role to a computer (USB MIDI + network link + firmware updates), host role to a USB MIDI keyboard, switched by the Type-C controller; TRS MIDI in and out on the UART; one merged, timestamped event stream.
- [ ] Watchdog restart with the bypass relay held.
- [ ] Engine test without hardware: the same arm64 executable runs in a stock Debian arm64 container (Docker on a Mac, GitHub's ARM Linux runner in CI), answers Hello on the link and takes a deployed patch (no audio timing). Flashing and boot are tested on the prototype with Raspberry Pi Imager.

## Factory [DEVICE-FACTORY]

- [ ] Product bundle built by `make` from a project file and a profile.
- [ ] Provisioning: flash the eMMC over USB-C, write the serial number, run the device test; pass or fail with no partial image.
- [ ] Device test: loopback audio, driven knob wipers, MIDI loopback, bypass with power off; a report per serial number.

## Hardware [DEVICE-HW-PLAN]

The [hardware specification](../../specs/synth/PrismDeviceHardware.md) defines the board, latency budget, compute and the AUD 500 budget, and holds the research and prices behind them. The enclosure is drawn in the [device designs](../../designs/prism-device/README.md#enclosure-device-hw-enclosure), with its own release checks. Hardware work proceeds in parallel with performance optimisation ([DEVICE-PLAN-SPEED]).

### Prototype [DEVICE-HW-PROTOTYPE]

No part is ordered until the concept test passes on the Mac with the virtual device ([DEVICE-VERIFY-CONCEPT]).

- [ ] Order a Raspberry Pi 5 4 GB, HiFiBerry DAC2 ADC Pro, MCP3208, pots, toggles, footswitches, H11L1 and TRS jacks; record actual prices against the [budget](../../specs/synth/PrismDeviceHardware.md#budget-device-hw-budget).
- [ ] Bring up the firmware on the prototype: audio at 64 frames, knobs, switches, TRS MIDI, and the USB-C port in both roles (computer link and USB MIDI keyboard).
- [ ] Measure audio round trip and note to sound with loopback cables: mean, worst case and jitter over 24 hours at full budget. Pass: round trip under 5 ms, note to sound under 5.5 ms worst case, never over 10 ms, jitter under 1 ms ([DEVICE-HW-LATENCY]).
- [ ] Measure the acceptance stereo patch's share of the deadline on one core and the temperature, and feed both to the performance track ([DEVICE-PERF]). Pass: 70% or less with no thermal throttling ([DEVICE-HW-COMPUTE]).
- [ ] Power pulled during deploy and during update; bypass with the power off.
- [ ] 3D-printed starter case ([DEVICE-HW-ENCLOSURE-PRINTED]): a printed box that fits the prototype and holds the controls and jacks in the panel layout. Model files in the repository; no rugged tests apply.

### Prism board [DEVICE-HW-BOARD]

- [ ] Test USB role switching on Raspberry Pi's CM5 IO board with USB MIDI and the link running. Pass → the second USB-C footprint stays unfitted; fail → it is fitted ([DEVICE-HW-USB-RISK]).
- [ ] KiCad schematic from Raspberry Pi's CM5 IO board design plus the audio, control, MIDI, relay and power circuits proven on the prototype.
- [ ] Layout: four layers, the analog ground plane joined once under the PCM3060, the buck converter and USB away from the codec and clocks.
- [ ] Fabrication outputs, bill of materials and assembly files generated by `make`, never committed.
- [ ] Order five assembled boards; record the real unit cost against the budget.
- [ ] Repeat every prototype journey on the board in the enclosure, on both barrel and USB-C power.

### Enclosure [DEVICE-HW-ENCLOSURE]

The die-cast road case is the eventual case and comes last, with the Prism board; the starter case above comes first. Tracked in the [enclosure release checks](../../designs/prism-device/README.md#enclosure-release-checks).

## Open questions [DEVICE-PLAN-OPEN]

- **Stabilisation mode.** The repository is in stabilisation mode (no new features). The [instrument plan](instrument.md) completes an already specified feature; the device, firmware, hardware and embedded target work are new features and start only when the user lifts stabilisation for them.
- **Cost.** ARM runner minutes and any hardware runner are new spend; hardware journeys stay local until a runner is approved.
