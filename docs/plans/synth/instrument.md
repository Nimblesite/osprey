# Instrument builder and macros: delivery plan [PRISM-INSTRUMENT-PLAN]

The [instrument builder specification](../../specs/synth/PrismInstrument.md) defines the requirements. The [instrument designs](../../designs/prism-timeline/README.md#instrument-builder-prism-instrument) show Play, Edit and MIDI learn. This plan comes first: the [device plan](device.md) needs macros that already work in Prism.

Builds on the synth package's `Macros` ([advanced-synthesis.md](advanced-synthesis.md), "macro controls"), [DAW-CONTROLS-DIAL], [DAW-MIDI-LEARN] and [DAW-OSC]. Each item lands with a loaded headless and windowed journey with screenshot goldens.

## Macros in the patch [PRISM-INSTRUMENT-PATCH]

- [ ] **Amount in the patch ([PRISM-INSTRUMENT-AMOUNT]).** Add `amount` to `Macro` in the synth package's `graph.td`, regenerate, and save and load it with `PatchDocument`. Package journey: many macros, many amounts, save and reload, and `Macros.apply` gives the same graph.
- [ ] **Smoothing ([PRISM-INSTRUMENT-SMOOTH]).** Macro moves reach the audio through `Smoothers` at 20 ms. Journey: a step in the amount gives no sample step in the output above the smoothed bound.
- [ ] **Proofs ([PRISM-INSTRUMENT-PROOF]).** Lean proofs for `FitMath.macroValue` (monotone, in range, exact ends) and switch positions, derived from the shipped source, each failing on a source mutation, in CI.
- [ ] **Detach on direct edit ([PRISM-INSTRUMENT-AMOUNT]).** Turning a module dial that a macro drives detaches that target with a notice. Journey: detach, undo, re-attach.

## Instrument face [PRISM-INSTRUMENT-FACE]

- [ ] **Model ([PRISM-INSTRUMENT-MACRO]).** Declare the instrument in `daw.td` (controls with id, label, kind, position, size; macro targets with node, parameter, range, curve; MIDI and OSC bindings), generate the code, and store it with the patch in the project file. Journey: save, reopen, compare every field; a deleted module removes its targets with a notice; a second macro on one parameter is refused with the reason.
- [ ] **Views and modes ([PRISM-INSTRUMENT-MODES]).** Patch view and Instrument view on one track patch; Play and Edit mode commands with keys in all three keymaps. Journey: switching renders identical Master PCM before and after.
- [ ] **Play mode ([PRISM-INSTRUMENT-PLAY]).** Only controls drawn; a turn moves every target through `Macros` as one undo step. Journey: turn each control across its range and assert each target's value and the rendered Master audio change (cutoff measured from attenuation), undo restores them.
- [ ] **Edit mode ([PRISM-INSTRUMENT-EDIT]).** Add, drag, resize by handles, grid snap, rename, change kind, delete; one undo step each; fixed control size across window sizes. Journey: many controls, many moves and resizes, screenshots per step, undo and redo of each.
- [ ] **MIDI learn and OSC ([PRISM-INSTRUMENT-REMOTE]).** Learn binds a controller to a control; `/track/<n>/instrument/<control>` address with checked edits and feedback. Needs [DAW-MIDI] input and the [DAW-OSC] listener first. Journey: CC and OSC messages move the on-screen control and the targets; out-of-range messages are refused with the reason.
- [ ] **Accessibility ([DAW-ACCESS]).** Name, value, range, kind, focus order and arrow-key steps reported and tested for every control.

## Device faces [PRISM-INSTRUMENT-DEVICE]

These need the device profile model from the [device plan](device.md#device-profiles-and-the-virtual-device-device-engine-virtual).

- [ ] **Design screenshots first.** The device face with four assigned knobs and a footswitch, assigning a macro to a three-way switch, the Devices panel with a device playing an unknown patch, and a deploy refused for budget, in the style of the [Prism timeline designs](../../designs/prism-timeline/README.md).
- [ ] **Device face ([PRISM-INSTRUMENT-DEVICE]).** Choose a profile in Edit mode; assign macros to fixed controls; refuse kind mismatches with the reason; switch faces with identical Master PCM.
- [ ] **Panel overlay ([PRISM-INSTRUMENT-DEVICE-LABELS]).** Export the SVG overlay; the journey compares it with a golden.
