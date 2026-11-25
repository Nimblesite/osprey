# Prism hardware: delivery plan [DEVICE-PLAN]

The [Instruments and devices specification](../specs/instruments/README.md) defines the requirements: the [instrument builder and its macros](../specs/instruments/PrismInstrument.md) and the [Prism device](../specs/instruments/PrismDevice.md). Designs: [instrument views](../designs/prism-timeline/README.md#instrument-builder-prism-instrument) and [device panel and face](../designs/prism-device/README.md), with the typed model in [device.td](../designs/prism-device/device.td).

The order is fixed: macros work in Prism first, then the same engine runs headless as a virtual device, then on ARM Linux, then on hardware. Each phase ships something usable on its own.

## Gate: the engine must be fast enough [DEVICE-PLAN-SPEED]

The device runs the same engine as Prism, on a slower CPU. Today that engine renders 0.37 s of audio per second on a Mac under load ([DAW-TRANSPORT-LIVE-SPEED] in the [daw plan](daw.md)), and the profile is mostly reference counting and allocation. A Cortex-A76 core is slower again, and the device must also meet the no-allocation callback contract ([SYNTH-RENDER]).

- [ ] The daw's live-speed items are done: 1.5× real time or better on the codex song on the Mac.
- [ ] The render callback allocates and frees nothing: checked by instrumenting the generated callback path, not assumed ([SYNTH-VERIFY]).
- [ ] Measure the per-processor costs for the profile's budget table ([DEVICE-ENGINE-BUDGET]) on a Raspberry Pi 5. Phases 4 and 5 do not start until a useful patch (two oscillators, filter, envelope, delay, eight voices) fits in 70% of a 64-frame block at 48 kHz.

## Phase 1: macros and the instrument face in Prism [PRISM-INSTRUMENT]

Builds on the synth package's `Macros` ([advanced-synthesis.md](advanced-synthesis.md), "macro controls"), [DAW-CONTROLS-DIAL], [DAW-MIDI-LEARN] and [DAW-OSC]. Each item lands with a loaded headless and windowed journey with screenshot goldens.

- [ ] **Amount in the patch ([PRISM-INSTRUMENT-AMOUNT]).** Add `amount` to `Macro` in the synth package's `graph.td`, regenerate, and save and load it with `PatchDocument`. Package journey: many macros, many amounts, save and reload, and `Macros.apply` gives the same graph.
- [ ] **Smoothing ([PRISM-INSTRUMENT-SMOOTH]).** Macro moves reach the audio through `Smoothers` at 20 ms. Journey: a step in the amount gives no sample step in the output above the smoothed bound.
- [ ] **Proofs ([PRISM-INSTRUMENT-PROOF]).** Lean proofs for `FitMath.macroValue` (monotone, in range, exact ends) and switch positions, derived from the shipped source, each failing on a source mutation, in CI.
- [ ] **Model ([PRISM-INSTRUMENT-MACRO]).** Declare the instrument in `daw.td` (controls with id, label, kind, position, size; macro targets with node, parameter, range, curve; MIDI and OSC bindings), generate the code, and store it with the patch in the project file. Journey: save, reopen, compare every field; a deleted module removes its targets with a notice; a second macro on one parameter is refused with the reason.
- [ ] **Views and modes ([PRISM-INSTRUMENT-MODES]).** Patch view and Instrument view on one track patch; Play and Edit mode commands with keys in all three keymaps. Journey: switching renders identical Master PCM before and after.
- [ ] **Play mode ([PRISM-INSTRUMENT-PLAY]).** Only controls drawn; a turn moves every target through `Macros` as one undo step. Journey: turn each control across its range and assert each target's value and the rendered Master audio change (cutoff measured from attenuation), undo restores them.
- [ ] **Edit mode ([PRISM-INSTRUMENT-EDIT]).** Add, drag, resize by handles, grid snap, rename, change kind, delete; one undo step each; fixed control size across window sizes. Journey: many controls, many moves and resizes, screenshots per step, undo and redo of each.
- [ ] **MIDI learn and OSC ([PRISM-INSTRUMENT-REMOTE]).** Learn binds a controller to a control; `/track/<n>/instrument/<control>` address with checked edits and feedback. Needs [DAW-MIDI] input and the [DAW-OSC] listener first. Journey: CC and OSC messages move the on-screen control and the targets; out-of-range messages are refused with the reason.
- [ ] **Accessibility ([DAW-ACCESS]).** Name, value, range, kind, focus order and arrow-key steps reported and tested for every control.
- [ ] **Detach on direct edit ([PRISM-INSTRUMENT-AMOUNT]).** Turning a module dial that a macro drives detaches that target with a notice. Journey: detach, undo, re-attach.

## Phase 2: device faces and the virtual device [DEVICE-ENGINE-VIRTUAL]

- [ ] **Profile model ([DEVICE-PROFILE]).** Move [device.td](../designs/prism-device/device.td) into its package, generate Osprey types, and write the reference profile as data.
- [ ] **Device face ([PRISM-INSTRUMENT-DEVICE]).** Choose a profile in Edit mode; assign macros to fixed controls; refuse kind mismatches with the reason; switch faces with identical Master PCM. Design screenshots first, as listed in the [device designs](../designs/prism-device/README.md#device-face-in-prism-prism-instrument-device).
- [ ] **Panel overlay ([PRISM-INSTRUMENT-DEVICE-LABELS]).** Export the SVG overlay; journey compares it with a golden.
- [ ] **Engine program ([DEVICE-ENGINE]).** An Osprey program linking only `@osprey/synth`: load a `PatchDocument`, check roles, nodes and budget, render. Lives beside the other apps as `make -C src/toolchain/targets/<platform> prism-device`, with tests in each platform's tests Makefile and a headless suite in `src/toolchain/targets/tests`.
- [ ] **Controls ([DEVICE-CONTROLS]).** Knob filter, hysteresis, pickup and jump, switches, footswitch toggle, hold and long press, written in Osprey; proofs per [DEVICE-PROOFS].
- [ ] **Link ([DEVICE-LINK]).** tdbin frames over TCP with the 1 MiB bound; Hello, live control, deploy, refusals, get patch. The virtual device listens on localhost only.
- [ ] **Devices panel ([DEVICE-LINK-SHOW]).** Prism lists devices, opens the playing patch or fetches an unknown one, deploys with the profile check shown first.
- [ ] **Virtual device journeys ([DEVICE-VERIFY]).** In CI: many deploys across slots, every refusal kind, malformed and oversized frames, every control moved with macro and audio assertions, click-free swaps, injected faults.

## Phase 3: ARM Linux target [DEVICE-ENGINE-TARGET]

- [ ] `--target=linux-arm64` in the CLI and its spec in [the compiler specs](../specs/compiler/README.md), with a `failscompilation/` case for anything the target refuses.
- [ ] The hardened C runtime builds for `aarch64-unknown-linux-gnu`.
- [ ] The test corpus runs under ARC on GitHub's ARM Linux runner in the test phase, with coverage.

## Phase 4: firmware [DEVICE-OS]

- [ ] Buildroot tree with pinned sources: PREEMPT_RT kernel, BusyBox, ALSA, USB gadget, the engine as the only service. Image built by `make`, shipped as a CI artifact, never committed.
- [ ] Read-only squashfs root, atomic data partition, A/B images with the 10 s health check and fallback.
- [ ] Image signing with the release key; the updater refuses unsigned images.
- [ ] Isolated audio core, real-time priority, locked memory, direct ALSA at 64 frames × 2.
- [ ] USB composite gadget (MIDI + network link), USB MIDI host, TRS MIDI UART; one merged, timestamped event stream.
- [ ] Watchdog restart with the bypass relay held.
- [ ] QEMU boot test in CI: the image boots, the engine starts and answers Hello on the link (no audio).

## Phase 5: hardware [DEVICE-HW]

- [ ] **Prototype:** Raspberry Pi 5, HiFiBerry DAC+ADC Pro, MCP3208 with eight knobs, toggles and footswitches on a breadboard, TRS MIDI on the UART. Run the hardware journeys ([DEVICE-VERIFY]) locally: boot to sound, round-trip latency, 24-hour run, power pulled during deploy and update.
- [ ] **Carrier board:** KiCad schematic and layout for Compute Module 5, PCM5122, PCM1863, instrument buffer, bypass relay, MCP3208 and reference, MIDI, power. Fabrication outputs generated by `make`, never committed.
- [ ] **Enclosure:** 1590XX drilling template and panel overlay from the device face.
- [ ] Repeat every hardware journey on the carrier board.

## Phase 6: factory [DEVICE-FACTORY]

- [ ] Product bundle built by `make` from a project file and a profile.
- [ ] Provisioning command: flash, serial number, device test, pass or fail with no partial image.
- [ ] Test jig and device test: loopback audio, driven knob wipers, MIDI loopback, bypass with power off; a report per serial number.

## Open questions [DEVICE-PLAN-OPEN]

- **Stabilisation mode.** The repository is in stabilisation mode (no new features). Phase 1 completes an already specified feature; phases 2–6 and the `linux-arm64` target are new features and start only when the user lifts stabilisation for them.
- **Cost.** A hardware CI runner and the ARM Linux runner minutes are new spend; hardware journeys stay local until a runner is approved.
- **Codec parts.** The PCM5122 and PCM1863 are chosen because a mainline-supported prototype board carries both. Confirm availability and noise figures before the carrier board is laid out.
