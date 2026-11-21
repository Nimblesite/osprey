# Prism VST3 worker [SYNTH-HOST-VST]

This process loads third-party VST3 modules separately from the Osprey application. It scans component IDs and descriptors, probes lifecycle and saved state, and processes checked audio/MIDI/automation blocks through persistent instances at 48 kHz. Failed loads, absent class IDs, unsupported layouts and non-finite audio fail explicitly. Prism uses it for bounded graph auditions; continuous device output and sandboxing remain unfinished.

Run `make -C src/toolchain/targets/tests synth-vst` from the repository. It fetches the pinned official SDK into `target/vst3sdk`, builds the worker and ADelay/mda fixtures into `target/vst-worker`, runs the SDK validator when building those fixtures, then runs nine process-level checks. It does not install plug-ins into the user's library and needs no audio device or window. Fetch/build/test commands have bounded timeouts.

The SDK is pinned to `3cdf9ca5d1f5b1b21e0a86832aa4abe55607bd96`; its exact submodule commits are supplied by that revision. The SDK and examples retain their upstream notices and [MIT license](https://github.com/steinbergmedia/vst3sdk/blob/3cdf9ca5d1f5b1b21e0a86832aa4abe55607bd96/LICENSE.txt). No SDK sources are copied into the repository.

`scan <module.vst3>` emits one JSON class record per audio component. `probe <module.vst3> <class-id>` is a test operation that requires measurable audio and emits peak/latency evidence. It intentionally rejects a silent plug-in result and is not a general playback API. The streaming protocols accept valid silence and distinguish it from a failed processor.

Verified on macOS arm64 with ADelay and mda JX10. Windows and Linux loader paths are configured but not yet verified. The state probe changes a real processor parameter, verifies its saved bytes change, restores the original component state, flushes deferred state at a processing boundary and checks the controller value agrees. Automation is submitted at offset 17; this test does not establish that every plug-in honours that offset sample-accurately. Live restart notifications, continuous Prism device playback, persistent app-owned worker lifetimes, crash/hang recovery, sandbox capabilities and callback-allocation evidence remain pending. A separate process alone does not restrict a native plug-in's filesystem or network access.

## Persistent blocks [SYNTH-HOST-STREAM]

`osprey-vst-worker stream <plugin.vst3> <class-id>` keeps one plug-in instance alive until stdin closes. The initial 16-byte response is `OSV1` followed by little-endian unsigned 32-bit input channels, output channels and a MIDI-input flag. This first transport supports one audio output bus, at most one audio input bus and at most one event input bus; unsupported layouts fail explicitly. Processing is float32, 48 kHz, offline mode, with 1–256 frames per block. This is worker transport, not a realtime device engine; Prism currently uses the bounded text variant.

Requests contain three little-endian unsigned 32-bit counts: frames, notes and parameter points. At most 128 notes and 1,024 parameter points follow. Each note contains five unsigned 32-bit fields (kind: 1 on/2 off, sample offset, MIDI pitch, MIDI channel, note identity) and one float32 velocity. Each parameter point contains an unsigned 32-bit parameter ID and offset followed by a float64 normalized value. Notes and parameter points must each be ordered by offset. Audio follows, interleaved float32 with the advertised input-channel count. The canonical application-side types are generated from `src/osprey/packages/synth/host.td`.

Each response contains unsigned 32-bit frame count and plug-in latency followed by interleaved float32 output. A response is flushed before reading another request. The worker retains DSP state and advances sample position across blocks. Values, counts, layouts, event fields and parameter identities are checked before processing; malformed/truncated input produces `vst.failure` on stderr and a nonzero exit. EOF between complete requests ends cleanly. Silence is valid output, unlike the separate lifecycle probe. Hosts must bound response reads and kill unresponsive/crashed workers; this transport does not sandbox third-party native code.

The compact fixed-format transport avoids a new runtime dependency in the native worker. tdbin was considered: the available typeDiagram tree has Rust/TypeScript codecs but no C++ codec. A generated tdbin bridge remains a future option; this version is explicitly identified by its magic and is not presented as tdbin.

The stream journey uses actual ADelay and mda JX10 binaries: exact delayed PCM crosses a block boundary, MIDI starts at sample 17, a held note remains audible, a matching note-off with an explicitly short release becomes silent, dense events are accepted up to the limit, and malformed counts/values/order/IDs are rejected. ADelay's example bypass has an upstream TODO, so the PCM oracle uses its documented minimum one-sample delay rather than assuming bypass works.

`describe <plugin.vst3> <class-id>` emits one checked JSON descriptor with class name, channel counts, MIDI input support and up to 1,024 normalized parameters. Metadata is read after processor/controller synchronization; invalid values or unsupported layouts fail before any descriptor is emitted. `Hosted.component` in `@osprey/synth` converts the typed descriptor into a checked graph node with actual channel types and independent parameter identities.

`stream-text` accepts the same request bytes as strict hexadecimal on stdin and emits the response bytes as hexadecimal on stdout. It exists for native capability transports whose slices carry UTF-8; it uses the identical processor and packet checks, not a second renderer. Input is capped at 1 MiB decoded and output audio at 65,536 channel samples. The entire response is emitted only after the bounded clip succeeds. The persistent binary `stream` mode remains available for a device-engine adapter.


`stream-session` keeps the same plug-in alive across newline-delimited hexadecimal transactions. Each transaction has the same input and output limits as `stream-text`, and each complete response is one hexadecimal line flushed before the next request is read. Limits apply per transaction; the plug-in sample clock and DSP state continue across transactions. EOF between lines closes cleanly; empty, odd-length, malformed, oversized or unterminated lines fail explicitly without a response for that transaction. A caller must impose a deadline for every response and close or terminate the worker when finished.

The session test reads each reply before closing stdin, checks exact ADelay state across transactions, and compares 162 JX10 transactions against one persistent binary stream, including held notes and release to silence. The cumulative sample count exceeds the whole-clip limit while every transaction remains bounded. This proves worker continuity, not persistent Prism ownership or realtime device scheduling.


### Device queue [SYNTH-DEVICE-QUEUE]

On macOS, `osprey-vst-worker audio-device` accepts newline-framed hex blocks containing the synth package's canonical mono/stereo 48 kHz IEEE float32 WAV. Blocks contain 1–8192 frames and samples within -1..1. It preallocates four AudioQueue buffers, prefills two, and acknowledges each accepted block with bounded queue counters. `!drain` waits for every queued frame to be returned by the device before acknowledging; closing/killing the session cancels ownership. JSON diagnostics report device start, drain and failures. Other platforms explicitly report the adapter as unavailable.

`python3 src/toolchain/hosts/synth_vst/build.py --device-test` verifies real queue operation, drain and malformed PCM rejection; it is included in the macOS worker CTest suite. A returned buffer proves device-queue consumption, not an independent acoustic recording. Prism now uses this queue for bounded auditions and continuous Play/Stop. Its loaded native journey captures actual queue receipts and verifies that all instrument, effect and device workers are gone after Stop.

## Third-party spectrum test on macOS

With the Prism GPUI host built for the DAW and the VST worker built, run the following from the repository root. Point `VST3_FOLDER` at a folder containing the extracted VST3 bundle. The test runs Prism with a temporary preferences folder and the real audio device. It keeps the plug-in output, the PCM submitted to the device and the device receipts under `target/pr-validation/vst-spectrum-*`.

```sh
make -C src/toolchain/targets/macos/tests synth-vst-spectrum VST3_FOLDER="/absolute/path/to/plug-ins"
make -C src/toolchain/targets/macos/tests synth-vst-spectrum VST3_FOLDER="/Library/Audio/Plug-Ins/VST3" VST3_CASE=tal
make -C src/toolchain/targets/macos/tests synth-vst-spectrum VST3_FOLDER="/folder/with/both/plug-ins" VST3_CASE=patch
```

The first command uses Dragonfly Room Reverb 3.2.10. Its macOS universal disk image has SHA-256 `e0b854b92a4e51ce5851320fb1a9f91ab1a4309f997de070e175b3e5cab4adaf`. The test compares dry, muted and wet auditions and exercises continuous Play and Stop. The second command uses TAL-NoiseMaker and measures the change in fundamental frequency after raising the MIDI note by one octave. Both compare the stereo samples leaving the VST3 instance with the samples submitted to the device. [Dragonfly release](https://github.com/michaelwillis/dragonfly-reverb/releases/tag/3.2.10)

The `patch` case connects TAL-NoiseMaker to Dragonfly Room Reverb and a stereo output. It checks both cables after deleting and undoing the reverb, renders the patch into `Prism-TAL-Dragonfly.wav` in the reported run directory, and verifies the pitch and reverb tail in that file.

## VST3 project and worker state

Prism stores each node’s class ID, graph-facing ports, normalized controls and modulation routes in the song. It resolves bundle paths from the configured library on this machine. A persistent worker accepts a hexadecimal `SAVE` line and returns an opaque, class-bound `OPS1` state frame. Sending that frame to a fresh session restores the processor and optional private controller state and returns hexadecimal `OK`. The session tests exercise two SDK classes, changed parameters, multiple restores and class mismatch rejection.

The spectrum target accepts `VST3_CASE=latency` with CHOW Tape Model 2.11.4. At 48 kHz the plug-in reports 39 frames of latency. The test checks each stereo output sample against parallel dry and processed paths. Set `VST3_UI=window` to run the spectrum journeys in a native window.

### Native editor windows on macOS

Choose a track’s patch, select a VST3 node in Patch Controls, and choose **Open plug-in window**. The track patch stays in its own window alongside the arrangement. Each vendor editor runs in a separate worker for its node. Prism and the vendor controller exchange normalized control values in both directions. Processor-private state and live meters belong to the processing instance.

Run `make -C src/toolchain/targets/macos/tests synth-vst-editors VST3_FOLDER=/path/to/plugins` after building the worker and Prism GPUI test host. The journey edits every automatable parameter in the inspector, compares the values sent to the actual vendor controller, checks its endpoint readbacks, and exercises closing and reopening both windows. For a known unresponsive editor, `node src/toolchain/hosts/synth_vst/editors.mjs /path/to/plugins --unresponsive-editor DragonflyEarlyReflections.vst3` verifies recovery and continued editing through the inspector.
