# Prism: routing, instruments, advanced synthesis and the device [PRISM-ROUTING] [PRISM-INSTRUMENT-PLAN] [SYNTH-ADVANCED] [DEVICE-PLAN]

Prism is an editable instrument graph on `@osprey/synth`: sources, effects, MIDI generators, modulators and VST3 host nodes, connected by compatible ports, with macros and instrument faces on the same patch, heard through a continuous device queue. The same package carries the advanced synthesis families and the engine the Perception device runs, where every change is measured against a 1.33 ms deadline per 64-frame block [DEVICE-PERF]. Device, firmware, hardware and embedded-target work are new features and start only when the user lifts stabilisation mode for them [DEVICE-PLAN-OPEN].

Contracts: [Synthesis.md](../../specs/synth/Synth.md) ([SYNTH-GRAPH], [SYNTH-SIGNALS], [SYNTH-RENDER], [SYNTH-DSP], [SYNTH-HOST-VST], [SYNTH-ADVANCED]) and the [device](../../specs/synth/Perception.md#perception-device-device) and [instrument builder](../../specs/synth/Perception.md#instrument-builder-prism-instrument) parts of Perception. Designs: [Prism timeline](../../designs/prism-timeline/README.md) and [Prism device](../../designs/prism-device/README.md). Checks: `make -C src/osprey/packages synth`, `make -C src/toolchain/targets/tests synth` and `synth-vst`, `make -C src/toolchain/targets/macos prism`.

## Graph editor [PRISM-ROUTING-INTEGRATION] [PRISM-ROUTING-PRESENTATION] [PRISM-ROUTING-CABLES] [PRISM-ROUTING-LAYOUT] [PRISM-ROUTING-TIMELINE]

- [ ] A struck key on the Prism skin's keyboard plays the track's real patch graph (the fixed-patch demo is deleted; no key plays yet), through Windows' `--audio-worker` as on the Mac; the picker advertises MIDI generators, modulation processors and VST nodes only once they work: `make -C src/toolchain/targets/tests synth` [PRISM-ROUTING-INTEGRATION].
- [ ] Movable node positions with screenshot and windowed verification; node geometry, pointer interaction and accessibility bounds checked in the real window [PRISM-ROUTING-INTEGRATION] [PRISM-ROUTING-LAYOUT].
- [ ] Adaptive canvas resizing restored in the unified editor; the native window matches the original Prism design (fixed-studio tests do not validate the graph editor) [PRISM-ROUTING-PRESENTATION].
- [ ] Real mouse dragging and cable deletion verified in the rebuilt window; the windowed regression journey captured [PRISM-ROUTING-CABLES].
- [ ] Nested patch racks in track and bus graphs: the design walkthrough with a second send from an occupied source outlet and a refused zero-delay loop [PRISM-ROUTING-TIMELINE].
- [ ] The detached Timeline keeps session, playhead, selected track and undo history across both windows; rendered states compared with the design on FerroUI [PRISM-ROUTING-TIMELINE].

## Graph and hosted-playback acceptance [PRISM-ROUTING-VERIFY] [SYNTH-RENDER-BLOCKS]

- [ ] Graph-edit journey through visible controls: many instances, chains and fan-out, incompatible ports and cycles refused, routes removed and reconnected, a connected node deleted, the snapshot unchanged on rejection [PRISM-ROUTING-VERIFY].
- [ ] Audio journey: sounds differ before and after a routing change; oscillator and filter state kept under parameter-only edits; block partitions and note-off ownership exercised [PRISM-ROUTING-VERIFY] [SYNTH-RENDER-BLOCKS].
- [ ] Hosting loads an existing VST3 binary; discovery and loading never run on the audio callback; scan failures, unsupported buses and absent binaries are typed errors; the selected SDK revision and loaded-binary evidence recorded; exact per-sample automation response and separate controller-state persistence in the worker: `make -C src/toolchain/targets/tests synth-vst` [SYNTH-HOST-VST] [PRISM-ROUTING-VST-WORKER].
- [ ] The sample-positioned MIDI source and stereo components run through the audition adapter; a mono adapter that ignores them fails [PRISM-ROUTING-MIDI].
- [ ] Hosted patch journey (`synth-vst`): a note generator through real instrument and effect binaries, controls edited, the stereo tail verified, PCM captured; routed LFO and hosted parameter edits keep frozen graph ownership and refuse stale replies and non-automatable edits [PRISM-ROUTING-AUDIO-HOSTED] [PRISM-ROUTING-MODULATION] [PRISM-ROUTING-MODULATION-HOSTED].
- [ ] Oscillator MIDI input ([PRISM-ROUTING-MIDI-OSCILLATOR]): numerical pitch and velocity, note ownership, exact block-boundary gating and long playback [SYNTH-HOST-MIDI-OSCILLATOR]; release keeps its length and remaining frames, latches only on the matching note-off, survives repeated or unrelated note-offs and cancels on retrigger without resetting phase [SYNTH-HOST-MIDI-OSCILLATOR-RELEASE]; attack, crossfades and live topology state migration before click-free realtime MIDI instruments are claimed.

## Continuous playback and the device queue [PRISM-ROUTING-AUDIO] [PRISM-ROUTING-SESSIONS] [PRISM-ROUTING-CONTINUOUS] [PRISM-ROUTING-THROUGHPUT] [PRISM-ROUTING-GRAINS]

- [ ] The desktop host owns session lifetimes, per-request watchdogs and explicit shutdown; malformed or nonzero receipts refused, input and output bounded, children cleared on shutdown, workers removed after every transaction failure [PRISM-ROUTING-AUDIO-PENDING] [SYNTH-HOST-STREAM-SESSION] [SYNTH-HOST-STREAM-SESSION-DESKTOP]; Windows and Linux execution verified.
- [ ] The retained graph feeds a continuous audio device queue with live graph and parameter changes; edits apply transactionally without restarting playback; the callback only publishes returned buffers and counters; graph block scheduling; non-macOS device adapters, or explicit unsupported-adapter failures [SYNTH-DEVICE-QUEUE] [SYNTH-RENDER-BLOCKS-GRAPH] [PRISM-ROUTING-SESSIONS] [PRISM-ROUTING-CONTINUOUS].
- [ ] Sustained zero-starvation playback (granular included) with speaker output quality and comparative runtime evidence; low-latency scheduling, zero-allocation production, state persistence and latency compensation; a ten-minute maximum-workload run with zero starvation [PRISM-ROUTING-GRAINS] [PRISM-ROUTING-THROUGHPUT].
- [ ] Reproduced playback failures resolved before smooth sustained playback is claimed; the real device adapter and explicit playback-failure handling pass the windowed and sustained-audio gates [PRISM-ROUTING-AUDIO].
- [ ] The Undo/Redo journey that removes and reapplies Delay and Mixer nodes: native execution and screenshot evidence [PRISM-ROUTING-GRAINS].
- [ ] Applied graph and parameter revisions and component activity published through Signals with no UI dependency in the package [PRISM-ROUTING-DELIVERY] [PRISM-ROUTING-DIAGNOSTICS].

## Macros in the patch [PRISM-INSTRUMENT-PATCH]

Macro controls build on the synth package's `Macros`, [DAW-CONTROLS-DIAL], [DAW-MIDI-LEARN] and [DAW-OSC], and work in Prism before device faces depend on them.

- [ ] **Amount ([PRISM-INSTRUMENT-AMOUNT]).** `amount` on `Macro` in `graph.td`, regenerated, saved and loaded with `PatchDocument`; journey: many macros, many amounts, save and reload, `Macros.apply` gives the same graph.
- [ ] **Smoothing ([PRISM-INSTRUMENT-SMOOTH]).** Macro moves reach the audio through `Smoothers` at 20 ms; a step in the amount gives no sample step above the smoothed bound.
- [ ] **Proofs ([PRISM-INSTRUMENT-PROOF]).** Lean proofs for switch positions, derived from the shipped source, each failing on a source mutation, in CI (`FitMath.macroValue`'s exact ends, monotonicity and range are proved in `ProductionSpectral.lean.in`).
- [ ] **Detach on direct edit ([PRISM-INSTRUMENT-AMOUNT]).** Turning a module dial a macro drives detaches that target with a notice; journey: detach, undo, re-attach.

## Instrument face [PRISM-INSTRUMENT-FACE]

- [ ] **Model ([PRISM-INSTRUMENT-MACRO]).** Declare the instrument in `daw.td` (controls with id, label, kind, position, size; macro targets with node, parameter, range, curve; MIDI and OSC bindings), generate the code, store it with the patch in the project file; journey: save, reopen, compare every field; a deleted module removes its targets with a notice; a second macro on one parameter is refused with the reason.
- [ ] **Views and modes ([PRISM-INSTRUMENT-MODES]).** Patch view and Instrument view on one track patch; Play and Edit mode commands with keys in all three keymaps; switching renders identical Master PCM.
- [ ] **Play mode ([PRISM-INSTRUMENT-PLAY]).** Only controls drawn; a turn moves every target through `Macros` as one undo step; journey: turn each control across its range, assert each target's value and the rendered Master audio change (cutoff from attenuation), undo restores them.
- [ ] **Edit mode ([PRISM-INSTRUMENT-EDIT]).** Add, drag, resize by handles, grid snap, rename, change kind, delete; one undo step each; fixed control size across window sizes; journey: many controls, many moves and resizes, screenshots per step, undo and redo of each.
- [ ] **MIDI learn and OSC ([PRISM-INSTRUMENT-REMOTE]).** Learn binds a controller to a control; `/track/<n>/instrument/<control>` with checked edits and feedback; needs [DAW-MIDI] input and the [DAW-OSC] listener first; journey: CC and OSC messages move the on-screen control and the targets; out-of-range messages refused with the reason.
- [ ] **Accessibility ([DAW-ACCESS]).** Name, value, range, kind, focus order and arrow-key steps reported and tested for every control.

## Device faces [PRISM-INSTRUMENT-DEVICE]

These need the profile model under [Device profiles and the virtual device](#device-profiles-and-the-virtual-device-device-engine-virtual).

- [ ] Design screenshots first, in the style of the [Prism timeline designs](../../designs/prism-timeline/README.md): the device face with four assigned knobs and a footswitch, a macro assigned to a three-way switch, the Devices panel with a device playing an unknown patch, a deploy refused for budget [PRISM-INSTRUMENT-DEVICE].
- [ ] Device face: choose a profile in Edit mode, assign macros to fixed controls, refuse kind mismatches with the reason, switch faces with identical Master PCM [PRISM-INSTRUMENT-DEVICE].
- [ ] Panel overlay: export the SVG overlay; the journey compares it with a golden [PRISM-INSTRUMENT-DEVICE-LABELS].

## Advanced synthesis: core contracts [SYNTH-ADVANCED-CORE]

- [ ] Shared foundation exercised with invalid state, rate changes, asset generations, irregular partitions, unavailable models, cancellation and migration; dimensions, products, limits, finite data and model operators validated before preparation; format, processor state and asset lifetimes threaded through scalar and graph execution; producer queues, fault publication, latency, cancellation ownership and device starvation measured; seeds, applied parameter semantics and hashes kept across save/reload and relinking; wasm32 package execution, explicit backend unavailability, GC and ARC on supported hosts [PRISM-ADVANCED-FOUNDATION].
- [ ] Schema changes round-trip old graphs and exercise missing and unknown fields; grain scheduling, body state and neural request descriptors carried through the public preparation and block interfaces, validation and observations; asset decoding, model execution and generated-asset jobs bounded, reusing worker ownership and cancellation with a dedicated typed model protocol [PRISM-ADVANCED-BASELINE].
- [ ] Native Osprey formulas with proofs of their parsed bodies for windows, interpolation, modal transitions, waveguide tuning, energy accounting and controller mappings, checked for reference parity [PRISM-ADVANCED-BASELINE] [SYNTH-DSP-PROOFS].

- [ ] `ChannelLayout` (ordered named channels; stereo default, 5.1/7.1/7.1.4/any N) declared in `graph.td` and carried by every port [SYNTH-GRAPH-CHANNELS].
- [ ] Layouts other than stereo on a track, `Grains` stereo summing and the `GraphClips` mono-to-stereo component [DAW-MODULAR-LAYOUT].
- [ ] Track and Master output layouts in the project file (`daw.td`); sample import keeps every source channel; the macOS device queue plays any layout CoreAudio offers [SYNTH-GRAPH-CHANNELS].
- [ ] Polarity (bipolar/unipolar) and control/audio-rate adapter nodes; hertz/normalised stays `Descriptors.fromControl`/`toControl` [SYNTH-ADVANCED-CORE-PORTS].
- [ ] Trigger events and mixed note, parameter and trigger events in one frame, in deterministic sample-offset order (same-frame parameter events and note on/off across exact partitions are already tested) [SYNTH-ADVANCED-CORE-PORTS].
- [ ] Decoding, table preparation, pool allocation and transform construction outside the audio callback; parallel routes aligned in preparation from each component's `Timings.latency`; a grain cloud added to a running graph starts without a restart (`GraphRebase.rebased`) [SYNTH-ADVANCED-CORE-RESOURCES].
- [ ] No allocation, blocking lock, file or network access, log formatting or observer callback in production render paths, by the realtime render-path rules [SYNTH-ADVANCED-CORE-RESOURCES] [SYNTH-ADVANCED-ORDER].

## Advanced synthesis: Prism integration [SYNTH-ADVANCED-PRISM]

- [ ] Prism logo, original node styling, visible cables, inspector and bottom-docked closable keyboard preserved: `make -C src/toolchain/targets/tests synth` [SYNTH-ADVANCED-PRISM].
- [ ] Module browser grouped by Sources, Grains, Physical, Spectral, Modulators, Effects, Utilities and Output with search and drag/click insertion (the native picker lists all 29 kinds as "Category · Title") [SYNTH-ADVANCED-PRISM].
- [ ] Cable creation and removal, node movement and removal, Undo and Redo work for every advanced family [SYNTH-ADVANCED-PRISM].
- [ ] Controls bound to applied parameter Signals; loading, failure, pending and rejected states shown on the affected control and node; an unavailable sample, model or plug-in is never a silent working instrument [SYNTH-ADVANCED-PRISM].
- [ ] Macro controls mapped to typed parameters with declared ranges and smoothing in Prism (the library part, `Macros` and `Smoothers`, is done) [SYNTH-ADVANCED-PRISM].
- [ ] The same control commands exposed to Light Orchestra and phone adapters without networking or session state in the synth library [SYNTH-ADVANCED-PRISM].
- [ ] Palette cards, observation Signals and saved patch files for every family; complete patches saved with resources, seeds, module versions, automation and controller mappings; incompatible or missing resources load explicitly [SYNTH-ADVANCED-PRISM] [SYNTH-ADVANCED-DELIVERY].

## Advanced synthesis: granular [SYNTH-ADVANCED-GRAINS]

The sample-backed Grain Cloud slice is delivered before another family expands [SYNTH-ADVANCED-MILESTONES].

- [ ] Independent sample-source and scheduler graph factories with event and control modulation ports, Grain Scheduler and Sample Source in the module browser, typed modulation adapters, then granular presets before physical models; no Prism-only synthesis path [SYNTH-ADVANCED-GRAINS-INTEGRATION] [SYNTH-ADVANCED-MILESTONES-NEXT].
- [ ] Source selection, waveform and position, density, duration, pitch, spread, seed and freeze controls in the existing inspector design, in meaningful units and bounded, with Prism's original artwork [SYNTH-ADVANCED-GRAINS-INTEGRATION] [SYNTH-ADVANCED-MILESTONES-NEXT].
- [ ] Manual journey: add Grain Cloud, load a sample, connect stereo output, play several notes, edit controls, stop, remove the cable and node [SYNTH-ADVANCED-MILESTONES-NEXT].
- [ ] Keyboard and phone expression mapped to declared parameters, not instrument-specific commands [SYNTH-ADVANCED-GRAINS-INTEGRATION].
- [ ] Resource identity and scheduler state saved in patches; unavailable samples a visible recoverable error [SYNTH-ADVANCED-GRAINS-INTEGRATION].
- [ ] Distribution controls verified through the Prism inspector (the clip journey covers 2,000 events at three seeds) [SYNTH-ADVANCED-GRAINS-SCHEDULING].
- [ ] A generated channel-policy model; the built-in source kept as an explicit selectable resource; loaded resource name, duration, rate and channel metadata, waveform and current source position shown through bounded observations [SYNTH-ADVANCED-GRAINS-IMPORT] [SYNTH-ADVANCED-MILESTONES-NEXT].
- [ ] An import-during-playback policy, shown clearly, with rendering never blocking while a replacement is prepared; content hash and a portable resolution strategy persisted, missing or changed files giving an explicit recoverable result [SYNTH-ADVANCED-GRAINS-IMPORT].
- [ ] `WaveImport.decode` journeys: every supported encoding, empty, truncated and oversized files, malformed chunk arithmetic, cancellation, stale completions, node removal, repeated replacement, Undo/Redo, missing resources, pitch consistency across sample rates [SYNTH-ADVANCED-GRAINS-IMPORT].
- [ ] Audio-queue starvation traced through producer, transport and device receipts (workload and timings recorded), then the producer bottleneck removed without weakening validation or hiding throughput failure behind a larger queue [SYNTH-ADVANCED-MILESTONES-NEXT].
- [ ] Seeded event traces and window and resampling oracles for reverse and single-frame sources, replacement, capture wrap, zero and maximum density and exhaustion; the shared sample and live kernel with independent pitch and scan, region boundaries, source retention and visible overload counts; Size, Density, Pitch, Scan, Capture, Freeze, waveform and region gestures driven through the Granular card and inspector with audio, applied state and screenshots compared, pitch and alias energy measured against the reference resampler, and a frozen pad and a time-stretched transient demonstrated with reproducible traces [PRISM-ADVANCED-GRANULAR].

## Advanced synthesis: families [SYNTH-ADVANCED-PHYSICAL] [SYNTH-ADVANCED-TABLES] [SYNTH-ADVANCED-SPECTRAL] [SYNTH-ADVANCED-NONLINEAR] [SYNTH-ADVANCED-PHASE]

- [ ] Physical: activity and decay observations on the node rather than decorative waveforms; bowed and blown models, voice allocation, sample-rate state transfer, a mesh and transient-free time-varying coefficients [SYNTH-ADVANCED-PHYSICAL-INTEGRATION] [SYNTH-ADVANCED-PHYSICAL].
- [ ] Physical oracles and journeys: equations, boundary conditions, modal weights and ranges pinned for bars, plates, membranes, strings and tubes with impulse, tuning, decay and energy oracles; convergence failure, joint coupling, automation and material changes exercised before live use; Exciter, Modal Body, String and Tube driven through palette selection, conditional controls, pickup and contact gestures and mode and energy observations; struck metal, damped membrane, plucked, bowed and blown patches saved and reloaded after velocity, pickup, material and coupling changes; tuning and T60, decay and energy bounds and solver-exhaustion evidence collected [PRISM-ADVANCED-PHYSICAL].
- [ ] Tables: prepared wavetable banks with explicit sample rate, table length, phase alignment and resource ownership; table import by FFT resynthesis; crossfaded level switching; Wavetable and Partial Bank cards and resource controls in Prism [SYNTH-ADVANCED-TABLES].
- [ ] Spectral pitch: the 1 cent, 1 dB level and 1 dB ripple acceptance held on plucks and a voice-like source (sines pass at 16 and 48 kHz in `wave_journeys.ospml`) [SYNTH-SPECTRAL-PITCH-EFFECT]; transient handling and time stretching with their own latency contract [SYNTH-ADVANCED-SPECTRAL-MODULES].
- [ ] Spectral morph: cross-frame peak tracking for inharmonic sounds ([failure modes](../../specs/synth/Synth.md#failure-modes)) [SYNTH-SPECTRAL-MORPH].
- [ ] Real-time autotune: YIN-style fundamental with confidence, snapping to key, scale or the timeline's notes, retune speed, amount and humanise, formant-preserving FFT correction, unvoiced frames dry, bounded per-hop work without allocation, declared latency; native Osprey formulas; tests for a 30-cent-flat vibrato line, glide timing, unvoiced noise and block-split invariance [SYNTH-SPECTRAL-AUTOTUNE].
- [ ] Spectral: shared analysis feeding freeze, morph and resynthesis, spectral ports, tail flushing, pitch and time beyond integer-bin shifts, a CPU budget, and displays driven by bounded observer snapshots, never analysis on the UI thread [SYNTH-ADVANCED-SPECTRAL].
- [ ] Delay, Diffuser, Feedback Matrix, Waveshaper, Frequency Shifter and Reverb exposed in Prism; oversampling beyond 2×; delay and matrix as separate nodes; pitch-shifted feedback [SYNTH-ADVANCED-NONLINEAR].
- [ ] Phase and operator modules with editable routing in Prism, modulation depth and scaling adapters, antialiased [SYNTH-ADVANCED-PHASE].

## Advanced synthesis: learned, neural and combined [SYNTH-ADVANCED-LEARNED] [PRISM-ADVANCED-NEURAL] [PRISM-ADVANCED-HYBRID]

- [ ] App controls preview, apply and reject fitted parameters without overwriting a working patch silently (`Fitting` and `FitReports` exist) [SYNTH-ADVANCED-LEARNED].
- [ ] Neural fixtures and backend: redistributable RAVE, DDSP, codec and text-to-audio models selected independently with exact hashes, operator and runtime compatibility, licences and training-data provenance recorded; load, warm-up, execution distribution, memory, hop and lookahead and cancellation benchmarked on Windows and the other target hosts; the backend (ONNX export or TorchScript) chosen only after the models run with bounded operator support [PRISM-ADVANCED-NEURAL].
- [ ] Neural lifecycle: manifest rejection, worker loss, late replies, shape errors and codebook bounds exercised through typed effects, with history across irregular blocks [PRISM-ADVANCED-NEURAL].
- [ ] RAVE transfer and DDSP Audio/MIDI with latency-compensated mixing and pitch confidence; real codec encode and decode fixtures run; generated assets validated with cancellation, preserving prompt, duration, seed and model revision; Preview and Insert exercised independently [PRISM-ADVANCED-NEURAL].
- [ ] Neural UI: model selection, capability and error display, latent or harmonic/noise controls, codec inspection and generation, with missing assets, replacement, stale results, cancellation and save/reload; genuine audio, held-out measurements and lifecycle evidence per capability; backend nondeterminism recorded in replay metadata [PRISM-ADVANCED-NEURAL].
- [ ] Combined body-dependent contacts: analytic controller, then a learned scheduler or texture decoder with energy-constrained forces, so initial body state changes future contact traces only with Interaction enabled; the native energy model and its checked bound proved against the production transition; the analytic baseline measured before training [PRISM-ADVANCED-HYBRID].
- [ ] Joint sound analysis (grain dictionary, contact envelope and material from reference audio) and text-directed contact trajectories, each with its own reconstruction, validity and edit evidence [PRISM-ADVANCED-HYBRID].
- [ ] The Combined module's Texture, Material, Body memory, Interaction and Energy controls, contact map, timeline, energy inspector, numeric and keyboard alternatives and controller selection, compared under matched loudness and latency with routed grains, open-loop contacts, analytic feedback and learned control without energy features; negative results retained [PRISM-ADVANCED-HYBRID].
- [ ] Resampling quality thresholds, membrane and tube reference models, runtime and redistributable model selection, platform memory and performance limits and the hybrid training dataset resolved by their named experiments [PRISM-ADVANCED-DELIVERY].

## Arrangements [SYNTH-ARRANGEMENT]

- [ ] Stereo instruments; `synth::Limiters` stays a module the user places and never lands on Master by itself [SYNTH-ARRANGEMENT] [DAW-TRANSPARENCY].
- [ ] Playback on Windows and Linux (`afplay` is macOS only) [SYNTH-ARRANGEMENT].
- [ ] An intro's tempo and key detected instead of measured by hand [SYNTH-ARRANGEMENT].
- [ ] Pluggable processing (after #427 is merged and the user approves): every processing call site in `@osprey/synth` and the daw listed, one effect declared per kind of processing, and each call site routed through its effect with a default handler byte-identical to today's output on the existing goldens [SYNTH-ARRANGEMENT-EFFECTS].
- [ ] The analog summing handler (channel and bus saturation, band limits, seeded noise that rises with channel count, gain tolerances and crosstalk, rail clipping and slew limiting), each drive defaulting to zero; a test that sat(a) + sat(b) differs from sat(a + b) and that zero drive equals the plain sum; a proof per nonlinear stage [SYNTH-ARRANGEMENT-EFFECTS].

## Advanced synthesis delivery plan [PRISM-ADVANCED-PLAN]

Acceptance ([SYNTH-ADVANCED-VERIFY], the [acceptance matrix](../../specs/synth/Synth.md#acceptance-contract-synth-advanced-verify)) and handoff ([SYNTH-ADVANCED-DELIVERY]). Results are recorded for the stated workload, backend and host; a module is marked production only with measured evidence.

- [ ] Loaded end-to-end patch journeys with repeated same-kind instances, fan-out, modulation, removal, undo, save/load and failures [SYNTH-ADVANCED-VERIFY-CORRECTNESS].
- [ ] Physical tuning, decay and energy bounds, rate changes and extreme parameters; wavetable and additive phase continuity, modulation and table replacement; spectral reconstruction, overlap/add normalisation, latency, freeze/release and transport discontinuities [SYNTH-ADVANCED-VERIFY-CORRECTNESS].
- [ ] Nonlinear and phase outputs compared with independent higher-quality references under level and latency matching; nonfinite data, malformed resources, invalid units, stale revisions, overflow and unavailable resources injected, with explicit failure and preserved prior state [SYNTH-ADVANCED-VERIFY-CORRECTNESS].
- [ ] Allocation, locking and unbounded operations instrumented in the compiled native callback path; worst callback time, queue starvation, CPU, memory and latency measured on named hardware with declared workloads [SYNTH-ADVANCED-VERIFY-REALTIME].
- [ ] Slow or detached Signal subscribers never stall rendering; observations keep coherent revisions and sample positions [SYNTH-ADVANCED-VERIFY-REALTIME].
- [ ] Live graph and resource swaps, state transfer, crossfade policy, tail disposal and failure rollback verified [SYNTH-ADVANCED-VERIFY-REALTIME].
- [ ] Reference and offline availability separated from verified realtime capability in public descriptors and documentation [SYNTH-ADVANCED-VERIFY-REALTIME].
- [ ] Headless and windowed journeys that add, connect, edit, play and remove every advanced module; screenshots against Prism's visual style; real mouse dragging, cable deletion, keyboard docking, window resizing and accessible controls; window screenshots and journeys verified on an unlocked desktop [SYNTH-ADVANCED-VERIFY-PLATFORMS] [PRISM-ROUTING-DELIVERY].
- [ ] Native and canvas interfaces exercised with loaded headless and windowed journeys, screenshots, actual audio output and level-matched listening examples for each family (patch, resource and seed metadata); cross-platform availability with explicit platform limits [PRISM-ROUTING-DELIVERY] [SYNTH-ADVANCED-VERIFY-PLATFORMS].
- [ ] macOS and Windows integrations verified with device and backend differences documented; browser and mobile builds through their supported hosts, reporting unsupported modules, resource limits and latency instead of substituting another algorithm [SYNTH-ADVANCED-VERIFY-PLATFORMS].
- [ ] Each module's ports, units, scaling, defaults, state and reset semantics, latency, capacity and supported rates and channels documented (the module reference) [SYNTH-ADVANCED-DELIVERY].
- [ ] Code, specifications, patches and test evidence linked through the section IDs here, and this checklist reconciled with [Synthesis.md](../../specs/synth/Synth.md) [SYNTH-ADVANCED-DELIVERY].
- [ ] Experimental or production status marked from measured evidence; remaining limitations recorded; task-owned temporary processes and artifacts removed [SYNTH-ADVANCED-DELIVERY] [PRISM-ROUTING-DELIVERY] [PRISM-ROUTING-WRAP-UP].
- [ ] The shipped songs play every family (granular and spectral morph are missing today); a test opens each song and checks every family is present [SYNTH-SHOWCASE].
- [ ] `@osprey/synth` coverage to 100% (63.4%, 2,650 of 4,180 lines) through loaded journey tests per family [SYNTH-ADVANCED-VERIFY] [PRISM-ROUTING-VERIFY-COVERAGE].
- [ ] Static analysis, package coverage, CI preparation, root CI and the duplication gate green, with one validated push under the TMC lock and its CI inspected [PRISM-ROUTING-DELIVERY] [SYNTH-ADVANCED-DELIVERY].

## Device performance, in parallel with everything [DEVICE-PLAN-SPEED]

The device runs the same engine as Prism, on a Cortex-A76 core that is slower than an Apple core ([DAW-TRANSPORT-LIVE-SPEED] in the [daw plan](daw.md)). Nothing waits for this track and it waits for nothing.

- [ ] The daw's live-speed items done: 1.5× real time or better on the codex song on the Mac [DAW-TRANSPORT-LIVE-SPEED].
- [ ] The render callback passes the real-time checks: no allocation, free, blocking lock or I/O, statically and under RealtimeSanitizer [TARGET-EMBEDDED-REALTIME].
- [ ] Per-processor costs measured on a Raspberry Pi 5 for the budget table as soon as the prototype boots, and re-measured on every engine change [DEVICE-ENGINE-BUDGET].
- [ ] The engine profiled on the Pi 5 itself, not only on the Mac; the hottest costs fixed first.
- [ ] The acceptance patch fits within 70% of a 64-frame block at 48 kHz on one core [DEVICE-HW-COMPUTE].

## Device profiles and the virtual device [DEVICE-ENGINE-VIRTUAL]

- [ ] **Profile model ([DEVICE-PROFILE]).** Move [device.td](../../designs/prism-device/device.td) into its package, generate Osprey types, and write the reference profile as data.
- [ ] **Engine program ([DEVICE-ENGINE]).** An Osprey program linking only `@osprey/synth`: load a `PatchDocument`, check roles, nodes and budget, render; `make -C src/toolchain/targets/<platform> prism-device` with tests in each platform's tests Makefile and a headless suite in `src/toolchain/targets/tests`.
- [ ] **Controls ([DEVICE-CONTROLS]).** Knob filter, hysteresis, pickup and jump, switches, footswitch toggle, hold and long press, written in Osprey; proofs per [DEVICE-PROOFS].
- [ ] **Link ([DEVICE-LINK]).** tdbin frames over TCP with the 1 MiB bound: Hello, live control, deploy, refusals, get patch; the virtual device listens on localhost only.
- [ ] **Devices panel ([DEVICE-LINK-SHOW]).** Prism lists devices, opens the playing patch or fetches an unknown one, and deploys with the profile check shown first.
- [ ] **Concept test on the Mac ([DEVICE-VERIFY-CONCEPT]).** `make -C src/toolchain/targets/macos prism-device` starts the virtual device; from Prism, deploy a source patch and an effect patch, play one from a MIDI keyboard and run audio through the other, turn every on-screen control, switch slots, see the playing patch in the Devices panel. This gates ordering parts.
- [ ] **Virtual device journeys ([DEVICE-VERIFY]).** In CI: many deploys across slots, every refusal kind, malformed and oversized frames, every control moved with macro and audio assertions, click-free swaps, injected faults.

## Firmware [DEVICE-OS]

Built on Elk Audio OS, Zynthian and PiPedal's defaults ([DEVICE-RESEARCH-PRIOR], [DEVICE-RESEARCH-OS]).

- [ ] Raspberry Pi OS Lite (64-bit) image built with `rpi-image-gen` from a pinned package list: the standard kernel, ALSA, USB gadget modules, the engine as the only service; built by `make`, shipped as a CI artifact, never committed.
- [ ] A Raspberry Pi OS (Debian arm64) root as the sysroot for `--target=linux-arm64 --sysroot` [TARGET-EMBEDDED-LINUX-CROSS].
- [ ] Read-only root with the OS's overlay file system, an atomic data partition, A/B images through `tryboot` with the 10 s health check and fallback; images signed with the release key and the updater refusing unsigned images.
- [ ] The real-time kernel kept as one line in the package list, built only if the 24-hour journey records dropouts [DEVICE-OS-KERNEL].
- [ ] Isolated audio core, `SCHED_FIFO`, locked memory, direct ALSA at 64 frames; 32 and 128 offered, nothing that can exceed 10 ms [DEVICE-HW-LATENCY-LIMITS].
- [ ] One dual-role USB-C port ([DEVICE-HW-USB]): device role to a computer (USB MIDI, network link, firmware updates), host role to a USB MIDI keyboard, switched by the Type-C controller; TRS MIDI in and out on the UART; one merged, timestamped event stream.
- [ ] Watchdog restart with the bypass relay held.
- [ ] Engine test without hardware: the same arm64 executable in a stock Debian arm64 container (Docker on a Mac, GitHub's ARM Linux runner) answers Hello on the link and takes a deployed patch (no audio timing); flashing and boot tested on the prototype with Raspberry Pi Imager.

## Factory [DEVICE-FACTORY]

- [ ] Product bundle built by `make` from a project file and a profile.
- [ ] Provisioning: flash the eMMC over USB-C, write the serial number, run the device test (loopback audio, driven knob wipers, MIDI loopback, bypass with power off; a report per serial number); pass or fail with no partial image.

## Hardware [DEVICE-HW-PLAN]

The [hardware specification](../../specs/synth/Perception.md#perception-device-hardware-device-hw) defines the board, latency budget, compute and the AUD 500 budget; the enclosure is drawn in the [device designs](../../designs/prism-device/README.md#enclosure-device-hw-enclosure). ARM runner minutes and any hardware runner are new spend: hardware journeys stay local until a runner is approved [DEVICE-PLAN-OPEN].

### Prototype [DEVICE-HW-PROTOTYPE]

No part is ordered until the concept test passes on the Mac with the virtual device ([DEVICE-VERIFY-CONCEPT]).

- [ ] Order a Raspberry Pi 5 4 GB, HiFiBerry DAC2 ADC Pro, MCP3208, pots, toggles, footswitches, H11L1 and TRS jacks; record actual prices against the [budget](../../specs/synth/Perception.md#budget-device-hw-budget).
- [ ] Bring up the firmware on the prototype: audio at 64 frames, knobs, switches, TRS MIDI, and the USB-C port in both roles.
- [ ] Measure audio round trip and note to sound with loopback cables: mean, worst case and jitter over 24 hours at full budget; pass: round trip under 5 ms, note to sound under 5.5 ms worst case, never over 10 ms, jitter under 1 ms [DEVICE-HW-LATENCY].
- [ ] Measure the acceptance stereo patch's share of the deadline on one core and the temperature; pass: 70% or less with no thermal throttling [DEVICE-HW-COMPUTE].
- [ ] Power pulled during deploy and during update; bypass with the power off.
- [ ] 3D-printed starter case ([DEVICE-HW-ENCLOSURE-PRINTED]) that fits the prototype and holds the controls and jacks in the panel layout; model files in the repository, no rugged tests.

### Prism board [DEVICE-HW-BOARD]

- [ ] USB role switching tested on Raspberry Pi's CM5 IO board with USB MIDI and the link running; pass leaves the second USB-C footprint unfitted, fail fits it [DEVICE-HW-USB-RISK].
- [ ] KiCad schematic from the CM5 IO board design plus the audio, control, MIDI, relay and power circuits proven on the prototype; layout on four layers, the analog ground plane joined once under the PCM3060, the buck converter and USB away from the codec and clocks.
- [ ] Fabrication outputs, bill of materials and assembly files generated by `make`, never committed.
- [ ] Five assembled boards ordered; the real unit cost recorded against the budget.
- [ ] Every prototype journey repeated on the board in the enclosure, on both barrel and USB-C power.

### Enclosure [DEVICE-HW-ENCLOSURE]

- [ ] The die-cast road case, designed last with the Prism board; its release checks in the [enclosure designs](../../designs/prism-device/README.md#enclosure-release-checks) pass.
