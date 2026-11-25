# Synthesis rendering and audio devices

Part of the [synthesis toolkit](Synthesis.md).
## Rendering and real-time contract [SYNTH-RENDER]

Preparation takes sample rate, channel layout, maximum block size, voice/component counts and event capacity. Rendering uses caller-owned/preallocated buffers and explicit component state. The callback performs no allocation/free, GC, blocking locks, file/network I/O, logging, model loading or unbounded work. Osprey compilation to native code by itself does not prove this property; inspect and instrument the generated callback path. If current runtime facilities cannot satisfy it, report the gap and keep that path outside the callback.

Events carry sample offsets and stable ordering within a block. Note ownership, retrigger, sustain, per-note expression, voice stealing and release policy are explicit. Automation supports ramps with declared interpolation, avoiding accidental block-rate steps. Different legal block partitions of one event timeline must yield equivalent sound and final state within a documented numerical tolerance.

Initial production rate targets are 44.1, 48 and 96 kHz, with irregular block sizes including 1, 31, 64, 127, 256 and 1024 samples. A platform may negotiate others only through checked preparation. Rate changes, device loss, transport jumps and reset have defined state transitions. Peak CPU, callback deadline misses, latency, memory and output quality are measured separately on named hardware. No hard-real-time guarantee is implied by a desktop operating system.

Invalid configuration is rejected before activation. Runtime failures return a typed fault and bounded diagnostics. A host may explicitly choose to silence a failed stream, but it must also surface the failure; silence is not a fabricated successful render. Non-finite input, parameter overflow and numerical instability have specified handling and tests.

## Stateful multichannel blocks [SYNTH-RENDER-BLOCKS]

`ClipKernels.block` accepts and returns independent per-channel kernel state alongside interleaved samples at the hosted path's 48 kHz rate. Unequal block partitions retain oscillator/LFO phase, filter integrators and delay history. The component must exactly match its prepared graph entry, and state identities, channel counts and block capacities are checked before processing. The existing whole-clip entry point initializes state and delegates to this same implementation.

`GraphClips.start`, `next` and `replied` schedule those states across complete acyclic graph blocks. A typed pending request owns the frozen graph, current block buffers and kernel states; only its named external node can supply the checked response. The graph clock advances once the entire block completes. MIDI note ownership and the 128-frame automation grid use that absolute clock, including unequal block sizes and positions beyond the bounded whole-clip window. A continuing graph that loses or duplicates required kernel state is rejected.

This is producer-side, allocating reference execution. It does not establish allocation-free device callbacks, persistent VST sessions in Prism, graph-wide streaming scheduling or uninterrupted device playback; those integration steps remain required.

## Reference continuous device execution [SYNTH-DEVICE-QUEUE]

Prism owns one bounded audio-device session. Completed graph blocks are encoded outside the callback and transferred as canonical 48 kHz mono/stereo float32 WAV. macOS queues them through AudioQueue and hosts VST sessions alongside the device. Windows converts the float samples to 16-bit PCM for WinMM and reclaims buffers as the device returns them. Both adapters bound the queue and accept 1–8192 frames with finite samples within -1..1. Invalid layouts, lengths, amplitudes, queue timeouts and mismatched acknowledgments are explicit failures.

Prism transfers 2048 graph frames at a time, while VST processing packets remain at most 256 frames. A typed pending device record retains the exact graph, sample clock and component states until the device acknowledges the transfer. Audio slices are reused across blocks. Audition drains every accepted frame; continuous Play schedules further blocks until Stop. Stop during opening first adopts the returned handle and then releases it, preventing a late-open resource leak. Host shutdown also releases owned sessions.

The reference producer allocates and runs outside the device callback. A native loaded-binary journey streamed 985088 frames through JX10, two ADelay instances, LFO modulation and the real device with zero queue-empty observations, then verified worker disposal. Queue-empty counters record the device's observations. Prism applies graph edits at restart. Transactional live replacement and host latency compensation remain outstanding.

Queue-empty observations must be monotonic within a device session. The controller rejects decreasing counters and retains a visible warning for a nonzero count through the audition completion. The backend emits a structured `audio.queue_empty` diagnostic when that count increases, outside the audio callback. A deliberate producer pause exercises this path on the real device.

Latest verification (2026-11-16): the final sustained native reruns failed the unchanged zero-empty-queue assertion, including a rerun without concurrent test compilation. Earlier successful 985088-frame evidence is not reproducible reliability. Exact finite VST-chain PCM still passes. Continuous playback is experimental pending producer-throughput work; do not treat this branch as release-ready.
