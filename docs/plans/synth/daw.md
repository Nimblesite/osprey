# The daw: delivery plan [DAW-PLAN]

The [daw specification](../../specs/synth/Perception.md#perception-studio-the-daw-daw) defines the requirements. The [Timeline designs](../../designs/prism-timeline/README.md) are the visual reference, within the [scope](../../specs/synth/Perception.md#scope-daw-scope) the specification sets.

## Real-time play [DAW-TRANSPORT-LIVE]: the first blocker

- [ ] **Keep Elise Unbound supplied through the first 16 beats** ([DAW-TRANSPORT-LIVE-SPEED]). Run the cold playback-rate check with `DAW_RATE_SONG=src/osprey/examples/daw/tracks/elise-unbound.json DAW_RATE_BEATS=16`. ARC and GC rendered identical PCM across 740,211 stereo frames. ARC took 21.62 s (0.71×) with a 709 ms gap. GC took 68.91 s (0.22×). An independent ARC run first missed its simulated device deadline after 536,576 frames (11.18 s of audio). In a two-second ARC profile, 909 of 1,538 main-thread samples were under `SpectralKernels::streamed`; the most frequent CPU leaf functions released or retained allocations. Verify zero deadline misses and at least 1.5× throughput on this range.
- [ ] **Keep the device queue supplied during extended Elise playback** ([DAW-TRANSPORT-LIVE-SPEED]). Notes are made ahead of the playhead and joined live; re-check the earlier measurements once the cache is back. Re-measure the first 48 beats and count empty callbacks as well as returned frames.

**The user's verdict (2026-11-21): without real-time playing the app is useless. This is the largest open defect and comes before every other item in this plan.** Play used to render two bars to a WAV ("Rendering…") and then play the file. The requirement is in the specification under [Transport](../../specs/synth/Perception.md#transport-daw-transport).

- [ ] **Engine at 1.5× real time or better on the codex song ([DAW-TRANSPORT-LIVE-SPEED]).** Measured 2026-11-21: the live stream makes 0.37 s of audio per second (17.1 s in 45.8 s, one core, machine under load); the offline two-bar render makes 0.56. A 12 s profile of the live stream is mostly reference counting and allocation (`arc_drop_child`, `arc_release_zero_locked`, `osp_retain`, `osp_release`, `arc_alloc_locked`), then `osprey_list_get`. Owners: OspreyOpus1 (synth kernels, delay lines, effect state), ferroui-prism-lead (stream encoding, block loop, code generation). Until this is met the device runs empty and live sound breaks up.
- [ ] **Eliminate remaining cold Elise dropouts.** At beats 8–24, a device trace showed four late notes, including two Throat morph notes 159 and 319 ms past onset, plus empty callbacks before those notes. The subsequent device run drained all 721,263 frames with two empty callbacks. The dense codex passage rendered identical audio at 1.15× but missed a delivery deadline by 707 ms.
- [ ] **Meet the 1.5× single-core deadline on cold codex.** The range-map compiler build rendered all 4,835,750 frames in 173.47 s (0.58×, longest packet gap 1.24 s); its PCM matched the previous whole-song digest. A current checkout sample put ARC child release, object release, retain and allocation at the top of the synthesis thread.
- [ ] **Eliminate residual cold-device underruns and shorten cold startup** ([DAW-TRANSPORT-LIVE-SPEED]). On 21 November 2026, the installed renderer delivered all 721,263 stereo frames of Elise beats 8–24. CoreAudio reported two empty callbacks near frame 687,520 after a 4.27 s cold start, and zero after an 89.5 ms warm start. Two isolated candidate runs reported zero cold underruns. The cold spectral head start in `a27555d29` is bounded.
- [ ] **Close the whole-song and spectral deadline gaps** ([DAW-TRANSPORT-LIVE-SPEED]). With `DAW_RATE_BEATS=161`, the cold codex score produced 100.74 s of audio in 227.64 s (0.44×); the longest inter-block gap was 2.95 s. Elise at beat 13.8919375 produced 156,937 frames in 12.12 s (0.27×). The 17.4× improvement in the earlier bowed-string onset does not cover this FFT-heavy passage. The latest measured block-rendering run finished all 4,835,750 codex frames in 233.59 s (0.43×), with a 1.11 s longest gap. A separate two-beat run with four note makers and a simulated 48 kHz output queue exhausted the buffer on both codex scores, Elise Motif and Silt Relay. Elise Unbound, Phosphor Drift and Violet Static kept the simulated queue supplied.
- [ ] **Start latency of 5 to 10 ms ([DAW-TRANSPORT-LIVE-LATENCY]).** The device starts after 240 to 480 buffered frames. Interactive playback now flushes 160-frame packets and the device starts after two (320 frames, 6.67 ms of audio). With nine makers and an empty cache, the two-beat codex simulation received its first packet after 2.43 s and kept the queue supplied. Starting fresh for the whole song, the first packet took 2.38 s. All 4,835,750 frames arrived, but the simulated queue ran dry eight times for a total of 21.5 ms. Measure wall-clock time from Play to first sound, including project and device startup.
- [ ] **MIDI clock sync ([DAW-TRANSPORT-MIDI-CLOCK]).** Send timing clock, Start, Continue, Stop and Song Position Pointer from the audio sample clock; follow an external clock when external sync is on. Needs: MIDI port access in the host (CoreMIDI on macOS, Windows MIDI Services), the clock source and ports in `daw.td`, and scheduling against the device's played frames. Nothing is built.
- [ ] **Restore notes made ahead of the playhead ([DAW-TRANSPORT-LIVE-SPEED]).** Playback must buffer ahead. `fa9634be4` deleted the note cache (`render/note_cache.ospml`), its `warm` makers and the app's maker launch; they come back on today's engine (untouched Master, stereo tracks, block drivers). Verify: `the_daw_renderer_exits_one_on_every_failure_and_zero_when_it_writes` in `osprey-cli/tests/project_e2e.rs` passes, including the repaired truncated note and the late-note handoff.
- [ ] The hex text between the renderer and the device costs encoding time on every block; a binary block needs a byte-writing output in the language or the runtime.
- [ ] Project edits must remain playable when a patch branch is disconnected [DAW-PATCH-HEARD]. The edited codex `sub` track with its mixer-to-gain cable removed refused playback before the first block and reported an unsupported graph. Reproduce in a disposable project, verify silence on the disconnected path and continued audio from the other tracks, then reconnect and verify the bass returns.

## Where the work stands at the 15:30 shutdown, 2026-11-21

Everything below is committed on `feat/consolidated`; nothing is pushed. `make -C src/toolchain/targets/macos prism` builds the renderer and app and installs Prism in Applications.

In the code, not yet seen working by the user (rebuild with `make -C src/toolchain/targets/macos prism` first):

- Play reads notes made ahead and kept on disk (`render/note_cache.ospml`); the makers are limited, low priority and end with their Play (`122da5982`).
- The start and end markers drag independently (`1ab47ad22`); a held Gain or Pan fader follows the pointer (`a7ad62cae`); the piano roll's keys sound through the track's patch (`5a9f73628`).
- Resizing the window no longer resizes controls (`209fb9aef`): pages draw at one point per unit. The pages do not yet fill a larger window, and a window smaller than the page cuts it off without scrolling.
- The lanes scroll one row at a time by their scrollbar (`961618056`).
- The song root is title, tempo, sampleRate, buses and tracks; the Connections list is gone from the patch inspector.

Do first next session, in this order:

1. Hear a cold Play of each song with the user. Makers started by the old build may still be running after a restart: `pkill -f "daw .* warm"` ends them. If a cold song still starts rough, make the stream wait for the first second of notes instead of making them itself, and warm after each edit.
2. [DAW-LAYOUT-FILL]: pages lay out to the window (lanes and roll stretch, inspector pinned right), lists scroll row by row (issue 33), the patch canvas pans and zooms (issue 35).
3. Track scope and frequency verdict on the Timeline (issue 25).
4. The daw test suite still cannot be type checked in one piece; the full suite exceeded a five-minute timeout in the latest run. The eight duration cases now use measured frame counts for the current two-second tail. Focused GC and ARC runs pass the arrangement journey, block partition comparisons and project loading for all seven songs. The transport journey, with the new marker and fader checks, was run alone in a scratch project and passes (`bdbe5ef89`). The roll keys are covered too (run the same way, passing). Journey tests are still owed for the lane scrollbar and the note makers.
5. Song files still carry `drive`, `space`, `intro`, `sustains` and track `reverb` keys (ignored on load); their authors remove them. Screenshot goldens need refreshing after the Mix page and scale changes.
6. `make ci` failed on duplication (4.1% against 4.0%) at `bc19a47f5`; `timeline_view.ospml` is the largest offender.

## Issues from the user's hands-on test, 2026-11-21 [DAW-TEST-2026-11-21]

Every issue the user raised while testing Prism by hand, with where it is tracked and where it stands. Nothing here is closed until the user has seen it work in the app.

| # | Issue, as reported | Tracked under | State |
| --- | --- | --- | --- |
| 1 | Play says "Rendering…" and then plays rendered audio. Play must be real time | [DAW-TRANSPORT-LIVE] | Live stream built (633e10d21). Notes are now made ahead by parallel makers and kept on disk: with the notes kept, the 17-track song streams at 1.3–1.5 s of audio per second on a loaded machine (0.37 before). Open: a cold song needs about two minutes of making on six cores, so a Play in that time can still break up; not yet heard by the user |
| 2 | The timeline must follow along with what is playing | [DAW-TRANSPORT-FOLLOW] | Built (633e10d21): playhead, lanes, piano roll page, position readout. Not yet seen by the user with steady audio |
| 3 | Play only plays one short section. It must start at the cursor and play to the end | [DAW-TRANSPORT-RANGE] | Built (633e10d21) |
| 4 | Start and end markers the user drags left and right; the end is never before the start | [DAW-TRANSPORT-RANGE] | Built (633e10d21): the end marker drags by its handle on the ruler; a press elsewhere on the ruler moves the start. Not yet verified in a window |
| 5 | A Loop toggle: loop the start-to-end range for ever | [DAW-TRANSPORT-LOOP] | Loop playback built in `633e10d21`. Subsequent passes reuse notes and run at about 1.6× real time. The native menu exposes L in all three keymaps as of `c6b5a387e` |
| 7 | Real-time play needs only 5 to 10 ms of buffering before it starts | [DAW-TRANSPORT-LIVE-LATENCY] | Open: today 171 ms of blocks plus one buffered block |
| 8 | Playing must sync with the MIDI clock | [DAW-TRANSPORT-MIDI-CLOCK] | Open: nothing built |
| 9 | A patch must render nicely by default, like the design, not as a grid of oversized cards with cables over them | [PRISM-PATCH-LAYOUT] | Open. Owner: prism-routing-design |
| 11 | Dragging the sliders is very clunky, with a huge delay | [DAW-MIX-DRAG] | Open: the value applies only on release, and each commit re-encodes and writes the project |
| 12 | Play failed: "sub: Reference rendering requires mono audio, one output and supported bounded processors." | [DAW-PATCH-HEARD] | Fixed (633e10d21): the Gain module's input in the sub patch had no cable; an unplugged input now hears silence and the song plays. Open: why the Mixer→Gain cable was lost while the user arranged the patch |
| 13 | Editing a built-in instrument silently swaps its sound and drops settings | "Built-in instruments become patches" | Open |
| 14 | The routing page must use the real patch nodes and cables, not its own drawn cards | [PRISM-ROUTING-EDITOR] | Open. Owner: prism-routing-design |
| 15 | Dials against the design: parameters after the tenth are dropped, readings are always hundredths, fine drag has no Shift cue, no arrow keys or focus, no double-press to default | Patch Controls | Open |
| 16 | In a tall window the pages do not fill it: the song pages sit in the top third with empty space below, and nothing scrolls or pans to use the room | [DAW-LAYOUT-FILL] | Open |
| 18 | Stretching the window sideways changes the zoom: everything grows or shrinks with the window's width | [DAW-LAYOUT-FILL] | Open |
| 19 | Open track patches in a separate window and close them while the song keeps playing | [DAW-PAGES-PATCH-WINDOW] | Open. Verify both window close and the focused Close command from Timeline and Bus Routing |
| 21 | Play asks for a song even though a project is already open | [DAW-TRANSPORT-REFUSAL] | Fixed (`757b36f2d`), test first: with a song open and no renderer, Play and Render say Prism was launched without its song renderer; with no song they say to open one; while playing they say to stop first. The project is kept. The transport journey, run alone, was red before and passes after. [Issue 425](https://github.com/Nimblesite/osprey/issues/425) stays open until the user sees it in a release build |
| 23 | A track needs as many internal routes and sends as the project requires | [DAW-GRAPH-SUBGRAPH] | Open. Exercise branching, merging and routing into several buses using the shared node editor. Save and reopen the full graph and timeline from one project JSON file |
| 24 | The example music sounds too simple | [DAW-DEMO-SOUND] | Open. Hand-author the Silt Relay arrangement and patches, exercise the synth module palette, render the master and listen through its section changes. The team is revisiting the other demo songs too Violet Static is finished in `0f67e51a0`. Its 48 kHz stereo master runs 55.657 seconds at −10.9 LUFS with a −0.9 dB true peak. |
| 25 | The Scope is useless inside the patch. The user must see a track's sound from the main view, perhaps not as a node at all but as a panel shown when a track or channel is selected, and it must say whether the sound hits the right frequencies for what it is | [DAW-ANALYSIS-TRACK] | Open |
| 26 | The song root holds properties that do not belong there: `intro`, `sustains`, `drive`, `space`. Drive is a saturator on the master channel, not a root property. Keep `title`, `tempo`, `sampleRate` | [DAW-PROJECT] | Done in the model, engine and UI: the root is title, tempo, sampleRate, buses and tracks; the track's Space send went with the Space. Open: the tests are being updated and cannot be run (suite type check), the song files still carry the old keys (ignored on load, dropped on the next save; their authors are rewriting them), and the screenshots' goldens need refreshing |
| 28 | The start and end markers must move independently | [DAW-TRANSPORT-RANGE] | Fixed in the app code: either marker is taken by its handle or anywhere down its line, in the ruler or the lanes, and drags alone; the start stops one bar before the end and the end one bar after the start. Before, only the end had a handle (a 14-unit band on the ruler) and a press anywhere else moved the start. Not yet seen by the user; journey test owed |
| 29 | Tapping a key on the piano roll's keyboard does nothing. It must play that note through the track's patch | [DAW-ROLL-KEYS] | Fixed in the app code (5a9f73628): a press on a key sounds that pitch through the selected track's patch. Not yet heard by the user; journey test owed |
| 33 | The lanes' scrollbar has no fine control: it is either at the top or at the bottom. It must scroll smoothly, row by row, up and down | [DAW-LAYOUT-FILL] | Fixed in the app code (`961618056`): the lanes scroll by one row, the thumb follows the pointer and its height shows how much is in view. Open: the wheel (the host sends no wheel events yet); not yet seen by the user; journey test owed |
| 34 | **Hard rule:** resizing the window must never resize the controls. Resizing only frees up canvas space or reduces it | [DAW-LAYOUT-FIXED-SCALE] | Fixed in the host and app code (`209fb9aef`): pages draw at exactly one point per unit at every window size. The user's last screenshot (15:02, a small window with a shrunken patch) was a build from before this change: it still showed the Connections list deleted earlier. Must be confirmed in a rebuilt app at small, wide and tall window sizes. Open: a window smaller than the page cuts it off without scrolling |
| 35 | The patch screens need zoom and pan | [DAW-LAYOUT-FILL] | Open. Zoom is the user's own setting, never the window's size |
| 36 | Play takes too long to start, then plays a few bars and stops | [DAW-TRANSPORT-LIVE-SPEED] | Cause found in the running system: every song opened and every Play started twelve note makers that were never ended, so 30 and more of them ran at once (load average 258) and starved the stream. Fixed in the app code: one maker for every three cores at the lowest priority, the makers of a Play end with it, and opening another song or pressing Play ends the song's earlier makers. Open: a song whose notes are not made yet still starts rough; not yet heard by the user |
| 38 | Found by the lead while fixing 28, not yet reported by the user: selecting a track moves the start marker to that track's first bar, because one field (`rollBar`) is both the piano roll's bar and Play's start | [DAW-TRANSPORT-RANGE] | Open. Give Play's start its own field so only the user moves it; 18 test lines read `rollBar` |
| 40 | Found by the lead while getting PR #428 green, not yet reported by the user: with the patch panel docked on Windows, the toolbar band stops short of the panel and leaves a white strip where Undo and Redo belong, and the panel follows the runner's light theme beside the always-dark canvas. In a window narrower than the 1536-point page, the docked panel hides the right of the canvas, because the canvas keeps one scene unit per point ([DAW-LAYOUT-FIXED-SCALE]). | [DAW-INSPECTOR] | Open. The Windows goldens of 2026-11-23 record the strip and the light panel; the macOS self-test passes. Lay the toolbar out across the pane the panel leaves, and decide whether a docked panel narrows or overlays the canvas. |

Found while fixing these:

- [ ] Find how the sub patch lost its Mixer→Gain cable during the user's drag, and add the regression.
- [ ] Toggle Loop and move the end marker while the song plays (today both say "Stop first").
- [ ] A play that hits a track which cannot render names the track and module in plain words.

## Pages fill the window; the canvas pans [DAW-LAYOUT-FILL]

Reported by the user on 2026-11-21 with two screenshots of a tall, narrow window: "The UI is totally fucked. Nothing is scrollable so we can fill the empty space with more canvas anywhere. I can't even drag the modules lower than about the first quarter of the vertical space."

- [ ] **Hard rule ([DAW-LAYOUT-FIXED-SCALE]): resizing the window never resizes the controls.** Stated by the user on 2026-11-21: "resizing the window MUST NOT RESIZE THE CONTROLS. That's a hard rule. Resizing ONLY frees up canvas space or reduces it, that's all." Every control, label, card and cable keeps its size at every window size; a larger window shows more canvas, more lanes and more rows, a smaller one shows fewer and scrolls. The patch canvas has its own zoom and pan, set by the user, and the window's size never changes them.
- [ ] **Lanes scroll row by row.** Done for the scrollbar (`961618056`). Open: the wheel moves one row at a time.
- [ ] **Resizing the window never changes the zoom.** Reported by the user as "a huge bug": stretching the window horizontally scales every card, label and control. Cause: each page is one fixed scene (1440 units wide) scaled to fit the region, so the scale is the window's width divided by 1440. Required: one scene unit is one point at 100% zoom on every page; a larger window shows more canvas, lanes and rows at the same size, a smaller one shows less and scrolls; only the user's zoom commands change the scale. Needs the scene drawn unscaled by the host (or the region's size in points given to the app so it lays out to it), and every page laid out from the region's width and height instead of 1440 × 940.
- [ ] **Song pages use the whole window.** The Timeline and Bus routing pages are drawn on a fixed 1440 × 940 scene scaled to the window's width, so a tall window shows the page in its top third and a dead area below (`Header::shift` centres only sideways; `TimelineGeometry.centred`). The page must lay out to the region's real width and height: more track lanes, a taller piano roll, a taller routing table and signal flow.
  - Inventory for that change (counted 2026-11-21): the Timeline page fixes its width in `timeline_view.ospml` through the lanes' right edge (1092, 1098, 1113: 5 uses), the inspector's left edge and column (1132, 1154, 1416: 22 uses) and its rows through 438, 542 and 802 (7 uses); `header_view.ospml` and `timeline_geometry.ospml` hold the scene's 1440 by 940 (10 uses) and `mix_view.ospml` one 542. Replace these with values measured from the viewport (`viewWidth`, `viewHeight`) in the native Osprey geometry first, so drawing and hit testing change together.
- [ ] **The patch canvas pans and zooms.** Drag on empty canvas, or two-finger scroll, pans; pinch or Cmd-wheel zooms; the canvas is larger than the window and Zoom to Fit shows the whole patch. Needs the wheel and pinch events the host does not send yet (also needed by [DAW-TIMELINE-ZOOM]).
- [ ] **Lists scroll.** The track list, the lanes and the routing table scroll with the wheel when they hold more rows than fit.
- [ ] A journey test at three window shapes (wide, square, tall) asserts that each page's body reaches the bottom of its region and that a module dragged to the bottom edge lands there.

## Slider drag lag [DAW-MIX-DRAG]

- [ ] **Hear each knob move while the song plays [DAW-TRANSPORT-LIVE-CONTROLS].** During a drag, the dial, its displayed value and the playing track must follow the same accepted parameter Signal. Verify the next audio block of a held note, subsequent notes, mixer and bus controls, Undo and a looping song. Measure the time to audible output while dragging on the codex song. The original held-note regression measured a peak of 0.353518 on both channels after gain was set to zero. Still unmeasured: the time from pointer to speaker (acoustic latency); the measured path ends at the audio queue callback (47.9–49.5 ms on the codex tablepad).

Reported by the user on 2026-11-21: dragging the Track Inspector's Gain and Pan sliders is very clunky, with a huge delay.

- [ ] **A commit does not stall the interface.** Each commit encodes the whole project, decodes it again to check it, writes the file, and rebuilds the lane traces and overview. Measure the release-to-repaint time on the codex song; target under 16 ms, with the file write off the interface thread or deferred. On the 128-bar fixture, a move took 0.83 ms on average and release with project serialization and saving took 50.27 ms.
- [ ] The same check for every other slider and dial: Space send, bus faders, the Space decay, patch dials.

## Patch layout [PRISM-PATCH-LAYOUT]

- [ ] **Grain cloud sample path overflows its card** [PRISM-PATCH-LAYOUT-SAMPLE-LABEL]. In the reported screenshot, Grain cloud 0 displays `src/osprey/examples/daw/tracks/samples/elise-motif.wav` across the orange card border. Display the filename inside the waveform panel, truncate long filenames to its available width, and expose the complete path in the inspector or an accessible description. Verify with a narrow window and a long filename, keeping the output socket clear.

Reported by the user on 2026-11-21 with two screenshots: a track's patch opened as a tight grid of oversized cards with cables crossing over the cards, where the design shows cards at their natural size, spaced along the signal path from left to right with cables between them.

- [ ] A patch with no saved positions is laid out along its signal flow: sources on the left, the Output on the right, one column per step, branches stacked, no card over another and no cable through a card. Owner: prism-routing-design.
- [ ] Cards keep their designed size in a patch opened from a track (the generic cards in the report are larger than the designed Oscillator, Master and Scope cards).

## Patch windows and routing interactions [DAW-PAGES-PATCH-WINDOW]

- [ ] **Verify focused Close and ongoing playback in the shipping desktop shell.** The locked Mac returned Core Video error -6661 while starting FerroUI’s display link, blocking a visual inspection of that shell. Verify on an unlocked display during playback that closing it with the window close control or the focused window’s Close command leaves the project open and transport running. Reopen the patch and check its saved parameters, cables and node positions.
- [ ] **Use the patch node editor for Bus Routing** [DAW-GRAPH-SUBGRAPH]. Replace the separately drawn cards, ports, wires and hit areas in `MixView.flow` with the patch editor bound to the project graph. Exercise the reported `glow` route through Stereo pan, pads, Space and Master. Select each node, adjust Space decay and group gain, drag a cable, fan out an outlet, undo, redo and reopen. Verify the displayed values, saved routing and resulting audio agree.

### Regressions from the native review [DAW-PAGES-REGRESSIONS]

The following four host-free journeys failed independently. Keep each open until its existing assertions pass alongside the native interaction tests. [Screenshots and review](../../designs/prism-timeline/native-review/README.md)

- [ ] Verify the drawn sockets and cable hit areas select, connect and disconnect the intended patch ports.
- [ ] Verify that changing module selection shows only that module’s controls and preserves its routing.
- [ ] Restore the device journey that checks pitch, level and filter response and displays playback completion or device failure.
- [ ] Dragonfly Early Reflections' Cocoa editor times out on this Mac during its first OpenGL repaint. The windowed 23-step recovery journey confirms that Prism re-enables the Open command and accepts edits to all seven parameters after the timeout. The vendor graphics still need verification on an unlocked display. One isolated CHOW descriptor worker also crashed in a macOS URL-session callback. Subsequent scans and all 54 control checks passed. Native vendor editors on other platforms and plug-in-private state shared with a running DSP instance remain open.

- [ ] Verify the patch popup, focused-window Close, application Quit, keyboard focus return and screen-reader announcements on macOS and Windows. The screenshot review covered pointer interaction on macOS.
- [ ] Verify spectral, shifter and reverb state after the generic fold fix. The synth owner reported float samples being interpreted as integers when a generic function is passed directly to `foldList`. Check fractional samples under GC and ARC and the rendered audio against a known-good reference. [Compiler regression](../../regressions.md)

## Native Osprey formulas and Lean proofs [DAW-MATH]

Keep the production formulas in native Osprey and derive the definitions checked by Lean from those files. Track notation that obscures the original mathematical expression in the [native formula checklist](../math-to-code.md#native-osprey-formulas-math-code-native-notation).

- [ ] Migrate the dial, canvas and timeline geometry from Lean. Verify the native source through its callers and connect each newly proved property to its production function.

## Project file [DAW-PROJECT]

- [ ] Track and Master channel layouts (stereo default, any count) are stored in the project; tracks render per channel ([SYNTH-GRAPH-CHANNELS]).
- [ ] Built-in instruments become patches ([DAW-PROJECT-PATCH], adversarial review 21 Nov). A built-in track (Pluck, Bow, Bell, Mallet, Operator, Drum) renders through its own DSP, while Prism shows the nearest single node with a fixed 5 ms / 0.2 s gate. The first edit saves that approximation as a `PatchInstrument` with no warning, dropping the instrument's envelope and the settings no node holds (Drum `sweep`, `noise`, `decay`; Bow `vibrato`; Operator `attack`, `release`, `bloom`, `detune`). Fix: express every built-in as a patch of synth modules that renders the same samples (a test compares the two renders), migrate `tracks/*.json`, then delete the built-in instrument variants and their renderers.

### Open defects from the adversarial review of 21 November [DAW-REVIEW-OPEN]

Each defect the review found sat on a path with no test at its boundary. A new control, size or limit lands with a test at its edges, at every size it takes.

- [ ] The far Space room scales the size by 0.91, so a Space size below 0.2748 refuses the whole render.
- [ ] `Arrangements::frames` ignores sustain pads, so pads and their reverb are cut at the end of the piece.
- [ ] At 8 kHz a 0.5 s intro tail gives 4,000 samples, under the 4,096 a sustain needs.

## Transport [DAW-TRANSPORT]

- [ ] The renderer is faster than real time: see [DAW-TRANSPORT-LIVE] at the top of this plan.

## One graph [DAW-GRAPH]

**The project is one big graph of cables, and a patch is a subgraph of it.** Every item below builds on that: tracks, buses, Space and Master are nodes; a track's patch is the graph nested inside its node, and its Output is the track's output. Play and render run the whole graph in one block loop into Master. No work item may add a per-track render path, a separate mix stage or a second routing model.

- [ ] One block loop runs every track, bus, send, pad and the Space into Master ([DAW-RENDER-PLAY]). `daw::Playback` (in progress, OspreyOpus1): the render and the journey test run on it. Patch modules still render a whole note when it starts, inside its track node. Running them in the same block loop is the next step ([DAW-GRAPH-SUBGRAPH]).
- [ ] A patch's audio and MIDI inputs take cables from other nodes in the project graph ([DAW-MODULAR]).

## Rendering the song [DAW-RENDER]

- [ ] Natively verified on the FerroUI host with the codex project.

## Timeline transport [DAW-TRANSPORT-TIMELINE]

- [ ] Playhead in beats, saved in the project file.
- [ ] Range markers, Loop and the following playhead: see [DAW-TRANSPORT-LIVE] at the top of this plan.

## Dynamics [DAW-DYNAMICS]

- [ ] `Compressor` and `Limiter` components in `@osprey/synth`, any channel count, linked (synth-advanced). Partial: `synth::Limiters` (4e212e09f) limits a linked stereo buffer: the lowest true-peak need (4× oversampled, ITU-R BS.1770-4) is held across the lookahead with a linear-time block minimum, released upward and averaged over the window, so no frame or inter-sample peak passes the ceiling before the final clamp; quiet signals pass bit-exact. The synth test covers 4 settings × 6 signals, impulses, a quarter-rate 45° sine (samples at 0.707, true peak 1) and 9 refusals. Not yet a graph node or any channel count; no compressor component.
- [ ] Tests: sine against the curve within 0.01 dB; inter-sample peaks under the ceiling (synth-advanced).
- [ ] Gain reduction and true-peak meters on the Mix page.
- [ ] Multiband compressor and multiband limiter ([DAW-DYNAMICS-MULTIBAND]): Linkwitz–Riley crossovers (3–5 bands) that sum flat, per-band dynamics; modules the user adds to a patch, a bus or Master, with no built-in Master chain; settings in daw.td; native Osprey formulas and Lean proofs; flat-sum and per-band tests.
- [ ] Pitch effects on tracks and buses ([DAW-PITCH-EFFECTS]): the FFT pitch shifter in semitones and cents, and autotune following key/scale or the timeline's notes, each with latency compensation in the renderer.

## Buses and sidechain [DAW-MIX-BUS]

- [ ] Bus strips on the Mix page: EQ and compressor controls, gain reduction meter.
- [ ] Natively heard on the full codex render.

## Best-of-breed EQ [DAW-MIX-BUS-EQ]

The three-band split is a start, not the EQ. Target: a parametric EQ as good as the best commercial ones, on every track, bus and Master.

- [ ] Up to 8 bands: bell, low and high shelf, notch, band-pass, tilt, and high- and low-pass at 6–48 dB/oct. Biquads from the RBJ cookbook, implemented in native Osprey; each band has frequency, gain and Q.
- [ ] Analog-matched response near Nyquist (no cramping): oversampled or matched-z design. A linear-phase mode for mastering, with its latency reported.
- [ ] Mid/side and left/right per band. Dynamic EQ: a band's gain follows a threshold and can be keyed by a sidechain.
- [ ] Smooth parameter changes without zipper noise or filter blow-up under automation.
- [ ] Spectrum overlay with the curve drawn on the Mix page, and a band dragged on the curve.
- [ ] Tests with many input permutations: the magnitude and phase response measured on swept sines at many frequencies, gains and Qs, each within 0.01 dB of the analytic curve; flat at 0 dB within 1e-9; stable at the extremes; settings in daw.td round-trip through the project.

## Songs that push the synthesis [SYNTH-SHOWCASE]

- [ ] Original example songs in the spirit of Tipper, Amon Tobin and Mr. Bill (inspiration only; no copying of their tracks or sounds). Each pushes every technique as far as it goes: granular micro-edits and glitch, spectral freeze, morph and formant shifts, FM and phase-distortion basses, physical models, resampled foley, heavy sidechain and bus processing.
- [ ] Each song passes the mix checks ([DAW-ANALYSIS-GATE]) for its genre.

## Tests and tooling [DAW-TESTS]

- [ ] Every journey gets more input permutations and more assertions per interaction, across the daw and @osprey/synth.
- [ ] Scripts and tooling are written in Python-flavour Osprey, not Python. Tonight's one-off migrations were Python and are not committed.
- [ ] Consolidate: one spec, one plan and one test home per feature; merge overlapping sections and duplicate helpers (the deslop gate must go green).

## Spectral analysis and mix advice [DAW-ANALYSIS]

- [ ] **Track scope on the main view ([DAW-ANALYSIS-TRACK]).** Reported by the user on 2026-11-21: "scope is useless in the patch. we need to be able to see this from the main view. Potentially this should not be a node at all, maybe just an extra thing that you can see when selecting a channel/track. And again, this should tell the user if it's hitting the right frequencies for what it is." Selecting a track on the Timeline shows that track's waveform and band spectrum in the track panel, with no Scope module needed in its patch. The panel states the verdict in words: the range its notes ask for, the range where the energy is, and what is wrong (for example "bass: most energy above 2 kHz"). The patch's Output card stops drawing an empty trace.
- [ ] Band spectrum per track, bus and Master from the STFT, with peak and RMS, on the Mix page. Partial: `synth::Spectra` reads ten octave bands (31.25 Hz–16 kHz) from Hann-windowed FFT frames, mean and loudest level, where a full-scale sine reads 0 dB; tested within 0.1 dB on single and paired sines, silence at −120, a half-length tone 3 dB below its peak, 4 refusals (4e212e09f). Every render prints Master's bands (1b45a3ba0). Per track and bus, and the Mix page, remain.
- [ ] Pitch fit: measured fundamental and partials against the played notes, detuning in cents.
- [ ] Per-instrument expected ranges in daw.td; energy outside them reported.
- [ ] EQ advice (band, dB, reason: masking, resonance, mud, harshness, low end), applied as a checked project edit.
- [ ] Masking report between tracks in the same band.
- [ ] Native Osprey formulas, proofs of their production bodies, and tests on synthetic signals with known spectra.
- [ ] Mix checks per example project, run by `make` and red on failure: tonal balance against the genre curve, LUFS and true peak, clipping, masking, mono low end, pitch fit ([DAW-ANALYSIS-GATE]).
- [ ] Every project in `tracks/` passes its mix checks; fix the mixes that fail.
- [ ] Checks per window along the song; a failure alerts at once and marks the bars on the Timeline with track, check, value and limit; tapping it seeks there ([DAW-ANALYSIS-TIMELINE]).
- [ ] `genre` in daw.td; genre target curves, loudness and dynamic range as versioned data ([DAW-ANALYSIS-GENRE]).
- [ ] AI review behind an effect: measured spectrum, loudness and masking plus genre in, typed findings out, advisory only, pinned model, timeout ([DAW-ANALYSIS-AI]).

## MIDI keyboard [DAW-MIDI]

- [ ] Realtime audio path: the renderer meets [DAW-PERF], then a callback backend plays one track's patch live.
- [ ] Host lists MIDI sources and delivers timed events through the native boundary (CoreMIDI first).
- [ ] Notes play the selected track's patch.
- [ ] Record into the timeline at the playhead.
- [ ] Controller learn, stored in the project.

## OSC [DAW-OSC]

- [ ] OSC 1.0 packet reader and writer in Osprey with bounds checks, as a package with tests.
- [ ] UDP listener on localhost behind an effect.
- [ ] Addresses generated from daw.td; messages go through the same checked edits.
- [ ] Feedback of playhead, meters and changed values.

## Pages [DAW-PAGES]

- [ ] Screenshot goldens compared automatically in the app test targets.
- [ ] Fix: a render cancelled mid-way leaves a 0-byte `piece.wav` that the next Play replays as unchanged (found by [DAW-VIDEO]).

### Menu bar and header [DAW-MENU] [DAW-HEADER]

- [ ] Provide separate Timeline and Mix windows where the host supports them. The required independent track patch window is tracked under [DAW-PAGES-PATCH-WINDOW].

### Human-centred UI, user orders of 21 Nov [DAW-HUMAN]

Spec: [Daw.md](../../specs/synth/Perception.md#perception-studio-the-daw-daw) sections [DAW-HUMAN], [DAW-MODULAR], [DAW-CONTROLS], [DAW-ACCESS], [DAW-COMMANDS], [DAW-KEYS], [DAW-STORE], [DAW-INSPECTOR].

- [ ] **Dials ([DAW-CONTROLS-DIAL]).** Every numeric field is a round dial with its number: press, hold, move up or down; Shift for fine steps; double-press for the default; typing and arrow keys. Slider (horizontal or vertical) where a length reads better. Design worked out with prism-routing-design. Owner: ferroui-prism-lead. Built (c608c5131) in Patch Controls and the canvas inspector: 180 units of travel cross the range, Shift ten times finer, sideways does nothing, one undo per turn, Escape puts the value back, a tap opens the number to type, each dial reports name, value and range. Not built, tracked under Accessibility and Patch Controls: arrow-key steps and focus, double-press for the default (a `Parameter` has no default field yet), dials on the cards, sliders. Not yet compared with `docs/designs/prism-timeline/patch-controls.png` in a live window.
- [ ] **Patch Controls uses the design's controls.** The text boxes, "Connect/Disconnect" buttons and platform-styled menu in that panel are replaced by the toggle, dial, slider, menu and button of [DAW-CONTROLS]; the number box that reformats while typing (392 became 923.0) goes with them.
- [ ] **Command palette ([DAW-COMMANDS]).** One command list feeding menus, shortcuts and a VS Code style palette.
- [ ] **Timeline zoom ([DAW-TIMELINE-ZOOM], user order).** Pinch, Cmd-wheel and Zoom In/Out/Fit/Selection commands with shortcuts in all three keymaps; zoom about the pointer (keys: about the playhead); the song strip's frame drags to move and its edges to zoom; row height with Alt; ruler divisions follow the zoom; zoom kept per project and never an undo step. Needs: a zoom and offset in the view model (`daw.td`), `timeline_geometry.ospml` taking them, wheel and pinch events from the host. Owner: ferroui-prism-lead. **Built so far (dfb97ae79):** Zoom In, Zoom Out and Zoom to Fit in the View menu with keys in all three keymaps, about the playhead, from the whole song down to one beat; lanes, traces, ruler divisions (bars, beats, sixteenths), taps and a marker under the song overview follow the beats in view; the view follows the playhead; zoom is not an undo step. **Not built:** pinch and Cmd-wheel (the host sends no wheel or pinch event yet), Zoom to Selection, dragging the strip frame, row height, the piano roll's own zoom, zoom kept per project.
- [ ] **Stacked commands ([DAW-COMMANDS]).** A stack is a named list of commands with its own parameters (a macro), one undo step, all or nothing, in an editable file. A shortcut runs a command or a stack, with arguments if the keymap line gives them. Needs the one command list first.
- [ ] **Design screenshots are gorgeous (user order).** Every PNG in `docs/designs/prism-timeline` is finished art at 2×: one type scale, one spacing grid, the Prism palette only, real project content (no placeholder names), no clipped or overlapping text. prism-routing-design draws; the lead signs each off before code follows it.
- [ ] **Commands for an AI agent ([DAW-COMMANDS]).** Run a command by name with arguments from outside the window, one line in and one line out; list the commands and what can run now.
- [ ] **Accessibility ([DAW-ACCESS]).** Focus and keyboard operation for every canvas control, role/name/value/state reported for each, contrast and colour-independence checked, reduced motion and larger text followed.
- [ ] **Pluggable storage ([DAW-STORE]).** One store interface with JSON (today), database and git (a commit per save, history, restore) behind it.
- [ ] **Patch Output and inputs ([DAW-MODULAR]).** A patch ends in an Output routed to Master by default; the skin's "Master" card is renamed and no module is called master. Patches declare audio and MIDI inputs as modules and route them with cables.
- [ ] Windows goldens under `examples/daw/goldens/windows` predate the new header and inspector and must be rewritten on Windows (`make -C src/toolchain/targets/windows/tests synth-update`); `routed-delay.png` is the one reported.
- [ ] Real mouse drag on sockets and the menu bar need rechecking in the live app once the desktop is unlocked (the screen was locked during this work).

### Strip the UI [DAW-PAGES-STRIP]

From the committed screens: cut everything that repeats or says nothing (ferroui-prism-lead owns the views).

- [ ] Patch cards: drop the small name above each card (the title already says it) and "Edit in Patch controls" on every card and in the inspector; tapping the card is the edit.
- [ ] Round values for display: "261.6255653005986" shows as "261.63 Hz" (C4). Units on every value.
- [ ] Drop "1 inputs · 1 outputs" (the ports show it) and the bottom legend of cable kinds.
- [ ] Timeline: the piano-roll "Bar 23" label must not collide with the "Piano roll" title.

## Verification

- [ ] The duplication gate is green: 4.1% against the 4.00% ceiling on 21 Nov.
- [ ] `make ci` passes; a coordinated push is made under the TMC lock.

## Performance [DAW-PERF]

Profiled 21 Nov with `sample` at 1 ms. Each fix is owned, and is done only when its owner reports the same measurement again with the target met.

**Engine.** `target/daw/daw tracks/codex-version.json <out> 32 8` (bars 9 and 10, ARC): 42.2 s of CPU for 27.5 s of audio, so 0.65× real time. Target: under 14 s (2× real time).

- [ ] [DAW-PERF-RENDER] A song with basic tracks and routing renders in at most a fifth of its length (5 minutes in about 1 minute), and no project in `tracks/` takes more than half its length. Phosphor Drift on 21 Nov: 1229 s for 226.9 s, so 5.4× its length, about 27× too slow. The pi bench (pi-render-bench-1..3) is measuring the other projects. Per module, 1 beat of Violet Static (beat 64, one track alone): the bowed+additive+pitchShifter+spectral Choir smear took over 150 s, bowed swapped for a wavetable still over 150 s (additive), wavetable+LFO lowpass+shifter 8 s; the Melt lead fell from 29 s to 5 s without its FFT pitch shifter. A full Violet Static (54.9 s) did not finish in 1800 s, so bowed, additive, pitchShifter and spectral are the first kernels to optimise.

| Where the time goes | Share | Fix | Owner |
|---|---|---|---|
| Reference counting and allocation (`arc_drop_child`, `osp_retain`, `osp_release`, `arc_alloc`, `memset`) | 78% | See the next three rows; the DSP arithmetic itself is under 5% | synth-advanced |
| `RenderInputs::emitted` copies every kernel state for each node of each sample, and `held` filters them again | largest | Keep states in schedule order and fold over them, so one node costs one state | synth-advanced |
| Delay lines are rebuilt each sample (`listAppend tail input` copies the whole line) | large | A ring: fixed storage with a write position | synth-advanced |
| `RenderInputs::parameter` finds each value by node and string key for each sample (`strlen`, `memcmp`, `filterList`) | 6% plus its allocations | Resolve each node's parameters, inputs and `enabled` once per block into the plan | OspreyOpus1 |

Measured attribution (self plus runtime time, of 15,300 samples): `RenderInputs::parameter` 19%, an anonymous per-sample closure 11%, dropping the old frame 7%, `RenderInputs::input` 7%, `emitted` 6.5%, `Arrangements::bused` 5%, `Graphs::node` 4.5%, `held` 4.5%, `RenderKernels::step` (the `enabled` string test) 4%.

Tried and rejected: replacing `listGet (filterList …) 0` with a `foldList` search doubled the time (42.3 s to 88.7 s, identical audio). A closure call per element costs more than the built-in filter, so the lookups must leave the sample loop, not be rewritten inside it.

Also tried and rejected: narrowing the snapshot to one component's parameters before its lookups left the time unchanged (42.25 s). The lists are already short; the cost is the fixed price of each lookup, allocation and state copy, paid once per component per sample.

**Decision: render in blocks ([SYNTH-RENDER-BLOCK]).** Each component renders a whole block (64 samples by default) from its input blocks in one call. Parameters, inputs and state are resolved once per block, and state is written back once per block, so the fixed price is divided by the block size and the arithmetic is what remains. A feedback cable delays by one block, or its cycle runs at a block of one sample when the patch asks for sample-exact feedback. Automation splits a block at its event. The audio of a patch without feedback must stay bit-identical to the reference renderer, which stays as the oracle in tests.

- [ ] Block kernels for oscillator, filter, gain, mixer and output, checked bit-identical against the reference (synth-advanced).
- [ ] The remaining kernels, then the daw player renders through blocks (synth-advanced, OspreyOpus1).
- [ ] States folded in schedule order; before and after times reported (synth-advanced).
- [ ] Ring delay lines. Blocked (synth-advanced, 21 Nov): an Osprey list has no in-place update, so a ring still copies; it needs a runtime primitive for a fixed float buffer, kept in C.
- [ ] Parameters and inputs resolved once per block; before and after times reported (OspreyOpus1).
- [ ] Bars 9 and 10 of the codex project render in under 14 s.

**Runtime and code generation ([MEM-ARC-RECENT]).** The renderer is slow at every level under it, so each level has its own row. Second profile after the first fix (10,300 samples): `arc_release_zero_locked` 17%, `arc_drop_child` 15%, `arc_alloc_locked` 7%, `osp_release` 6%, `osprey_list_get` 6%, `osp_retain` 6%, `memset` 5%, `osp_arc_free` 4%, `osp_mem_string_length` with `strlen` 6%. The Osprey code itself is under 5%.

| Level | Bottleneck | Fix | Result or owner |
|---|---|---|---|
| ARC runtime | Every dup, drop and child drop probed the registry hash table to ask "is this ours?" | Direct-mapped cache of recent members, cleared where membership ends | Done: 42.3 s to 35.1 s, identical audio |
| ARC runtime | A freed large block left a tombstone, so probe runs grew until the next rebuild | Backward-shift deletion, no tombstones | Done: no measurable change once the cache was in (35.9 s); kept because probe length no longer depends on churn |
| ARC runtime | A string literal is in rodata, so its length, dup and drop each ran a whole probe to miss | Cache of recent non-members, cleared on insert | Done: 35.0 s |
| ARC runtime | A list of records drops each element through the probe (`arc_drop_list_hdr`) | Tag a list whose elements the compiler proves are ARC bodies, and drop them by direct header read, as `MASK_DIRECT` does for records | Open (lead) |
| ARC runtime | `osp_mem_string_length` takes the heap lock and looks the string up for every comparison | Store the length beside the pointer for literals, or compare by pointer first when both are literals | Open (lead) |
| Code generation | A record update (`run(held = …)`) allocates a new record and dups every field although the old one is unique and dead | Reuse the unique record in place: the drop-then-allocate pair of the same layout becomes a field store | Open (OspreyOpus1) |
| Code generation | `mapList`, `filterList` and `listAppend` allocate a new list for each call inside the sample loop | Reuse the argument's storage when it is unique; fuse `listGet (filterList …) 0` into a search with no list | Open (OspreyOpus1) |
| List runtime | No fixed float buffer, so a delay line is copied per sample | A `FloatBuffer` primitive in C with checked index, behind a `Result` | Open (synth-advanced reported the blocker) |
| Renderer | The fixed price per component per sample | Block rendering, below | synth-advanced, OspreyOpus1 |
| daw mix | `BusMix` and `Arrangements::bused` build each side with `listAppend` and `listGet` per sample | Fold into a builder once per side; measured at HEAD with buses: 35.6 s, so no regression yet | synth-advanced |

Tried and rejected in code generation: stopping `listGet (filterList …) 0` at its first match (the list then holds one element). 34.8 s against 35.0 s, identical audio, so it was backed out: the cost is building and dropping the one-element list, not the scan. The fix that pays is a search that returns the element with no list at all, which needs the fused `match listGet` path to own the element across the loop's region.

The runtime fixes alone reach 0.79× real time. The 2× target needs the code generation rows and block rendering; no runtime change can remove allocations the generated code asks for.

- [ ] Direct element drops for proved lists.
- [ ] In-place reuse of unique records and lists in generated code.
- [ ] `FloatBuffer` primitive.

**Accountability, 01:06 on 21 Nov.** Each owner was asked for a hash and a measured time.

| Owner | Asked for | Delivered |
|---|---|---|
| ferroui-prism-lead (lead) | Interface target; runtime and code generation rows | Interface target met (4.8 ms mean). ARC registry b730e7d28: 42.3 s to 35.0 s. Two code generation and two renderer experiments measured and rejected, recorded above. Engine target not met: 0.79× real time against 2× |
| synth-advanced | States folded in order, ring delays, block kernels | None landed. Reported the ring-delay blocker (no fixed buffer primitive). Landed buses with EQ and a sidechain compressor on `DynamicsMath.lean` instead (cf62e50b7), committed while HEAD did not compile, tested on a dirty tree. No reply at 01:06 |
| OspreyOpus1 | Parameters resolved once per block | Not started. Landed Split, Join and Route (bb930140d) without the daw's exhaustive match, so HEAD was red for 12 minutes until 6fc3979d6; the second red HEAD of the night. No reply on performance at 01:06 |

Rule from tonight: a commit to `packages/synth` is tested against `examples/daw` on a clean `git archive HEAD` copy before it lands.

**Open defects reported by OspreyOpus1 from the native journey (not yet reproduced by a test).**

**Interface.** `tests/prism_events.rs` drag: 10.6 ms mean, 18.2 ms worst per pointer move. Timeline taps: 0.57 ms mean. Target: drag under 5 ms mean, under 8.3 ms worst (one 120 Hz frame).

| Where a drag move goes | Share | Fix | Owner |
|---|---|---|---|
| `EditorView::keyboardDock` is rebuilt although nothing in it changed | 28% | Cache it with the card art | ferroui-prism-lead |
| `CardArts::card` redraws the dragged card, because its key holds its position | 25% | Draw each card at the origin once and move it | ferroui-prism-lead |
| `Tellings::render` and `Streams::write` encode the whole scene | 30% | Follows from the two rows above and the next | ferroui-prism-lead |
| `PrismArt::backgroundPart` re-emits the scenery | 13% | Keep the built scenery in the editor | ferroui-prism-lead |

### Profile round, 21 Nov 10:30 [DAW-PERF-ROUND]

Measured at 486b165cf with `make -C src/toolchain/targets/macos/tests daw-profile` (new: `scripts/make/daw-profile.sh`, the ignored host test `tests/daw_profile.rs`, and `scripts/make/sample-attribution.py`, which names the Osprey function behind each runtime call in a 1 ms `sample`). Project: `tracks/codex-version.json`, ARC. A row below is done only when its owner runs the same target again and writes the new number beside the old one.

**What a user action costs** (milliseconds in the app's `event` export, mean of 117 or more events; the host adds 0.05):

| Action | Timeline | Mix | Patch (64 shapes) |
|---|---|---|---|
| Any pointer event, key or frame tick, **including one that changes nothing** | 2.0 to 2.1 | 1.5 to 1.6 | 0.47 to 0.50 |
| Window resize | 12.4 | not measured | 3.2 |
| Open the page | 2.3 | 1.7 | 3.3 |
| Show or hide Patch Controls, Keyboard | 5.0, 5.3 | | |
| Launch with the song | 50 | | |

Every event fits a 120 Hz frame, so nothing here is felt yet. The finding is the shape: the cost does not depend on what happened. A frame tick that changes nothing costs the same as a tap, because the whole page is rebuilt and rewritten on every event, and a resize costs six times that.

**Interface: where the time goes.** Share of app samples; "tick" is the Timeline frame tick (3,843 samples), "resize" the Timeline resize (3,870), "mix" the Mix tick (3,923), "patch" the Patch tick (3,842).

| Function (file:line) | Share | Cause | Fix | Owner |
|---|---|---|---|---|
| `synth::Header::placed`, `placedAreas` ([header_view.ospml:149](../../../src/osprey/examples/daw/src/header_view.ospml), :152), through `Header::shift` and `ui::Machines::number` | tick 34.5%, mix 50.3%, tap 26.7% | The lambda calls `shift editor` once per shape (about 400). Each call reads two knobs by name | Bind the shift once, outside the lambda | ferroui-prism-lead |
| `ui::Machines::knob` ([machine.ospml:112](../../../src/osprey/packages/native/src/machine.ospml)) and its predicate `k.knobName == name` (`__closure_fn_12`) | tick 35.9%, resize 42.1%, mix 51.8% | A knob is found by comparing its name with every knob's name, for every read, through `ui::Lists::find` ([base.ospml:23](../../../src/osprey/packages/native/src/base.ospml)): a whole-list filter, a list and a boxed result per read | Keep knobs in a map by name, or resolve a name to its index once per frame | ferroui-prism-lead |
| `ui::Streams::write`, `etches`, `path`, `step` ([stream.ospml:55-62, 76-78, 106, 116](../../../src/osprey/packages/native/src/stream.ospml)) | tick 41.5%, tap 44.4%, patch 21.6% | Every path step is built as a record inside a one-element list; `Lists::flatten` joins them; only then are the rows written. One step costs a record, a list, a builder and a join | Write each row to the bridge as it is produced, folding the refused count; no `List<Etch>` in between | ferroui-prism-lead |
| `ui::Lists::spanned` ([base.ospml](../../../src/osprey/packages/native/src/base.ospml), called by `flatten`) | tick 16.9%, tap 17.6% | Joining by halves still copies every element log n times and retains and drops each one | Goes away with the row above; elsewhere, one builder that every group is pushed into | ferroui-prism-lead |
| `ui::Surfaces::shown` and `ui::Tellings::render` (the whole telling) | 97.5% of every event | No event is ever answered with "nothing changed": the view is rebuilt, told and written even when the stamp is the same | Compare the root stamp first and answer an unchanged event without building the view; tell only regions whose stamp changed | ferroui-prism-lead |
| `synth::CardArts::refreshed` ([card_art.ospml:28](../../../src/osprey/examples/daw/src/card_art.ospml), from [editor_app.ospml:213](../../../src/osprey/examples/daw/src/editor_app.ospml)), `CardArts::card`, `PrismArt::drawn`, `ui::Canvases::artboard`, `ui::Templates::measure` and `fill` | resize 78.9% (card 64.8%, artboard 58.1%, measure 43.1%, fill 41.6%) | A resize redraws the art of every Patch card, whichever page is showing, and each template piece reads its knobs by name | Refresh card art only on the Patch page and only when the card scale changed; give a card its knob values once, as a record | ferroui-prism-lead |
| `synth::PrismArt::atlas` | patch 14.4%, mix 5.4%, tick 3.7% | The atlas list is rebuilt for every event | Build it once at launch and keep it in the editor | ferroui-prism-lead |
| `synth::Header::patchSubtitle`, `EditorView::toolbar` | patch 7.7%, 8.8% | Rebuilt per event although they change only with the page and the track | Cache with the header, keyed by page and track | ferroui-prism-lead |
| `synth::Graphs::node` | patch 4.4% | Finds a node by filtering the whole node list | Same search fix as `Lists::find` (code generation row 2) | synth-advanced |

Targets: an event that changes nothing under 0.1 ms; a Timeline or Mix event under 0.7 ms; a resize under 2 ms.

- [ ] Unchanged events answered without rebuilding the view.
- [ ] Atlas, toolbar and patch subtitle cached.
- [ ] A Patch resize redraws a card only when the card scale changed (2.84 ms today, every card redrawn).

**Engine.** Bars 9 and 10 again: 36.0 s (35.4 s of CPU, with the sampler attached). Nothing has moved since 35.0 s, and none of the block-rendering items above has landed. The same functions hold the time (12,808 samples, own code plus the runtime each calls directly):

| Function (file:line) | Share | Cause |
|---|---|---|
| `synth::RenderInputs::parameter` ([render_inputs.ospml:17](../../../src/osprey/packages/synth/src/render_inputs.ospml)) | 23.2% | `listGet (filterList (snapshot.parameters, \p => p.node == identity && p.key == key)) 0` per parameter per node per sample: a string compare per parameter, a list and a boxed result per read |
| The per-sample closures of `synth::Renderer` ([renderer.ospml:46, 87, 91, 94](../../../src/osprey/packages/synth/src/renderer.ospml)): `sample`, `evaluated` (the fold over `plan.order` calling `nextNode`) and `finished` | 9.4% + 5.8% + 3.0% own, 92% inclusive | Four closures deep per sample, each a heap environment and a boxed `Result` through `Outcomes::carried`. `finished` searches `outputs plan.graph` again, appends one sample with `listAppend` and rebuilds every meter with `metered`, per sample |
| `RenderInputs::input` 6.4%, `emitted` 5.5%, `held` 4.3%, `Graphs::node` 4.2%, `RenderKernels::step` 4.2%, `RenderKernels::selected` 2.3% | 27% | State and node found by filtering lists, per node per sample |
| `daw::Arrangements::bused` | 3.6% | A list built per sample per side |
| `PartialBanks::partial`, `stepped` | 4.0% | One record per partial per sample |

Leaf time is the runtime, not arithmetic: `arc_drop_child` 18.2%, `arc_release_zero_locked` 18.1%, `osp_retain` 7.8%, `arc_alloc_locked` 6.9%, `osprey_list_get` 6.6%, `osp_release` 6.5%, `memset` 5.1%, `osp_mem_string_length` 3.0%.

- [ ] `RenderInputs::parameter`, `input`, `held` and `Graphs::node` resolved once per block into the plan (the existing block-rendering items; owner still OspreyOpus1 and synth-advanced, nothing delivered).
- [ ] `Renderer::finished` finds the sink once per render, collects a block's samples in one builder and meters once per block, not per sample.

**Code generation and runtime: what the generated code does that it need not.** Each row was read from the LLVM IR of the daw (`osprey src/osprey/examples/daw --llvm --memory=arc --target=macos`) and matched to the samples. These cost every Osprey program, not only the daw.

| # | What is generated | Evidence | Cost measured | Fix |
|---|---|---|---|---|
| 1 | **String `==` is an ordering compare with two locked length lookups.** `a == b` emits `call i32 @osp_string_compare` then `icmp eq 0`. `osp_string_compare` ([string_runtime.c:63](../../../src/toolchain/compiler/runtime/string_runtime.c)) takes both lengths through `osp_strlen`, which calls `osp_mem_string_length` ([memory_arc.c:234](../../../src/toolchain/compiler/runtime/memory_arc.c)): the heap lock and a registry lookup each, a `strlen` when the string is a literal, then `memcmp` | IR of `__closure_fn_12`; `osp_mem_string_length` + `memcmp` + `strlen` + `osp_string_compare` | tick 23%, resize 31%, render 7% | Emit `osp_string_equal` for `==` and `!=`: equal pointers are equal; otherwise one pass over the bytes with no length lookup. Against a literal, pass the literal's length, known at compile time |
| 2 | **`listGet (filterList (xs, p)) 0` scans the whole list and builds a list.** Emitted as: closure environment `osp_alloc_tagged`, `osprey_list_builder_new_of`, a loop over every element (`osprey_list_get`, indirect call, `osprey_list_builder_push_borrowed`), `osprey_list_builder_seal`, `osprey_list_in_bounds`, `osprey_list_get` | IR of `ui::Machines::knob` (143 lines for a one-line function) | `ui::Lists::find`, `RenderInputs::parameter`, `Graphs::node`: tick 36%, render 23% + 4% | Recognise the shape, or add a `listFind` built-in: a loop that returns at the first match, with no list and no builder. (Stopping early while still building the one-element list was tried and rejected above; the list is the cost) |
| 3 | **A lambda written at a `mapList`, `filterList` or `foldList` call is still a heap closure called through a pointer.** `osp_alloc_tagged(16, 513)`, `osp_retain` of each capture, then per element `load` of the function pointer and `call i1 %r21(...)`, so `-O2` cannot inline the body | IR of `ui::Machines::knob`, lines 20 to 46 | One allocation and one release per call site per call, and an indirect call per element everywhere | When the argument is a literal lambda, emit its body in the loop (or a direct call with the captures on the stack); no heap environment |
| 4 | **Every element read in a generated loop is an out-of-line `osprey_list_get`**, which checks the bounds the loop has already established; a checked `listGet` is two calls (`osprey_list_in_bounds`, `osprey_list_get`) | IR of every `mapList`/`filterList`/`foldList` loop | 6.2% to 6.6% of leaf time in all four profiles | Read the element inline when the loop bound proves the index; or ship the runtime's fast paths (`osprey_list_get`, `osp_retain`, `osp_release`) as bitcode so `-O2` inlines them |
| 5 | **A list literal goes through a builder.** `[x]` is `osprey_list_builder_new_of`, `osprey_list_builder_push_borrowed`, `osprey_list_builder_seal`, with `osp_alloc` and `osp_alloc_tagged` | IR of `ui::Streams::step`: five builders for five one- or two-element literals | Part of `Streams::step` and `path` (tick 20%) | A literal of known length is one allocation with its elements stored directly |
| 6 | **A new list is tagged by a second locked call.** `osp_mem_set_layout` ([memory_arc.c:323](../../../src/toolchain/compiler/runtime/memory_arc.c)) takes the heap lock and looks the block up again, straight after the allocation, for every list header and node ([list_trie.h:86, 125](../../../src/toolchain/compiler/runtime/list_trie.h)) | Leaf samples | tick 4.9%, resize 2.0%, render 1.8% | Pass the layout to the allocation |
| 7 | **Every `Result` is a heap object**, and turning one error into another allocates a second. `listGet` allocates `osp_alloc_tagged(24, 1281)` for its answer; `match … Error _ => Error(…)` allocates another and retains the message literal | IR of `ui::Machines::knob`: three `osp_alloc_tagged(…, 1281)` for one lookup; function type `{ i64, i8, i8* }*` | Part of `arc_alloc_locked` + `arc_release_zero_locked`: tick 11%, render 25% | Return a `Result` by value (two registers or `sret`) when it is matched at once and never stored |
| 8 | **A pure call that does not depend on the lambda's argument is evaluated for every element.** `mapList (shapes, \shape => moved (shape, shift editor))` calls `shift editor` 400 times | `Header::placed`, tick 34.5% | See the first interface row | Hoist a call to an effect-free function out of the loop when none of its arguments mentions the lambda's parameters |
| 9 | **Retain and release are emitted for string literals.** `call void @osp_retain(i8* %r40)` where `%r40` is `@.str.174` in read-only data; the runtime probes to find it is not its own | IR of `ui::Machines::knob` | Part of `osp_retain` (tick 11.1%) | The compiler knows a literal: emit no retain or release for it |
| 10 | **`listConcat`, `flatten` and `mapList` are never fused.** `listConcat [head] (listConcat (Lists::flatten (mapList (steps, step))) …)` allocates each intermediate list | `ui::Streams::path` (stream.ospml:76-78) | tick 10% own plus `Lists::spanned` 17% | Lower nested concat, flatten and map into pushes on one builder |
| 11 | Dropping a list of records drops each element through the registry (`arc_drop_child`) | Leaf samples | tick 16.8%, resize 11.1%, render 18.2%: the largest single leaf in three of four profiles | Already open above ("Direct element drops for proved lists"); this round says it is the first runtime fix to make |
| 12 | A record update allocates and copies | Already open above | | In-place reuse of a unique record |

Order by what it buys across all four profiles: 11 (drops), 1 (string equality), 2 (find), 7 (`Result` by value), 3 (inline lambdas), 4 (inline element reads), then 5, 6, 9, 10, 8.

- [ ] 1. Equal strings still take the slow path in `osp_string_compare` (one hit per search); pass a literal's length at compile time.
- [ ] 2. First-match search with no list for `listGet (filterList …) 0`. Done by hand in `ui::Lists::find`; the compiler still builds the list at every other site (`RenderInputs::parameter`, `Graphs::node`, `CardArts::fixed`).
- [ ] 3. Literal lambdas at built-in loop sites emitted inline, no heap environment.
- [ ] 4. Inline element reads in proved loops, or runtime fast paths as bitcode.
- [ ] 5. List literals as one allocation.
- [ ] 6. List layout passed to the allocation; `osp_mem_set_layout` off the allocation path.
- [ ] 7. `Result` returned by value when matched at once.
- [ ] 8. Loop-invariant effect-free calls hoisted out of lambdas.
- [ ] 9. No retain or release on string literals.
- [ ] 10. Concat, flatten and map fused onto one builder.
- [ ] Each of these lands with a regression program in `src/osprey/tests/regressions/` and the before and after of `make daw-profile`.

### Build time, 21 Nov 12:40 [DAW-PERF-BUILD]

Building the daw must take seconds. Measured with `osprey src/osprey/examples/daw --compile --memory=arc --target=macos` and a 1 ms `sample` of the compiler.

Where the 50 s go now (36,400 samples, then `clang`):

| Phase (function) | Time | Cause | Fix |
|---|---|---|---|
| Code generation: `osprey_codegen::lower::compile_module` | 18.6 s | 7.2 s of it is `aggregate::gen_generic_record` → `closure::specialized_named_cell` → `closure::source::emit_closure_keyed`, and 3.9 s is `genfn::try_inline` → `monofn::specialize_values` → `closure::lower_fn_body`: a function or closure is lowered again for each place that names it with the same types | Keep one lowered body per (function, type arguments) and reuse it |
| `clang` on the emitted IR | about 14 s | One translation unit, optimised whole | Emit per module and compile in parallel; cache objects by IR hash |
| Type check: `osprey_types::check::driver::check_and_publish` | 8.4 s | `osprey_ast::lower_static::hygiene::resolve` 2.0 s, `check_data_contracts` 2.3 s, `effect_rows::solver::converge` 1.0 s | Profile each on its own; none is a single loop |
| Unused-binding warnings: `osprey_types::unused::unused_symbols` | 4.7 s | Calls `infer_checked`: a second whole-program inference, only to learn method targets and constructors the gate's solve already had | Keep the source solve from `staging::lower_proved` in `CheckedProgram` and read it here |

- [ ] One lowered body per function and type arguments in code generation.
- [ ] `unused_symbols` reuses the gate's solve.
- [ ] Objects compiled per module, in parallel, cached by hash.
- [ ] Target: a full daw build under 10 s, an unchanged rebuild under 2 s.
