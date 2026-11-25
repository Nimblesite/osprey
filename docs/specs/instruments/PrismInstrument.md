# Instrument builder [PRISM-INSTRUMENT]

Status: specified, not built. The synth package's `Macros` module validates and applies macros today ([SYNTH-ADVANCED-PRISM]); Prism has no instrument face yet. The [Prism hardware plan](../../plans/prism-hardware.md) tracks delivery. The [design screenshots](../../designs/prism-timeline/README.md#instrument-builder-prism-instrument) show Play, Edit and MIDI learn.

This contract belongs to [the daw](../applications/Daw.md): an instrument is stored in the project file and played through the project graph ([DAW-GRAPH-OUTPUT]).

An instrument is a patch with a performer's face on it. The patch keeps its modules and cables; the instrument hides them and shows only the controls the user chose. Each control is a **macro**: one knob, slider, toggle or pad that moves one or more module parameters at once (Brightness turning a filter's cutoff up and its resonance down, for example). The instrument is stored with its patch in the project file ([DAW-PROJECT-PATCH]), so reopening the project restores the same face.

- **Two modes ([PRISM-INSTRUMENT-MODES]).** A patch has a Patch view (the canvas) and an Instrument view. The Instrument view is in Play mode or Edit mode, switched by a command with a shortcut in every keymap ([DAW-KEYS]). Switching views or modes never changes a value or the sound.
- **Play mode ([PRISM-INSTRUMENT-PLAY]).** Only the instrument's controls are shown; no module, cable or socket is visible. Turning a control moves its macro as one undoable edit ([DAW-CONTROLS], [DAW-HUMAN]), and the patch is heard through Master as always ([DAW-GRAPH-OUTPUT]). Notes from the MIDI keyboard play the instrument as they play the track ([DAW-MIDI-PLAY]).
- **Edit mode ([PRISM-INSTRUMENT-EDIT]).** The user adds a control from the list of control kinds, then moves it by dragging, resizes it by its handles and places it anywhere on the instrument's face, snapping to the Prism spacing grid. They rename its label, choose its kind and delete it. Controls keep their size when the window resizes ([DAW-LAYOUT-FIXED-SCALE]). Each change is one undo step.
- **Macro mappings ([PRISM-INSTRUMENT-MACRO]).** In Edit mode a control lists its targets: a module parameter each, with a range (from and to, which may be reversed) and a curve. One control can drive many parameters, and one parameter can be driven by at most one macro, so no two controls fight over a value. A target whose module is deleted from the patch is removed from the macro with a notice naming it; a macro left without targets is shown as unmapped, never silently dropped. Mappings use the synth package's `Macros` (declared ranges, curves, smoothing), so a macro moves exactly the values a dial on the module would.
- **MIDI and OSC control ([PRISM-INSTRUMENT-REMOTE]).** Each control can be bound to a MIDI controller by moving it while the control is in learn ([DAW-MIDI-LEARN]), and has an OSC address `/track/<n>/instrument/<control>` ([DAW-OSC-ADDRESS]) so a phone surface such as TouchOSC drives it. Bindings are stored with the instrument. A remote change goes through the same checked edit as a turn on screen ([DAW-OSC-CHECK]), and the control on screen follows it; changed values are sent back to subscribed OSC peers ([DAW-OSC-FEEDBACK]).
- **Accessible ([DAW-ACCESS]).** Every instrument control reports its user-given name, value, range and kind, takes keyboard focus in its layout order and steps by arrow keys.

## Macros in the patch [PRISM-INSTRUMENT-PATCH]

A macro belongs to the patch, not to the screen. The synth package's `PatchDocument` already holds `macros` beside its nodes and cables ([SYNTH-GRAPH]), so a patch saved, copied to another track or deployed to a device carries its macros with it. The face (control kinds, positions, sizes and labels) is stored beside the patch in the project file, because it is presentation. A macro's current amount, from 0 to 1, is saved with the patch, so reopening the project or loading the patch on a device starts from the same sound.

- **Amount is the only state ([PRISM-INSTRUMENT-AMOUNT]).** A macro stores its amount; each target value is computed from it by `Macros.apply` and never stored twice. Turning a module dial that a macro drives detaches that target from the macro with a notice naming both, so the two never disagree.
- **Smoothing ([PRISM-INSTRUMENT-SMOOTH]).** Amount changes reach the audio as ramps through the synth package's `Smoothers` with a declared time (default 20 ms), never as steps, in Prism and on a device alike ([SYNTH-RENDER]).
- **Curves ([PRISM-INSTRUMENT-CURVE]).** A target's curve is its `exponent`, from 0.25 to 4, as `Macros` checks today. Reversed ranges are allowed. The mapping from amount to value is monotone and stays inside the target's range; that is a proof obligation ([PRISM-INSTRUMENT-PROOF]).

## Device faces [PRISM-INSTRUMENT-DEVICE]

A screen face can be any shape; a hardware face cannot. A **device profile** ([DEVICE-PROFILE]) lists a device's physical controls: their kinds, positions and labels. Choosing a profile for a patch's instrument switches Edit mode to a **device face**: the controls are the device's own, drawn where they sit on the box, and cannot be added, moved, resized or deleted. The user assigns a macro to each physical control instead.

- **Assigning ([PRISM-INSTRUMENT-DEVICE-ASSIGN]).** Each physical control takes at most one macro; each macro sits on at most one physical control. A macro with no physical control can still be set from Prism and by MIDI. A physical control with no macro is shown as unassigned on screen and does nothing on the device.
- **Kinds must fit ([PRISM-INSTRUMENT-DEVICE-KIND]).** A knob takes a continuous macro. A two-way switch takes a macro played at 0 or 1, and a three-way switch at 0, 0.5 or 1. A footswitch takes a macro it toggles or holds, chosen per assignment. A macro whose control kind does not match is refused with the reason, never converted silently.
- **One patch, many faces ([PRISM-INSTRUMENT-DEVICE-FACES]).** A patch keeps its screen face and one device face per profile it has been assigned to. Switching between them never changes a value or the sound ([PRISM-INSTRUMENT-MODES]).
- **Panel labels ([PRISM-INSTRUMENT-DEVICE-LABELS]).** From a device face, Prism exports a printable panel overlay: an SVG at the profile's panel size with each control's macro name under it. The overlay is generated when asked and never committed ([REPO-NO-ARTIFACTS]).

## Proof obligations [PRISM-INSTRUMENT-PROOF]

Per [MATH-CODE], the mapping formulas are written in Osprey and proved in Lean against the shipped source:

- `FitMath.macroValue` is monotone in the amount for every curve from 0.25 to 4, stays within the target's range for every amount from 0 to 1, and returns the range's ends at 0 and 1.
- A switch position maps to exactly one of its declared amounts.
- Each proof fails on a relevant mutation of the source, and runs in CI with the other synth proofs ([SYNTH-DSP-PROOFS-PRODUCTION]).
