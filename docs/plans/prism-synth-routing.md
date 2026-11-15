# Prism modular synthesis and VST hosting [PRISM-ROUTING]

The synth package remains UI-free. Prism becomes an editable instrument graph: add oscillators, effects, MIDI generators, modulators and VST3 host nodes from a menu, connect compatible ports, adjust parameters and hear the resulting graph. Existing audio and control work must remain usable while a replacement graph is validated. See [SYNTH-GRAPH], [SYNTH-SIGNALS], [SYNTH-RENDER] and [SYNTH-HOST-VST] in the [synthesis specification](../specs/runtime/Synthesis.md).

Work is isolated on `codex/prism-synth-routing`, based on `math-to-code` at `e3ebc1399`. The requested stopping time is 03:00 Australia/Melbourne on 16 November 2026; stop with an accurate handoff even if verification remains incomplete.

## Delivery checklist [PRISM-ROUTING-DELIVERY]

- [x] Create and attach the requested worktree without moving unrelated Inbox edits.
- [x] Establish the synth package baseline: thirteen existing loaded journeys pass.
- [ ] Implement transactional graph editing, cable removal, component removal and checked reconnection; rejected edits preserve the previous playable graph.
- [ ] Expand typed component descriptors and rendering for MIDI generators, modulators and effects; declare new models in typeDiagram and derive their representations.
- [ ] Replace Prism's fixed patch mapping with the synth graph, with a component menu, selectable nodes, compatible cable endpoints and parameter inspection.
- [ ] Publish applied graph/parameter revisions and component activity through Signals without a UI dependency in the package.
- [ ] Implement actual VST3 discovery, out-of-process scanning, activation, processing, MIDI/automation, state save/restore and disposal as an external graph node.
- [ ] Verify an SDK instrument and effect with real audio, MIDI, parameter and state round trips; surface unavailable or failed plug-ins explicitly.
- [ ] Exercise native and canvas interfaces with loaded headless and windowed journeys, screenshots and audio evidence; retain cross-platform availability with explicit platform limits.
- [ ] Run static analysis, required CI preparation and coverage checks; coordinate a single validated push and inspect its CI result.
- [ ] Record remaining limitations and remove only this task's temporary processes/artifacts.

## Acceptance evidence [PRISM-ROUTING-VERIFY]

A graph-edit test must add multiple instances, wire chains and fan-out, reject incompatible ports and illegal cycles, remove/reconnect routes, delete a connected node and verify unchanged snapshots on rejection. Audio tests must distinguish the sounds before and after a routing change, retain oscillator/filter state under parameter-only edits, and exercise block partitions and note-off ownership. UI tests must perform the edits through visible controls, not only construct the final graph directly.

Hosting means loading an existing VST3 binary, not producing a plug-in or rendering a placeholder oscillator. Plug-in discovery and loading never run on the audio callback. Scan failures, unsupported buses and absent binaries remain visible typed errors. Third-party native code is untrusted: process isolation and bounded protocol validation are part of the host design, and arbitrary plug-in safety is not implied by Osprey's guarantees.
