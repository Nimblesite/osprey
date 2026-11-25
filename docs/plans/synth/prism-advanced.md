# Prism: advanced synthesis delivery

Part of the [Prism routing plan](prism-routing.md).
## Advanced synthesis delivery plan [PRISM-ADVANCED-PLAN]

The 2026-11-17 handoff supplies the proposed delivery sequence below. The [active implementation checklist](advanced-synthesis.md) owns processor milestones and their status. The [package and UI specification](../../specs/synth/SynthAdvanced.md#advanced-synthesis-proposal-synth-advanced) defines acceptance. The [research guide](../../research/synthesis/README.md#neural-granular-and-physical-synthesis-synth-research-focused) compares the papers and instruments behind these choices.

### Repository baseline and dependencies [PRISM-ADVANCED-BASELINE]

The proposal started from the package's `graph.td`, `render.td`, `src/clip_kernels.ospml`, README and Prism's `editor.td`. Reuse graph identities, typed cables, parameter revisions and retained block processing. The 17 November baseline used basic/external processor variants, oscillator/filter/delay kernel state and a fixed 48000 Hz clip path. The [family checkpoint](advanced-synthesis.md#advanced-family-checkpoint-synth-advanced-families-checkpoint) records subsequent changes. Keep the 16 November sustained-device failure open until fresh device evidence meets the contract.

| Layer | Integration and evidence |
| --- | --- |
| Schemas | Extend typeDiagram sources and regenerate. Round-trip old graphs and exercise missing and unknown fields. |
| DSP mathematics | Implement native Osprey formulas and prove properties of their parsed bodies for windows, interpolation, modal transitions, waveguide tuning, energy accounting and controller mappings. Check reference parity. |
| Package processing | Carry grain scheduling, body state and neural request descriptors through public preparation/block interfaces, validation and observations. |
| Host adapters | Bound asset decoding, model execution and generated-asset jobs. Reuse worker ownership/cancellation with a dedicated typed model protocol. |
| Prism | Extend existing editor state, palette, cards, inspector and keyboard gestures within the single accessible routing workspace. |
| Evidence | Extend loaded package/app journeys and platform screenshots. Keep artifacts below `target/`. |

### Shared foundation [PRISM-ADVANCED-FOUNDATION]

Follow the [shared component checklist](advanced-synthesis.md#shared-component-foundation-synth-advanced-core). Exercise invalid state, rate changes, asset generations, irregular partitions, unavailable models, cancellation and migration before extending behavior. Validate dimensions, products, limits, finite data and model operators before preparation. Thread format, processor-specific state and asset lifetimes through scalar and graph execution. Measure producer queues, fault publication, latency, cancellation ownership and device starvation. Preserve seeds, applied parameter semantics and hashes across save/reload and relinking.

Exit evidence covers old-processor results, rejected state, irregular-block parity and transactional replacement under GC and ARC on supported hosts. Exercise wasm32 package execution and explicit backend unavailability. Use the supported memory backend on native Windows.

### Granular package and interface [PRISM-ADVANCED-GRANULAR]

The [granular checklist](synthesis-methods.md#granular-synthesis-synth-advanced-grains) records the current kernel, scheduler, asset and graph work. Use seeded event traces and window/resampling oracles for reverse and single-frame sources, replacement, capture wrap, zero/max density and exhaustion. Verify the shared sample/live kernel, independent pitch and scan, region boundaries, source retention and visible overload counts.

Drive sample import, notes, pitch changes at fixed scan, freeze, replacement during tails, capacity exhaustion, undo and save/reload through the Granular card and inspector. Exercise Size, Density, Pitch, Scan, Capture, Freeze, waveform/region gestures and numeric equivalents. Compare audio, applied state and screenshots; measure pitch and alias energy against the reference resampler. Demonstrate a frozen pad and time-stretched transient with reproducible event traces. GR-1 and Morphagene inform navigation, Quanta and Pigments inform sample voices, and Portal informs live capture.

### Physical package and interface [PRISM-ADVANCED-PHYSICAL]

Use the [physical processor checklist](synthesis-methods.md#physical-and-modal-synthesis-synth-advanced-physical) for implementation status. Pin equations, boundary conditions, modal weights and ranges for bars, plates, membranes, strings and tubes. Establish impulse, tuning, decay and energy oracles. Progress from modal bodies and strike/pluck through fractional-delay strings/tubes to bounded bow/blow feedback. Exercise convergence failure, joint coupling, automation and material changes before live use.

Drive Exciter, Modal Body, String and Tube through palette selection, conditional controls, pickup/contact gestures and mode/energy observations. Save and reload struck metal, damped membrane, plucked/bowed string and blown tube patches after varying velocity, pickup, material and coupling. Collect isolated-mode tuning/T60, decay/energy bounds, solver-exhaustion evidence and headless/windowed screenshots. Rings/Elements inform exciter/body controls, Chromaphone informs coupling, and Collision/Tension inform gestures.

### Neural package and interface [PRISM-ADVANCED-NEURAL]

1. Select redistributable RAVE, DDSP, codec and text-to-audio fixtures independently. Record exact hashes, operator/runtime compatibility, licenses and available training-data provenance.
2. Benchmark load, warm-up, execution distribution, memory, hop/lookahead and cancellation on Windows and other target hosts. Select a backend only after the chosen models execute with bounded operator support. ONNX export and TorchScript are candidates.
3. Exercise manifest rejection, worker loss, late replies, shape errors and codebook bounds before wiring real inference through typed effects. Retain history across irregular blocks.
4. Verify RAVE transfer and DDSP Audio/MIDI with latency-compensated mixing and pitch confidence. Run real codec encode/decode fixtures.
5. Generate validated assets with cancellation. Preserve prompt, duration, seed and model revision. Exercise Preview and Insert independently.
6. Drive model selection, capability/error display, latent or harmonic/noise controls, codec inspection and generation. Cover missing assets, replacement, stale results, cancellation and save/reload.

Collect genuine audio, held-out measurements and lifecycle evidence for each capability. Record backend-specific nondeterminism in replay metadata. Offline text generation has an independent delivery gate. Live transfer also requires measured deadlines.

### Combined synthesis experiments [PRISM-ADVANCED-HYBRID]

Use one evolving body and learned grain contacts. Its energy affects the timing, location and texture of later contacts. The [combined-method contract](../../specs/synth/SynthAdvanced.md#combined-grain-and-body-method-synth-advanced-hybrid) defines the state transition and work budget.

| Experiment | Deliverable | Decision evidence |
| --- | --- | --- |
| Body-dependent contacts | Analytic controller, then learned scheduler/texture decoder with energy-constrained forces | Initial body state changes future contact traces only with Interaction enabled. |
| Joint sound analysis | Estimate editable grain dictionary, contact envelope and material from reference audio | Held-out reconstruction and edits compared with independent parameter fitting. |
| Text-directed interactions | Generate contact/material trajectories for the same body | Prompt relevance, valid trajectories and predictable manual edits compared with generated-audio resampling. |

Review the full neural-granular, haptic modal/granular and differentiable-modal papers and broaden the prior-art search around contact-state feedback and energy-constrained learning. Define state units, force projection, material-change work, bounded transitions and failure behavior in native Osprey. Prove the energy bound against the production transition function. Establish analytic controls and ablation fixtures before training with disjoint recording/body splits, fixed seeds and matched budgets.

Evaluate offline, then run the learned controller through the neural lifecycle. Capture event traces, constrained updates, energy, latency and audio. Exercise the Combined module's Texture, Material, Body memory, Interaction and Energy controls, contact map, timeline, energy inspector, numeric/keyboard alternatives and controller selection. Compare ordinary routed grains/resonator, open-loop contacts, analytic feedback, learned control without energy features and the full method under matched loudness and latency. Retain negative results. Production work needs repeatable musical value within energy/resource limits. Joint analysis and text trajectories require independent evidence.

### Verification and delivery sequence [PRISM-ADVANCED-DELIVERY]

Shared preparation/assets/state precede granular and physical processing. Deliver the matching UI and journey at each stage. Neural work depends on backend selection and worker lifetime handling. The combined renderer depends on all three foundations, its native energy model, checked energy bound and analytic baseline. Measure that baseline before training the controller.

Use `make -C src/osprey/packages synth` for package verification and `make -C src/toolchain/targets/tests synth` for the host-free Prism suite when verification resumes. Windows has a synth launch target in its [platform Makefile](../../../src/toolchain/targets/windows/Makefile). Verify platform-specific launch, windowed and headless journeys through the appropriate scoped targets.

Delivery requires the [acceptance matrix](../../specs/synth/SynthAdvanced.md#acceptance-contract-synth-advanced-verify) with real assets/models and public entry points. Verify Windows sandbox capabilities, keyboard accessibility, resize behavior and screenshots on every advertised host. Require a ten-minute maximum-workload run with zero starvation and callback allocation/lock instrumentation before claiming continuous playback. Record delivered capabilities and constraints in package and Prism documentation. Keep unverified milestones open.

Resolve resampling quality thresholds, membrane/tube reference models, runtime and redistributable model selection, platform memory/performance limits and the hybrid training dataset with their named experiments before optimizing dependent paths. Numerical defaults remain testable starting points.
