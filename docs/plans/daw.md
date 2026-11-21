# The daw: delivery plan [DAW-PLAN]

The [daw specification](../specs/applications/Daw.md) defines the requirements. The [Timeline designs](../designs/prism-timeline/README.md) are the visual reference, within the [scope](../specs/applications/Daw.md#scope-daw-scope) the specification sets.

## Real-time play [DAW-TRANSPORT-LIVE]: the first blocker

**The user's verdict (2026-11-21): without real-time playing the app is useless. This is the largest open defect and comes before every other item in this plan.** Play used to render two bars to a WAV ("Rendering…") and then play the file. The requirement is in the specification under [Transport](../specs/applications/Daw.md#transport-daw-transport).

- [ ] **Engine at 1.5× real time or better on the codex song ([DAW-TRANSPORT-LIVE-SPEED]).** Measured 2026-11-21: the live stream makes 0.37 s of audio per second (17.1 s in 45.8 s, one core, machine under load); the offline two-bar render makes 0.56. A 12 s profile of the live stream is mostly reference counting and allocation (`arc_drop_child`, `arc_release_zero_locked`, `osp_retain`, `osp_release`, `arc_alloc_locked`), then `osprey_list_get`. Owners: OspreyOpus1 (synth kernels, delay lines, effect state), ferroui-prism-lead (stream encoding, block loop, code generation). Until this is met the device runs empty and live sound breaks up.
- [x] **Measure cold playback against the actual 48 kHz output** ([DAW-TRANSPORT-LIVE-SPEED]). Run `make -C src/toolchain/targets/macos/tests daw-playback-rate`. On 2026-11-21 the two-beat codex excerpt rose from 1.15× to 1.60× after removing empty track buffers and keeping modal and frequency-shifter state. All 539,077 stereo frames matched by SHA-256. The first block still took 3.27 s.
- [x] **Repair the codex score's repeatable playback stop** ([DAW-TRANSPORT-LIVE-SPEED]). The keysR strike comb multiplied note pitch by 35.51 and refused a note after 958,464 frames. Restoring its authored pitch ratio of 1 let the renderer finish all 4,835,750 frames.
- [x] **Reuse and repair prepared notes** ([DAW-TRANSPORT-LIVE-SPEED]). Meter and mix edits now preserve the note cache. The maker regenerates a truncated tensor and publishes its replacement atomically. The renderer journey verified byte-for-byte recovery. Repairing 15 truncated files and preparing the changed patch brought the full 4,835,750-frame codex stream to **10.96×** (100.74 s of music in 9.19 s, first block 27.3 ms, worst following block 34.0 ms).
- [x] **Sustain 160-frame interactive packets on CoreAudio** ([DAW-TRANSPORT-LIVE-LATENCY]). Three seconds of prepared silence used to exhaust the queue 56 times. The device still starts on the second packet (320 frames) and now holds up to 1,440 frames (30 ms) while playing. The new native cadence test registered zero empty callbacks during continuous delivery and detected a deliberate 100 ms pause. In a full prepared codex run, the old device reported 1,912 empty callbacks before the final packet; the revised device reported zero through the final packet, then one at end of stream as it drained all 4,835,750 frames. The user's current Prism session stayed open.
- [x] **Reduce work inside cold patch notes** ([DAW-TRANSPORT-LIVE-SPEED]). A three-second sample of the dense Elise passage put 208 of 586 main-thread samples in the FFT and 83 in partial-bank stepping. Rendering FFT pairs directly and skipping silent partial amplitudes while advancing their phases brought that passage from 11.96 s to 10.74 s with identical PCM. Both synth test backends passed all 69 journeys. Dry note preparation now collects only the sink. The two-beat codex maker produced all eight identical cache files in 9.84 s versus 10.66 s with metering.
- [x] **Remove multi-second stalls at repeated patch-note attacks** ([DAW-TRANSPORT-LIVE-SPEED]). A cache miss ran the entire note on the 160-frame producer. The three-note spectral chord at beat 12.5 of Elise blocked one packet for 7.55 s. Commit `032907fc3` renders live misses by block, including during a device loop. The first 20,571 frames of Violet matched across playback and two loop passes.
- [x] **Profile simple synthesis separately from spectral processing** ([DAW-TRANSPORT-LIVE-SPEED]). A single sine through gain and output spent more sampled CPU in graph evaluation, node lookup, state publication and reference counting than in the oscillator. Removing state-list copies for stateless nodes in `94cad1300` raised throughput from 8.60–8.81× to 9.29–9.39× with identical PCM.
- [x] **Resolve scheduled nodes once per audio block** ([DAW-TRANSPORT-LIVE-SPEED]). With `60e099dae`, the 16-second sine/gain/output fixture used 1.75–1.77 seconds of user CPU, down from 1.99–2.03, with byte-identical output. All 69 synth tests passed under ARC and GC. CoreAudio drained all 721,263 Elise frames with zero empty callbacks twice cold and once warm. First device progress took 4.06 and 4.87 seconds cold and 90 ms warm. Whole-song deadlines remain open.
- [ ] **Eliminate residual cold-device underruns and shorten cold startup** ([DAW-TRANSPORT-LIVE-SPEED]). On 21 November 2026, the installed renderer delivered all 721,263 stereo frames of Elise beats 8–24. CoreAudio reported two empty callbacks near frame 687,520 after a 4.27 s cold start, and zero after an 89.5 ms warm start. Two isolated candidate runs reported zero cold underruns. The cold spectral head start in `a27555d29` is bounded.
- [ ] **Close the whole-song and spectral deadline gaps** ([DAW-TRANSPORT-LIVE-SPEED]). With `DAW_RATE_BEATS=161`, the cold codex score produced 100.74 s of audio in 227.64 s (0.44×); the longest inter-block gap was 2.95 s. Elise at beat 13.8919375 produced 156,937 frames in 12.12 s (0.27×). The 17.4× improvement in the earlier bowed-string onset does not cover this FFT-heavy passage.
- [ ] **Start latency of 5 to 10 ms ([DAW-TRANSPORT-LIVE-LATENCY]).** The device starts after 240 to 480 buffered frames. Interactive playback now flushes 160-frame packets and the device starts after two (320 frames, 6.67 ms of audio). Measure wall-clock time from Play to first sound, including project and device startup.
- [ ] **MIDI clock sync ([DAW-TRANSPORT-MIDI-CLOCK]).** Send timing clock, Start, Continue, Stop and Song Position Pointer from the audio sample clock; follow an external clock when external sync is on. Needs: MIDI port access in the host (CoreMIDI on macOS, Windows MIDI Services), the clock source and ports in `daw.td`, and scheduling against the device's played frames. Nothing is built.
- [x] The renderer's live mode: `daw <project.json> <folder> <from-beat> <beats> live|loop` prints each block as one hex line for the audio device (`render/stream.ospml`, `ArrangeMath.live`). A loop keeps each track's made notes across passes, so later passes run at about 1.6× real time (633e10d21).
- [x] Play streams the range into the audio device through one process group; Stop cancels it; a refusal shows as "Playback failed: …" (633e10d21). Verified outside the app: the codex range played through the device, 659,863 frames, with 49 empty-queue callbacks in the note-dense first 4.4 s.
- [x] Range ([DAW-TRANSPORT-RANGE]): Play runs from the start marker (the playhead's bar) to the end marker (the song's end by default); both markers are drawn on the lanes and dragged on the ruler; the end is always after the start. Built (633e10d21); the drag is not yet verified in a window.
- [x] Loop toggle ([DAW-TRANSPORT-LOOP]) in the transport: the range repeats until Stop (633e10d21).
- [x] Following ([DAW-TRANSPORT-FOLLOW]): the playhead, the lanes, the piano roll page and the position readout follow the frames the device has played. Built (633e10d21).
- [x] **Make notes ahead of the playhead.** 86% of live time is making whole notes when they start (`Arrangements::advanced` → `NoteRenders::render`); mixing, buses and the Space are fast enough. Make notes before they are due, in parallel worker processes, and keep them in a note cache on disk that Play reads, so a first pass is as fast as a looped one. Needs a cheap sample file reader (the WAV import decodes byte by byte through `Result` folds). Built: `render/note_cache.ospml` (`daw … warm <maker> <makers>`), started at song open and with every Play; live play reads kept notes without starting a process, which also keeps the heap unlocked (the lock cost 28%). Open: makers after each edit, a journey test that runs the makers, and the cold-song wait.
- [ ] The hex text between the renderer and the device costs encoding time on every block; a binary block needs a byte-writing output in the language or the runtime.
- [x] Interactive launch stays open until the user closes it. Commit `5c0357a74` removed the default ten-minute timeout.
- [x] Reduce ARC list allocation in the playback path. In `3b69dd0e8`, a live render of the committed 17-track codex project from beat 0 for two beats, including its tail, produced 851,077 stereo frames at 48 kHz. Across three alternating runs per build, median wall time fell from 19.49 s to 18.20 s. All 208 output blocks were byte-identical. This excerpt reached 0.97× real time. The 1.5× live target above and the whole-project offline targets under [DAW-PERF-RENDER] remain open.
- [ ] Project edits must remain playable when a patch branch is disconnected [DAW-PATCH-HEARD]. The edited codex `sub` track with its mixer-to-gain cable removed refused playback before the first block and reported an unsupported graph. Reproduce in a disposable project, verify silence on the disconnected path and continued audio from the other tracks, then reconnect and verify the bass returns.

## Where the work stands at the 15:30 shutdown, 2026-11-21

Everything below is committed on `feat/consolidated`; nothing is pushed. `make -C src/toolchain/targets/macos daw` builds the renderer and the app from this tree (both checked at `2119e1f09`).

In the code, not yet seen working by the user (rebuild with `make daw` first):

- Play reads notes made ahead and kept on disk (`render/note_cache.ospml`); the makers are limited, low priority and end with their Play (`122da5982`).
- The start and end markers drag independently (`1ab47ad22`); a held Gain or Pan fader follows the pointer (`a7ad62cae`); the piano roll's keys sound through the track's patch (`5a9f73628`).
- Resizing the window no longer resizes controls (`209fb9aef`): pages draw at one point per unit. The pages do not yet fill a larger window, and a window smaller than the page cuts it off without scrolling.
- The lanes scroll one row at a time by their scrollbar (`961618056`).
- The song root is title, tempo, sampleRate, buses and tracks; the Connections list is gone from the patch inspector.

Do first next session, in this order:

1. Hear a cold Play of each song with the user. Makers started by the old build may still be running after a restart: `pkill -f "daw .* warm"` ends them. If a cold song still starts rough, make the stream wait for the first second of notes instead of making them itself, and warm after each edit.
2. [DAW-LAYOUT-FILL]: pages lay out to the window (lanes and roll stretch, inspector pinned right), lists scroll row by row (issue 33), the patch canvas pans and zooms (issue 35).
3. Track scope and frequency verdict on the Timeline (issue 25).
4. The daw test suite still cannot be type checked in one piece; the tests were edited by hand and by a pi agent for the deleted root properties and have **not been run**. `test/history.ospml` holds eight `TODO-MODEL` lines whose expected numbers depended on the Space or drive and must be measured again. The transport journey, with the new marker and fader checks, was run alone in a scratch project and passes (`bdbe5ef89`). The roll keys are covered too (run the same way, passing). Journey tests are still owed for the lane scrollbar and the note makers.
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
| 6 | The app shut down during the test | [DAW-TRANSPORT-LIVE] | Fixed (5c0357a74): the launcher's ten-minute limit is gone |
| 7 | Real-time play needs only 5 to 10 ms of buffering before it starts | [DAW-TRANSPORT-LIVE-LATENCY] | Open: today 171 ms of blocks plus one buffered block |
| 8 | Playing must sync with the MIDI clock | [DAW-TRANSPORT-MIDI-CLOCK] | Open: nothing built |
| 9 | A patch must render nicely by default, like the design, not as a grid of oversized cards with cables over them | [PRISM-PATCH-LAYOUT] | Open. Owner: prism-routing-design |
| 10 | Rearranged patch nodes must be saved to the project and reload as arranged | [PRISM-PATCH-LAYOUT] | Fixed in `fddb86f2d`. A native drag wrote the new card position on release and a fresh load restored it. Detached-window coverage remains open |
| 11 | Dragging the sliders is very clunky, with a huge delay | [DAW-MIX-DRAG] | Open: the value applies only on release, and each commit re-encodes and writes the project |
| 12 | Play failed: "sub: Reference rendering requires mono audio, one output and supported bounded processors." | [DAW-PATCH-HEARD] | Fixed (633e10d21): the Gain module's input in the sub patch had no cable; an unplugged input now hears silence and the song plays. Open: why the Mixer→Gain cable was lost while the user arranged the patch |
| 13 | Editing a built-in instrument silently swaps its sound and drops settings | "Built-in instruments become patches" | Open |
| 14 | The routing page must use the real patch nodes and cables, not its own drawn cards | [PRISM-ROUTING-EDITOR] | Open. Owner: prism-routing-design |
| 15 | Dials against the design: parameters after the tenth are dropped, readings are always hundredths, fine drag has no Shift cue, no arrow keys or focus, no double-press to default | Patch Controls | Open |
| 16 | In a tall window the pages do not fill it: the song pages sit in the top third with empty space below, and nothing scrolls or pans to use the room | [DAW-LAYOUT-FILL] | Open |
| 17 | On the patch page a module cannot be dragged lower than about the first quarter of the canvas | [DAW-LAYOUT-FILL] | Fixed in `6dc912692`. The native drag test reaches y = 1568.5 in a tall viewport and checks that the output socket follows the card |
| 18 | Stretching the window sideways changes the zoom: everything grows or shrinks with the window's width | [DAW-LAYOUT-FILL] | Open |
| 19 | Open track patches in a separate window and close them while the song keeps playing | [DAW-PAGES-PATCH-WINDOW] | Open. Verify both window close and the focused Close command from Timeline and Bus Routing |
| 20 | Quit in the application menu leaves Prism running | [DAW-PAGES-PATCH-WINDOW] | Fixed on macOS in `0af119f64`. The native Quit command exited a disposable instance with status zero |
| 21 | Play asks for a song even though a project is already open | [DAW-TRANSPORT-REFUSAL] | Fixed (`757b36f2d`), test first: with a song open and no renderer, Play and Render say Prism was launched without its song renderer; with no song they say to open one; while playing they say to stop first. The project is kept. The transport journey, run alone, was red before and passes after. [Issue 425](https://github.com/Nimblesite/osprey/issues/425) stays open until the user sees it in a release build |
| 22 | Stop appears active when the transport is idle | [DAW-TRANSPORT-STOP-IDLE] | Fixed. The song and patch Stop controls dim and lose their hit targets when idle. |
| 23 | A track needs as many internal routes and sends as the project requires | [DAW-GRAPH-SUBGRAPH] | Open. Exercise branching, merging and routing into several buses using the shared node editor. Save and reopen the full graph and timeline from one project JSON file |
| 24 | The example music sounds too simple | [DAW-DEMO-SOUND] | Open. Hand-author the Silt Relay arrangement and patches, exercise the synth module palette, render the master and listen through its section changes. The team is revisiting the other demo songs too Violet Static is finished in `0f67e51a0`. Its 48 kHz stereo master runs 55.657 seconds at −10.9 LUFS with a −0.9 dB true peak. |
| 25 | The Scope is useless inside the patch. The user must see a track's sound from the main view, perhaps not as a node at all but as a panel shown when a track or channel is selected, and it must say whether the sound hits the right frequencies for what it is | [DAW-ANALYSIS-TRACK] | Open |
| 26 | The song root holds properties that do not belong there: `intro`, `sustains`, `drive`, `space`. Drive is a saturator on the master channel, not a root property. Keep `title`, `tempo`, `sampleRate` | [DAW-PROJECT] | Done in the model, engine and UI: the root is title, tempo, sampleRate, buses and tracks; the track's Space send went with the Space. Open: the tests are being updated and cannot be run (suite type check), the song files still carry the old keys (ignored on load, dropped on the next save; their authors are rewriting them), and the screenshots' goldens need refreshing |
| 27 | `make daw` failed to build twice during the test | [DAW-TESTS] | Fixed. Causes: a deletion of mine caught half-applied, and a synth change to typed faults that three agents then patched in the same daw files at once. Rule now: a TMC file lock before every edit |
| 28 | The start and end markers must move independently | [DAW-TRANSPORT-RANGE] | Fixed in the app code: either marker is taken by its handle or anywhere down its line, in the ruler or the lanes, and drags alone; the start stops one bar before the end and the end one bar after the start. Before, only the end had a handle (a 14-unit band on the ruler) and a press anywhere else moved the start. Not yet seen by the user; journey test owed |
| 29 | Tapping a key on the piano roll's keyboard does nothing. It must play that note through the track's patch | [DAW-ROLL-KEYS] | Fixed in the app code (5a9f73628): a press on a key sounds that pitch through the selected track's patch. Not yet heard by the user; journey test owed |
| 30 | Delete the Connections list in the patch inspector: the cables already show it | [DAW-INSPECTOR] | Done: the list and its heading are gone; the tests expect no connection rows |
| 31 | The app shows a generic icon in the Dock | [DAW-MACOS-ICON] | Fixed in `19a98d9f7` (prism-routing-design): the Prism icon is restored on the real launch path and in Finder; the macOS packaging test passes |
| 32 | The patch inspector's knobs are too small and its rows too tight | [DAW-INSPECTOR] | Fixed in `1ca824b80`. Both inspectors use a 32-point dial and 52-point rows. The native journey verified taps and editing, and the [Waveshaper capture](../designs/prism-timeline/native-review/waveshaper.webp) shows the result. |
| 33 | The lanes' scrollbar has no fine control: it is either at the top or at the bottom. It must scroll smoothly, row by row, up and down | [DAW-LAYOUT-FILL] | Fixed in the app code (`961618056`): the lanes scroll by one row, the thumb follows the pointer and its height shows how much is in view. Open: the wheel (the host sends no wheel events yet); not yet seen by the user; journey test owed |
| 34 | **Hard rule:** resizing the window must never resize the controls. Resizing only frees up canvas space or reduces it | [DAW-LAYOUT-FIXED-SCALE] | Fixed in the host and app code (`209fb9aef`): pages draw at exactly one point per unit at every window size. The user's last screenshot (15:02, a small window with a shrunken patch) was a build from before this change: it still showed the Connections list deleted earlier. Must be confirmed in a rebuilt app at small, wide and tall window sizes. Open: a window smaller than the page cuts it off without scrolling |
| 35 | The patch screens need zoom and pan | [DAW-LAYOUT-FILL] | Open. Zoom is the user's own setting, never the window's size |
| 36 | Play takes too long to start, then plays a few bars and stops | [DAW-TRANSPORT-LIVE-SPEED] | Cause found in the running system: every song opened and every Play started twelve note makers that were never ended, so 30 and more of them ran at once (load average 258) and starved the stream. Fixed in the app code: one maker for every three cores at the lowest priority, the makers of a Play end with it, and opening another song or pressing Play ends the song's earlier makers. Open: a song whose notes are not made yet still starts rough; not yet heard by the user |
| 37 | Parameter names overlap the dial and the value in the patch inspector ("Transform (0 pass, 1 freeze, …") | [DAW-INSPECTOR] | Fixed in `92ab7207b`. The [Spectral inspector](../designs/prism-timeline/native-review/spectral-controls.webp) shows the current transform by name. The native journey selected freeze from all seven named options and checked the saved value in the project. |
| 38 | Found by the lead while fixing 28, not yet reported by the user: selecting a track moves the start marker to that track's first bar, because one field (`rollBar`) is both the piano roll's bar and Play's start | [DAW-TRANSPORT-RANGE] | Open. Give Play's start its own field so only the user moves it; 18 test lines read `rollBar` |
| 39 | The four wavetable modulation inputs shared one socket in the track patch | [PRISM-ROUTING-CABLES] | Fixed. Multi-input control sockets sit 34 points apart with independent 28-point hit targets. The native journey wires a single LFO to position, amplitude, pitch and phase, saves and reopens the project, moves the wavetable and reopens it again. It verifies all four cable destinations from the project JSON and the restored position of every inlet. The former build lost the cable to inlet 1. |

Found while fixing these:

- [x] Restore the integrated DAW build after the synth error-type changes [DAW-TESTS]. On 21 November the reported ARC renderer command and the full macOS DAW archive both built. The renderer wrote 16,500 stereo frames for a project with a pre-fader send. A missing source failed without writing a WAV. The CLI regression in `cbefa7764` also checks the exact instrument error on both offline and live playback.
- [x] Both native event journeys passed against the integrated macOS DAW archive on 21 November (9c33cf379). The separate root `make test` run reached Rust coverage compilation before its four-minute timeout.

- [x] [DAW-TRANSPORT-REFUSAL] Exercise Play with no project, an open project and no renderer, and playback already in progress. Each refusal must identify the condition and preserve the current project.
- [ ] The daw test suite cannot be run: type checking `src/osprey/examples/daw/test` did not finish in 55 minutes and held 5.6 GB (effect-row convergence, owner OspreyOpus1). The live transport and unplugged-input journeys were run alone in a scratch project with the same sources (both pass, and both fail when their expectations are changed).
- [ ] Find how the sub patch lost its Mixer→Gain cable during the user's drag, and add the regression.
- [ ] Toggle Loop and move the end marker while the song plays (today both say "Stop first").
- [ ] A play that hits a track which cannot render names the track and module in plain words.

## Pages fill the window; the canvas pans [DAW-LAYOUT-FILL]

Reported by the user on 2026-11-21 with two screenshots of a tall, narrow window: "The UI is totally fucked. Nothing is scrollable so we can fill the empty space with more canvas anywhere. I can't even drag the modules lower than about the first quarter of the vertical space."

- [ ] **Hard rule ([DAW-LAYOUT-FIXED-SCALE]): resizing the window never resizes the controls.** Stated by the user on 2026-11-21: "resizing the window MUST NOT RESIZE THE CONTROLS. That's a hard rule. Resizing ONLY frees up canvas space or reduces it, that's all." Every control, label, card and cable keeps its size at every window size; a larger window shows more canvas, more lanes and more rows, a smaller one shows fewer and scrolls. The patch canvas has its own zoom and pan, set by the user, and the window's size never changes them.
- [ ] **Lanes scroll row by row.** Done for the scrollbar (`961618056`). Open: the wheel moves one row at a time.
- [ ] **Resizing the window never changes the zoom.** Reported by the user as "a huge bug": stretching the window horizontally scales every card, label and control. Cause: each page is one fixed scene (1440 units wide) scaled to fit the region, so the scale is the window's width divided by 1440. Required: one scene unit is one point at 100% zoom on every page; a larger window shows more canvas, lanes and rows at the same size, a smaller one shows less and scrolls; only the user's zoom commands change the scale. Needs the scene drawn unscaled by the host (or the region's size in points given to the app so it lays out to it), and every page laid out from the region's width and height instead of 1440 × 940.
- [ ] **Song pages use the whole window.** The Timeline and Bus routing pages are drawn on a fixed 1440 × 940 scene scaled to the window's width, so a tall window shows the page in its top third and a dead area below (`Header::shift` centres only sideways; `TimelineGeometry.centred`). The page must lay out to the region's real width and height: more track lanes, a taller piano roll, a taller routing table and signal flow.
  - Inventory for that change (counted 2026-11-21): the Timeline page fixes its width in `timeline_view.ospml` through the lanes' right edge (1092, 1098, 1113: 5 uses), the inspector's left edge and column (1132, 1154, 1416: 22 uses) and its rows through 438, 542 and 802 (7 uses); `header_view.ospml` and `TimelineGeometry.lean` hold the scene's 1440 by 940 (10 uses) and `mix_view.ospml` one 542. Replace these with values measured from the viewport (`viewWidth`, `viewHeight`) in the Lean geometry first, so drawing and hit testing change together.
- [x] **A module drags across the resized patch canvas.** The native regression in `6dc912692` checks the oscillator and its output socket after a drag at 1536 × 828, 1536 × 1850 and 2260 × 828. The tall case previously stopped at y = 700.
- [ ] **The patch canvas pans and zooms.** Drag on empty canvas, or two-finger scroll, pans; pinch or Cmd-wheel zooms; the canvas is larger than the window and Zoom to Fit shows the whole patch. Needs the wheel and pinch events the host does not send yet (also needed by [DAW-TIMELINE-ZOOM]).
- [ ] **Lists scroll.** The track list, the lanes and the routing table scroll with the wheel when they hold more rows than fit.
- [ ] A journey test at three window shapes (wide, square, tall) asserts that each page's body reaches the bottom of its region and that a module dragged to the bottom edge lands there.

## Slider drag lag [DAW-MIX-DRAG]

- [ ] **Hear each knob move while the song plays [DAW-TRANSPORT-LIVE-CONTROLS].** During a drag, the dial, its displayed value and the playing track must follow the same accepted parameter Signal. Verify the next audio block of a held note, subsequent notes, mixer and bus controls, Undo and a looping song. Measure the time to audible output while dragging on the codex song. The original held-note regression measured a peak of 0.353518 on both channels after gain was set to zero.
  - [x] Dragging the Timeline track gain or Bus routing group gain fader to zero silences both channels before release, including a new note starting inside the block. Undo restores audible output on the next block and Redo silences it again on the same transport clock. The native journey passed under GC and ARC.
  - [x] An existing send previews level changes on a sounding note. Invalid text holds the last valid mix, and Cancel restores the saved send on the next block. The native journey checks stereo output and the saved project.
  - [x] A 960-frame loop crossed its boundary with group gain at zero. The native renderer produced four consecutive silent 160-frame packets and restored audible output on the packet after gain returned to one.
  - [x] Patch Gain stays muted across a loop boundary. Setting it to zero after packet five of a six-packet loop silenced packets six through nine. Restoring 0.5 brought both channels back on packet ten. A send connected before the gain node continued to sound, as routed.
  - [x] An overlapping tablepad note at beat 64.2046 now reuses its sounding sibling’s prepared wavetable. After dragging Gain, rendering its 160-frame block took 1.12–1.26 ms across six runs, down from 25.65 ms against a 3.33 ms deadline. The same 10,240 stereo frames with two gain edits were byte-identical. With the shipping nine-buffer queue, the hardware return took 31.72–31.77 ms in two runs with zero empty callbacks. The window adapter’s separate drag dispatch delay is tracked in [Osprey #426](https://github.com/Nimblesite/osprey/issues/426).
  - [x] Five native editor pointer drags on the codex tablepad at beat 64 changed the next available audio after 2.65–2.97 ms. Each drag moved Gain from 0.8 to zero. The first changed packet was 31, compared sample by sample against unedited playback. CoreAudio reported that packet returned 47.91–49.48 ms after the pointer stimulus, with zero empty callbacks over 10,240 frames per run. The measurement starts at the headless pointer driver and ends at the audio queue callback. Acoustic latency is still open.
  - [x] A held note reaches exact silence in both channels on the next block and resumes when gain is raised. Retuning it preserves oscillator phase under GC and ARC.
  - [x] On the 17-track codex project at beat 64, changing tablepad Gain from 0.8 to zero changed the next 160-frame packet against identical unedited playback. Reusing the active note’s wavetable reduced the time from publication to the changed packet from 27.7 ms to 3.6 ms. CoreAudio returned the packet after 10.8 ms (previously 35.0 ms). The device reported 17 empty callbacks during 10,240 frames. A repeat at 19:06, with other builds and tests running, changed packet 26 immediately after publication following packet 25. The changed packet took 10.1 ms, CoreAudio returned it in 20.0 ms, and 12 callbacks were empty. Pointer-to-speaker latency remains unmeasured.

Reported by the user on 2026-11-21: dragging the Track Inspector's Gain and Pan sliders is very clunky, with a huge delay.

- [x] The Timeline inspector and Mix table preview gain during each pointer move. Release writes the project and creates one undo step. The native journey checks all 17 Mix gain faders, Undo, Redo and a Timeline drag with 16,384 offscreen notes (c0e67f73d, 9c33cf379).
- [ ] **A commit does not stall the interface.** Each commit encodes the whole project, decodes it again to check it, writes the file, and rebuilds the lane traces and overview. Measure the release-to-repaint time on the codex song; target under 16 ms, with the file write off the interface thread or deferred. On the 128-bar fixture, a move took 0.83 ms on average and release with project serialization and saving took 50.27 ms.
- [ ] The same check for every other slider and dial: Space send, bus faders, the Space decay, patch dials.
- [x] The native journey checks the displayed gain at 24 consecutive pointer positions on a 128-bar arrangement, then reads the released value from the project (9c33cf379).

## Patch layout [PRISM-PATCH-LAYOUT]

- [ ] **Grain cloud sample path overflows its card** [PRISM-PATCH-LAYOUT-SAMPLE-LABEL]. In the reported screenshot, Grain cloud 0 displays `src/osprey/examples/daw/tracks/samples/elise-motif.wav` across the orange card border. Display the filename inside the waveform panel, truncate long filenames to its available width, and expose the complete path in the inspector or an accessible description. Verify with a narrow window and a long filename, keeping the output socket clear.

Reported by the user on 2026-11-21 with two screenshots: a track's patch opened as a tight grid of oversized cards with cables crossing over the cards, where the design shows cards at their natural size, spaced along the signal path from left to right with cables between them.

- [ ] A patch with no saved positions is laid out along its signal flow: sources on the left, the Output on the right, one column per step, branches stacked, no card over another and no cable through a card. Owner: prism-routing-design.
- [ ] Cards keep their designed size in a patch opened from a track (the generic cards in the report are larger than the designed Oscillator, Master and Scope cards).
- [x] Releasing a dragged node saves its position in the unified project JSON [DAW-PROJECT]. The native test in `fddb86f2d` checks the file during the drag, immediately on release and after a fresh load. Only the dragged card’s coordinates change.
- [ ] Repeat the card and socket restoration check in an independently closable patch window under [DAW-PAGES-PATCH-WINDOW].

## Patch windows and routing interactions [DAW-PAGES-PATCH-WINDOW]

- [ ] **Open a track’s patch in its own window.** Opening it from Timeline or Bus Routing leaves the project window visible at its current track, selection and playhead. The patch window edits the same project. Verify during playback that closing it with the window close control or the focused window’s Close command leaves the project open and transport running. Reopen the patch and check its saved parameters, cables and node positions.
- [ ] **Use the patch node editor for Bus Routing** [DAW-GRAPH-SUBGRAPH]. Replace the separately drawn cards, ports, wires and hit areas in `MixView.flow` with the patch editor bound to the project graph. Exercise the reported `glow` route through Stereo pan, pads, Space and Master. Select each node, adjust Space decay and group gain, drag a cable, fan out an outlet, undo, redo and reopen. Verify the displayed values, saved routing and resulting audio agree.
- [x] **Open the track patch from its signal-flow card.** A tap on the patch output now opens the selected track in the existing patch editor. The native journey opens tablepad this way, selects its wavetable and edits its Level dial. The other bus nodes still need the shared node editor.
- [x] **Type a value from a patch canvas dial.** Tapping Level opens its numeric field. An out-of-range value keeps the field open for correction. The native journey enters 0.375, saves the project and verifies that Undo restores the original JSON.
- [x] **Make every parameter reachable and readable** [DAW-CONTROLS-DIAL]. Both inspectors page through all parameters. The native journey covers 10, 11 and 21 parameters, reaches the last grain control, edits Pan spread, displays 1.808× Drive and the Shift fine-adjustment cue, and checks the Curve selector and the antialiasing switch. Keyboard focus, arrow steps, default reset and card controls are tracked under [DAW-HUMAN].
- [x] **Quit exits Prism on macOS.** In `0af119f64` the host registers a desktop application lifetime. A native accessibility press on the Quit menu item previously left the process running after five seconds. The same press on the fixed disposable instance closed it with exit status zero.

### Regressions from the native review [DAW-PAGES-REGRESSIONS]

The following four host-free journeys failed independently. Keep each open until its existing assertions pass alongside the native interaction tests. [Screenshots and review](../designs/prism-timeline/native-review/README.md)

- [ ] Verify the drawn sockets and cable hit areas select, connect and disconnect the intended patch ports.
- [ ] Verify that changing module selection shows only that module’s controls and preserves its routing.
- [ ] Restore the VST discovery journey that inserts independent plug-in nodes through native controls.
- [ ] Restore the device journey that checks pitch, level and filter response and displays playback completion or device failure.
- [x] **TAL-NoiseMaker insertion and playback [PRISM-ROUTING-VST].** Prism now accepts a controller that explicitly declines processor-state transfer and keeps MIDI-only controller mappings out of the patch parameter list. The macOS test discovers the installed instrument, inserts it, verifies the Filter Cutoff control, edits Master Volume and sends MIDI through its patch. With silent stereo input, raising MIDI pitch from 60 to 72 moves the strongest measured output frequency from 65 Hz to 131 Hz. Both auditions deliver all 12,288 stereo frames to the device byte for byte.
- [x] **Downloaded effect and continuous playback [PRISM-ROUTING-VST].** Tested Dragonfly Room Reverb from the upstream universal macOS 3.2.10 release. Prism discovered it in the configured folder and routed a 440 Hz oscillator through it. At 48 kHz, a Hann-windowed spectral measurement showed a 440 Hz amplitude of 0.99988 dry and 0.06893 wet, with digital silence on both channels when muted. Continuous Play delivered 1,968,128 frames (41.0 s) with zero empty device callbacks before Stop. The repeatable target also checks uninterrupted device delivery. [Test instructions](../../src/toolchain/hosts/synth_vst/README.md#third-party-spectrum-test-on-macos)
- [x] **Two VST3s in one patch [PRISM-ROUTING-VST].** The native patch test connected a MIDI note generator to TAL-NoiseMaker, routed its stereo output through Dragonfly Room Reverb and sent that return to the stereo output. The one-second note produced a 131 Hz peak. From 2.0 to 2.5 s the dry signal measured 0.000000129 RMS and the reverb return measured 0.008798 RMS on the left channel. The 48 kHz stereo WAV contains all 235,520 frames sent to the audio device. The test verified the audio bytes at both cable connections. This exercised Play Patch and recorded its rendered output. File → Render Song is a separate renderer.
- [ ] Verify the patch popup, focused-window Close, application Quit, keyboard focus return and screen-reader announcements on macOS and Windows. The screenshot review covered pointer interaction on macOS.
- [ ] Verify spectral, shifter and reverb state after the generic fold fix. The synth owner reported float samples being interpreted as integers when a generic function is passed directly to `foldList`. Check fractional samples under GC and ARC and the rendered audio against a known-good reference. [Compiler regression](../regressions.md)

## Math is Lean [DAW-MATH]

Every formula in the daw is a `def` in a `.lean` file, and Osprey code for it is generated from that file ([MATH-CODE](../specs/tooling/MathToCode.md)). This covers timing, envelopes, mix ranges, pan laws, the excerpt window, and timeline and piano-roll geometry. No arithmetic formula is written by hand in `.ospml`. The generated modules are never committed. A Lean construct the reader cannot handle is fixed by extending the reader, not by writing Osprey math.

- [x] `ArrangeMath.lean`: beat and second frames, envelopes, pan, the limiter, `mixable`, `inWindow` and `shifted`. `Projects` and `Arrangements` call them.
- [x] `TimelineGeometry.lean`: bars, `barStart`, and lane, roll and mix placement. The transport and timeline call `barStart`.
- [ ] Find the remaining hand-written arithmetic in `src/osprey/examples/daw/src/*.ospml` (pixel offsets, meters, percentages) and move it to Lean.

## Project file [DAW-PROJECT]

- [x] `SongFiles::encode` writes every key `decode` reads; a loaded test checks that the project round-trips, including a routed patch's bindings and cables.
- [x] `Projects::mixed`, `replaced` and `saved` reject out-of-range gain, pan and send, missing tracks and quoted paths. Save checks that the encoded text decodes before it writes.
- [x] An encoding failure is an error result instead of a `"null"` file.
- [x] A Prism patch edit is saved into its track and reopens as the same graph ([DAW-PROJECT-PATCH]).
- [x] The project JSON keys are the `daw.td` field names, and `tracks/*.json` is migrated ([DAW-PROJECT-MODEL]).
- [x] Generate the project and editor types and the project JSON field readers and writer from one `daw.td` with `make -C src/toolchain/targets/tests daw-models TYPEDIAGRAM_DIST=<dist>`.
- [ ] Node positions, grain sample assets and VST bindings are stored in the project.
- [ ] Track and Master channel layouts (stereo default, any count) are stored in the project; tracks render per channel ([SYNTH-GRAPH-CHANNELS]).
- [ ] Built-in instruments become patches ([DAW-PROJECT-PATCH], adversarial review 21 Nov). A built-in track (Pluck, Bow, Bell, Mallet, Operator, Drum) renders through its own DSP, while Prism shows the nearest single node with a fixed 5 ms / 0.2 s gate. The first edit saves that approximation as a `PatchInstrument` with no warning, dropping the instrument's envelope and the settings no node holds (Drum `sweep`, `noise`, `decay`; Bow `vibrato`; Operator `attack`, `release`, `bloom`, `detune`). Fix: express every built-in as a patch of synth modules that renders the same samples (a test compares the two renders), migrate `tracks/*.json`, then delete the built-in instrument variants and their renderers.

## Transport [DAW-TRANSPORT]

- [x] The renderer's excerpt mode (`<from-beat> <beats>`) renders the window without the intro.
- [x] Play renders the current snapshot into a per-bar folder, plays only real audio, replays an unchanged snapshot and renders again after an edit.
- [x] Tickets are bound to the launched folder, so moving the piano roll during a render cannot play the wrong bars.
- [x] The transport journey test (`the transport renders two bars …`) passes. The daw suite passes 22/22 under ARC.
- [x] Natively verified: Play → renderer → afplay → finished, on the codex project (17.7 s stereo, peak −1.0 dB).
- [x] **Play bars** names the two-bar transport. **Save project** writes the current patch without leaving its page.
- [x] Stop cancels the FerroUI transport process group. Native Play → Stop left no renderer and a zero-byte WAV. The host process tests also exercise descendant cancellation.
- [ ] The renderer is faster than real time: see [DAW-TRANSPORT-LIVE] at the top of this plan.
- [x] **Stop is disabled when nothing plays ([DAW-TRANSPORT-STOP-IDLE]).** Stop preserves the playhead and selection while idle. The GC and ARC transport journey checks the hit target during playback, rendering and patch startup, and after cancellation and natural completion.

## One graph [DAW-GRAPH]

**The project is one big graph of cables, and a patch is a subgraph of it.** Every item below builds on that: tracks, buses, Space and Master are nodes; a track's patch is the graph nested inside its node, and its Output is the track's output. Play and render run the whole graph in one block loop into Master. No work item may add a per-track render path, a separate mix stage or a second routing model.

- [ ] One block loop runs every track, bus, send, pad and the Space into Master ([DAW-RENDER-PLAY]). `daw::Playback` (in progress, OspreyOpus1): the render and the journey test run on it. Patch modules still render a whole note when it starts, inside its track node. Running them in the same block loop is the next step ([DAW-GRAPH-SUBGRAPH]).
- [ ] A patch's audio and MIDI inputs take cables from other nodes in the project graph ([DAW-MODULAR]).

## Rendering the song [DAW-RENDER]

- [x] **Render song** in the header bounces the whole project to `<song>/song/song.wav`: refused while busy, only real audio counts, an unchanged project is kept, Stop cancels it. The transport journey test covers it; the daw suite passes 22/22.
- [ ] Natively verified on the FerroUI host with the codex project.

## Timeline transport [DAW-TRANSPORT-TIMELINE]

- [x] Play bars and Stop from the Timeline.
- [x] Playhead at bar resolution (`Editor.rollBar` in daw.td): a line down the lanes with its handle on the ruler and its position shown; tapping the ruler moves it and keeps the track. Tested in the transport journey.
- [x] Play bars starts at the playhead. Stop halts. Return to Start moves the playhead to bar 1.
- [ ] Playhead in beats, saved in the project file.
- [ ] Range markers, Loop and the following playhead: see [DAW-TRANSPORT-LIVE] at the top of this plan.

## Dynamics [DAW-DYNAMICS]

- [x] Compressor and limiter formulas in [DynamicsMath.lean](../../src/osprey/packages/synth/src/DynamicsMath.lean); the Osprey module generates.
- [ ] `Compressor` and `Limiter` components in `@osprey/synth`, any channel count, linked (synth-advanced). Partial: `synth::Limiters` (4e212e09f) limits a linked stereo buffer: the lowest true-peak need (4× oversampled, ITU-R BS.1770-4) is held across the lookahead with a linear-time block minimum, released upward and averaged over the window, so no frame or inter-sample peak passes the ceiling before the final clamp; quiet signals pass bit-exact. The synth test covers 4 settings × 6 signals, impulses, a quarter-rate 45° sine (samples at 0.707, true peak 1) and 9 refusals. Not yet a graph node or any channel count; no compressor component.
- [ ] Tests: sine against the curve within 0.01 dB; inter-sample peaks under the ceiling (synth-advanced).
- [ ] Master ends in the compressor and limiter, replacing the `ArrangeMath` limiter; settings in daw.td. Partial: Master ends in the true-peak brick-wall limiter at −1 dBTP (1.5 ms lookahead, 80 ms release) and `drive` pushes the peak-normalised mix into it; the tanh limiter is deleted (1b45a3ba0). The compressor stage and limiter settings in daw.td remain.
- [ ] Gain reduction and true-peak meters on the Mix page.
- [ ] Multiband compressor and multiband limiter ([DAW-DYNAMICS-MULTIBAND]): Linkwitz–Riley crossovers (3–5 bands) that sum flat, per-band dynamics; Master chain is EQ → multiband compressor → multiband limiter → true-peak brickwall; also patch and bus effects; settings in daw.td; Lean formulas; flat-sum and per-band tests.
- [ ] Pitch effects on tracks and buses ([DAW-PITCH-EFFECTS]): the FFT pitch shifter in semitones and cents, and autotune following key/scale or the timeline's notes, each with latency compensation in the renderer.

## Buses and sidechain [DAW-MIX-BUS]

- [x] daw.td `Bus` (EQ, compressor, sidechain) and `Track.bus`; codec generated; both projects migrated.
- [x] The renderer sums routed tracks into their bus, then EQ, compressor (keyed by the sidechain track) and gain into Master.
- [x] Bus journey test: 0 dB EQ passes the input, the sidechain ducks by exactly −15 dB (20 dB over, ratio 4), self-keyed and boosted buses, every refusal, codex round trip.
- [x] Codex: `glow` and `tablepad` on a `pads` bus ducked by `kick`.
- [ ] Bus strips on the Mix page: EQ and compressor controls, gain reduction meter.
- [ ] Natively heard on the full codex render.

## Best-of-breed EQ [DAW-MIX-BUS-EQ]

The three-band split is a start, not the EQ. Target: a parametric EQ as good as the best commercial ones, on every track, bus and Master.

- [ ] Up to 8 bands: bell, low and high shelf, notch, band-pass, tilt, and high- and low-pass at 6–48 dB/oct. Biquads from the RBJ cookbook, written in Lean; each band has frequency, gain and Q.
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
- [ ] Lean formulas and tests on synthetic signals with known spectra.
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

- [x] Timeline, Mix and Patch pages on FerroUI ([screens](../../src/osprey/examples/daw/screens)).
- [ ] Screenshot goldens compared automatically in the app test targets.
- [x] Native video journey ([DAW-VIDEO]): 23 clicks across five voice types, Mix and Play bars; each click changes the screen; Play rendered 6.8 MB of bars audio in 26 s.
- [ ] Fix: a render cancelled mid-way leaves a 0-byte `piece.wav` that the next Play replays as unchanged (found by [DAW-VIDEO]).
- [ ] Fix: Patch → Timeline after Audition ignored the click in 1 of 3 native runs (found by [DAW-VIDEO]).
- [ ] Audition audio in the video soundtrack (the host writes what it plays).
- [x] Piano-roll note selection and velocity edits are saved into the project.
- [x] Native bar-9 journey on a copy of the codex project. Selected keysL, changed gain, pan and Space, inserted a delay, saved, reopened and played 13.75 seconds of stereo at 48 kHz.

### Menu bar and header [DAW-MENU] [DAW-HEADER]

- [x] The macOS menu bar holds the commands: File (Open Song ⌘O, Save ⌘S, Render Song ⌘R), Edit (Undo, Redo), View (pages ⌘1–3, Modules, Patch Controls, Keyboard) and Transport. The FerroUI host builds it from `Ui::section`s among the root's context-menu actions; read back from the running app and driven with View → Timeline.
- [x] One header on Patch, Timeline and Mix, drawn after [piano-roll.png](../designs/prism-timeline/piano-roll.png): bordered tabs with icons, song strip (BPM, meter, position, what is sounding), Play/Stop icon group. Save, Open, Render, Keyboard, Audition and Patch Controls are no longer canvas buttons.
- [x] **Song timing [DAW-HEADER-TIMING].** BPM and Meter in the header open the timing controls. The native macOS journey saves 96.5 BPM in 6/8 and undoes the edit.
- [x] **Patch controls [DAW-INSPECTOR].** File → Settings stores the VST3 folder. The Add component menu includes plug-ins discovered in the background. Right-clicking a node offers Delete node. The native test deletes a different node from the current selection. Routing stays on the canvas.
- [x] Pages fill the window (zoom 0) unless the keyboard is docked.
- [x] The native application menu reads Prism. The FerroUI host sets the application name before the platform exports its menus.
- [x] Match the reference’s 1440 × 940 canvas. The header ends at y=59, the song strip ends at y=142, seven full-height arrangement lanes sit above the piano roll, and the footer begins at y=909.
- [x] Keep the native menu bar alive when Mix adds the Routing menu. The macOS journey drives 23 interactions, then records rendered bars in a stereo video. Choosing Master and pads through Accessibility restores every value in the decoded project.
- [x] Choose the track output in place from Timeline or Mix and create a named stereo group in one save. The host journey covers cancel, duplicate and reserved names, pointer assignment and keyboard assignment. The running macOS app routed both keys tracks with every other project field preserved. [Chooser and group screenshots](../designs/prism-timeline/README.md#running-ferroui-window)
- [x] Select a grouped track’s dry bus or Space return in the signal diagram. The host journey reads back the appropriate inspector fields and verifies that inspecting both branches preserves the saved project.
- [x] Cache the miniature pitch traces when the project opens. The native test taps every arrangement page and ruler, moves every gain fader, and rereads the project after switching the dry route from pads to Master and back.

- [x] `ui::Lists::flatten` joined groups one by one, copying the list so far each time, and every frame's paths and scene rows went through it. It joins by halves now: a Timeline tap fell from 10 ms to 1.3 ms (45e335118).
- [x] The song strip shows the song overview: one column per bar, as tall as the bar is busy, with the piano roll's two bars framed; cached when the song opens.
- [x] The application menu is Services, Hide Prism, Hide Others, Show All and Quit: the toolkit's "About FerroUI" is dropped (read back from the running app).
- [x] The Patch page's header reads "Prism / Patch": the open page is lettered over the subtitle baked into the skin's art.
- [ ] Provide separate Timeline and Mix windows where the host supports them. The required independent track patch window is tracked under [DAW-PAGES-PATCH-WINDOW].

### Human-centred UI, user orders of 21 Nov [DAW-HUMAN]

Spec: [Daw.md](../specs/applications/Daw.md) sections [DAW-HUMAN], [DAW-MODULAR], [DAW-CONTROLS], [DAW-ACCESS], [DAW-COMMANDS], [DAW-KEYS], [DAW-STORE], [DAW-INSPECTOR].

- [x] Spec states the human-centred basis (ISO 9241-210) as checkable rules.
- [x] Canvas inspector for modules without skin art: a slider per parameter, set in place, one undo step (26a2241f6).
- [x] Sockets are 28-unit targets that win over overlapping cards; a wire drag is tested the way a hand drags (3996374e3).
- [x] Song pages centre in a wide window, show "No song open" with Open Song, and the patch keyboard docks under Track patch only (a9cfc0a76).
- [x] Patch Controls panel is dark in the Prism palette, without a second transport row (3cd896ee6).
- [ ] **Dials ([DAW-CONTROLS-DIAL]).** Every numeric field is a round dial with its number: press, hold, move up or down; Shift for fine steps; double-press for the default; typing and arrow keys. Slider (horizontal or vertical) where a length reads better. Design worked out with prism-routing-design. Owner: ferroui-prism-lead. Built (c608c5131) in Patch Controls and the canvas inspector: 180 units of travel cross the range, Shift ten times finer, sideways does nothing, one undo per turn, Escape puts the value back, a tap opens the number to type, each dial reports name, value and range. Not built, tracked under Accessibility and Patch Controls: arrow-key steps and focus, double-press for the default (a `Parameter` has no default field yet), dials on the cards, sliders. Not yet compared with `docs/designs/prism-timeline/patch-controls.png` in a live window.
- [ ] **Patch Controls uses the design's controls.** The text boxes, "Connect/Disconnect" buttons and platform-styled menu in that panel are replaced by the toggle, dial, slider, menu and button of [DAW-CONTROLS]; the number box that reformats while typing (392 became 923.0) goes with them.
- [x] **Keymaps ([DAW-KEYS]).** Prism, Ableton style and Logic style from a Shortcuts menu with the one in use ticked, a custom keymap file checked before it replaces the one in use, the choice kept between launches, and a shortcut on every command (486b165cf). Not yet: arguments and stacks on a keymap line ([DAW-COMMANDS]).
- [ ] **Command palette ([DAW-COMMANDS]).** One command list feeding menus, shortcuts and a VS Code style palette.
- [ ] **Timeline zoom ([DAW-TIMELINE-ZOOM], user order).** Pinch, Cmd-wheel and Zoom In/Out/Fit/Selection commands with shortcuts in all three keymaps; zoom about the pointer (keys: about the playhead); the song strip's frame drags to move and its edges to zoom; row height with Alt; ruler divisions follow the zoom; zoom kept per project and never an undo step. Needs: a zoom and offset in the view model (`daw.td`), `TimelineGeometry.lean` taking them, wheel and pinch events from the host. Owner: ferroui-prism-lead. **Built so far (dfb97ae79):** Zoom In, Zoom Out and Zoom to Fit in the View menu with keys in all three keymaps, about the playhead, from the whole song down to one beat; lanes, traces, ruler divisions (bars, beats, sixteenths), taps and a marker under the song overview follow the beats in view; the view follows the playhead; zoom is not an undo step. **Not built:** pinch and Cmd-wheel (the host sends no wheel or pinch event yet), Zoom to Selection, dragging the strip frame, row height, the piano roll's own zoom, zoom kept per project.
- [ ] **Stacked commands ([DAW-COMMANDS]).** A stack is a named list of commands with its own parameters (a macro), one undo step, all or nothing, in an editable file. A shortcut runs a command or a stack, with arguments if the keymap line gives them. Needs the one command list first.
- [ ] **Design screenshots are gorgeous (user order).** Every PNG in `docs/designs/prism-timeline` is finished art at 2×: one type scale, one spacing grid, the Prism palette only, real project content (no placeholder names), no clipped or overlapping text. prism-routing-design draws; the lead signs each off before code follows it.
- [ ] **Commands for an AI agent ([DAW-COMMANDS]).** Run a command by name with arguments from outside the window, one line in and one line out; list the commands and what can run now.
- [x] **Undo and Redo across the project ([DAW-HUMAN]).** The Edit menu shares one history across Patch, Timeline and Bus Routing. The native journey undoes two separate note gestures from other pages, retries after a failed project write, and undoes and redoes an internal send while comparing every saved field.
- [ ] **Accessibility ([DAW-ACCESS]).** Focus and keyboard operation for every canvas control, role/name/value/state reported for each, contrast and colour-independence checked, reduced motion and larger text followed.
- [ ] **Pluggable storage ([DAW-STORE]).** One store interface with JSON (today), database and git (a commit per save, history, restore) behind it.
- [ ] **Master-only render ([DAW-RENDER-MASTER]).** Default render is the single mastered track (done); stems as a separate, explicit render (not built).
- [ ] **Patch Output and inputs ([DAW-MODULAR]).** A patch ends in an Output routed to Master by default; the skin's "Master" card is renamed and no module is called master. Patches declare audio and MIDI inputs as modules and route them with cables.
- [ ] Windows goldens under `examples/daw/goldens/windows` predate the new header and inspector and must be rewritten on Windows (`make -C src/toolchain/targets/windows/tests synth-update`); `routed-delay.png` is the one reported.
- [ ] Real mouse drag on sockets and the menu bar need rechecking in the live app once the desktop is unlocked (the screen was locked during this work).

### Strip the UI [DAW-PAGES-STRIP]

From the committed screens: cut everything that repeats or says nothing (ferroui-prism-lead owns the views).

- [ ] Patch cards: drop the small name above each card (the title already says it) and "Edit in Patch controls" on every card and in the inspector; tapping the card is the edit.
- [ ] Round values for display: "261.6255653005986" shows as "261.63 Hz" (C4). Units on every value.
- [ ] Drop "1 inputs · 1 outputs" (the ports show it) and the bottom legend of cable kinds.
- [x] One play control: the header has one Play and one Stop (an icon group); Audition is a Transport menu command.
- [x] Long enumerated labels become named choices in both patch inspectors and on the module card [DAW-INSPECTOR]. The current mode stays visible and the picker writes its original numeric project value.
- [x] The selected tab names the page. The native journey checks that each of the three page names appears once when selected.
- [x] The inspector shows parameters only when the selected module has them. The native journey selects a stereo output and checks both inspectors.
- [ ] Timeline: the piano-roll "Bar 23" label must not collide with the "Piano roll" title.

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

- [x] Member cache, non-member cache and tombstone-free deletion in the ARC registry (42.3 s to 35.0 s).
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

- [x] Interrupted renders preserve the previous complete WAV. The renderer publishes the temporary WAV after successful encoding, and playback rejects truncated cached files. The interrupted-cache regression passes under GC and ARC.
- [ ] A tap on Timeline right after Audition is ignored about one run in three. Failing test first.

**Interface.** `tests/prism_events.rs` drag: 10.6 ms mean, 18.2 ms worst per pointer move. Timeline taps: 0.57 ms mean. Target: drag under 5 ms mean, under 8.3 ms worst (one 120 Hz frame).

| Where a drag move goes | Share | Fix | Owner |
|---|---|---|---|
| `EditorView::keyboardDock` is rebuilt although nothing in it changed | 28% | Cache it with the card art | ferroui-prism-lead |
| `CardArts::card` redraws the dragged card, because its key holds its position | 25% | Draw each card at the origin once and move it | ferroui-prism-lead |
| `Tellings::render` and `Streams::write` encode the whole scene | 30% | Follows from the two rows above and the next | ferroui-prism-lead |
| `PrismArt::backgroundPart` re-emits the scenery | 13% | Keep the built scenery in the editor | ferroui-prism-lead |

- [x] Keyboard dock cached.
- [x] Dragging reuses the module artwork. The native journey checks the painted sprite, socket and switch track the pointer while the inspector stays fixed in three viewport shapes (077da2b1e). Mean patch move time fell from 3.22 ms to 0.74 ms. A separate five-second window benchmark on the Violet Static grain patch measured 1.80 ms per event before and 1.09 ms after, with 307 shapes in both runs.
- [x] Each Timeline redraw computes the visible ruler interval once (9c33cf379). Dragging the Timeline gain control in a 128-bar project with eight offscreen sixteenth-note parts fell from 30.12 ms to 0.83 ms per move. Both figures measure the native application event handler.
- [x] Scenery kept. With the dock: 4.8 ms mean, 5.1 ms worst (was 10.6 and 18.2); 22/22 journeys pass. The interface target is met.
- [x] `daw_events` also taps the ruler and drags every Mix gain fader: 1.8 ms mean, 10.7 ms worst (the release saves the project).
- [x] `prism_events` holds a drag move to one 120 Hz frame on average (8.3 ms), two at worst.

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
| `synth::Header::placed`, `placedAreas` ([header_view.ospml:149](../../src/osprey/examples/daw/src/header_view.ospml), :152), through `Header::shift` and `ui::Machines::number` | tick 34.5%, mix 50.3%, tap 26.7% | The lambda calls `shift editor` once per shape (about 400). Each call reads two knobs by name | Bind the shift once, outside the lambda | ferroui-prism-lead |
| `ui::Machines::knob` ([machine.ospml:112](../../src/osprey/packages/native/src/machine.ospml)) and its predicate `k.knobName == name` (`__closure_fn_12`) | tick 35.9%, resize 42.1%, mix 51.8% | A knob is found by comparing its name with every knob's name, for every read, through `ui::Lists::find` ([base.ospml:23](../../src/osprey/packages/native/src/base.ospml)): a whole-list filter, a list and a boxed result per read | Keep knobs in a map by name, or resolve a name to its index once per frame | ferroui-prism-lead |
| `ui::Streams::write`, `etches`, `path`, `step` ([stream.ospml:55-62, 76-78, 106, 116](../../src/osprey/packages/native/src/stream.ospml)) | tick 41.5%, tap 44.4%, patch 21.6% | Every path step is built as a record inside a one-element list; `Lists::flatten` joins them; only then are the rows written. One step costs a record, a list, a builder and a join | Write each row to the bridge as it is produced, folding the refused count; no `List<Etch>` in between | ferroui-prism-lead |
| `ui::Lists::spanned` ([base.ospml](../../src/osprey/packages/native/src/base.ospml), called by `flatten`) | tick 16.9%, tap 17.6% | Joining by halves still copies every element log n times and retains and drops each one | Goes away with the row above; elsewhere, one builder that every group is pushed into | ferroui-prism-lead |
| `ui::Surfaces::shown` and `ui::Tellings::render` (the whole telling) | 97.5% of every event | No event is ever answered with "nothing changed": the view is rebuilt, told and written even when the stamp is the same | Compare the root stamp first and answer an unchanged event without building the view; tell only regions whose stamp changed | ferroui-prism-lead |
| `synth::CardArts::refreshed` ([card_art.ospml:28](../../src/osprey/examples/daw/src/card_art.ospml), from [editor_app.ospml:213](../../src/osprey/examples/daw/src/editor_app.ospml)), `CardArts::card`, `PrismArt::drawn`, `ui::Canvases::artboard`, `ui::Templates::measure` and `fill` | resize 78.9% (card 64.8%, artboard 58.1%, measure 43.1%, fill 41.6%) | A resize redraws the art of every Patch card, whichever page is showing, and each template piece reads its knobs by name | Refresh card art only on the Patch page and only when the card scale changed; give a card its knob values once, as a record | ferroui-prism-lead |
| `synth::PrismArt::atlas` | patch 14.4%, mix 5.4%, tick 3.7% | The atlas list is rebuilt for every event | Build it once at launch and keep it in the editor | ferroui-prism-lead |
| `synth::Header::patchSubtitle`, `EditorView::toolbar` | patch 7.7%, 8.8% | Rebuilt per event although they change only with the page and the track | Cache with the header, keyed by page and track | ferroui-prism-lead |
| `synth::Graphs::node` | patch 4.4% | Finds a node by filtering the whole node list | Same search fix as `Lists::find` (code generation row 2) | synth-advanced |

Targets: an event that changes nothing under 0.1 ms; a Timeline or Mix event under 0.7 ms; a resize under 2 ms.

- [x] `Header::placed` and `placedAreas` bind the shift once. Same machine load, before and after: Timeline 2.52 ms to 1.71 ms per event, Mix 1.92 ms to 0.89 ms (the quiet-machine figures in the table above were 2.05 and 1.55).
- [ ] Unchanged events answered without rebuilding the view.
- [x] `Streams` writes rows directly, with no intermediate list: one fold-style encoding (`Streams::laid`, `etched`) that the reader collects and the writer sends row by row. With the three fixes below it, 21 Nov 11:46, same probe: Timeline event 1.65 ms to 0.52 ms, Timeline frame tick 1.88 to 0.58, Mix 0.93 to 0.65, Patch 0.60 to 0.40, command Timeline 1.98 to 0.60.
- [x] Knob search stops at the first match and builds no list (`ui::Lists::find` through `sought`); the name compare no longer takes a lock (code generation row 1). A map by name is still open if `Machines::knob` shows again in a profile.
- [x] Card art refreshed only on the Patch page (`CardArts::cards`): Timeline resize 12.43 ms to 2.34 ms. Still open: a Patch resize redraws every card (3.38 ms to 2.84 ms) although only the card scale matters.
- [ ] Atlas, toolbar and patch subtitle cached.

**Engine.** Bars 9 and 10 again: 36.0 s (35.4 s of CPU, with the sampler attached). Nothing has moved since 35.0 s, and none of the block-rendering items above has landed. The same functions hold the time (12,808 samples, own code plus the runtime each calls directly):

| Function (file:line) | Share | Cause |
|---|---|---|
| `synth::RenderInputs::parameter` ([render_inputs.ospml:17](../../src/osprey/packages/synth/src/render_inputs.ospml)) | 23.2% | `listGet (filterList (snapshot.parameters, \p => p.node == identity && p.key == key)) 0` per parameter per node per sample: a string compare per parameter, a list and a boxed result per read |
| The per-sample closures of `synth::Renderer` ([renderer.ospml:46, 87, 91, 94](../../src/osprey/packages/synth/src/renderer.ospml)): `sample`, `evaluated` (the fold over `plan.order` calling `nextNode`) and `finished` | 9.4% + 5.8% + 3.0% own, 92% inclusive | Four closures deep per sample, each a heap environment and a boxed `Result` through `Outcomes::carried`. `finished` searches `outputs plan.graph` again, appends one sample with `listAppend` and rebuilds every meter with `metered`, per sample |
| `RenderInputs::input` 6.4%, `emitted` 5.5%, `held` 4.3%, `Graphs::node` 4.2%, `RenderKernels::step` 4.2%, `RenderKernels::selected` 2.3% | 27% | State and node found by filtering lists, per node per sample |
| `daw::Arrangements::bused` | 3.6% | A list built per sample per side |
| `PartialBanks::partial`, `stepped` | 4.0% | One record per partial per sample |

Leaf time is the runtime, not arithmetic: `arc_drop_child` 18.2%, `arc_release_zero_locked` 18.1%, `osp_retain` 7.8%, `arc_alloc_locked` 6.9%, `osprey_list_get` 6.6%, `osp_release` 6.5%, `memset` 5.1%, `osp_mem_string_length` 3.0%.

- [ ] `RenderInputs::parameter`, `input`, `held` and `Graphs::node` resolved once per block into the plan (the existing block-rendering items; owner still OspreyOpus1 and synth-advanced, nothing delivered).
- [x] First step, 21 Nov 11:55: the per-sample lookups (`RenderInputs::held`, `parameter`, `input`, `sample`, `Renderer::measured`, two in `RenderKernels`) stop at the first match through `synth::Seek::first` and build no list: bars 9 and 10 render in 35.2 s, from 39.8 s at the same commit and load, audio byte-identical. `Graphs::node` keeps the filter: its closure is still generic where the daw's tests instantiate it, and code generation refuses a generic closure value. `Seek::from` is now 19% of the render, so resolving once per block is still the fix.
- [ ] `Renderer::finished` finds the sink once per render, collects a block's samples in one builder and meters once per block, not per sample.

**Code generation and runtime: what the generated code does that it need not.** Each row was read from the LLVM IR of the daw (`osprey src/osprey/examples/daw --llvm --memory=arc --target=macos`) and matched to the samples. These cost every Osprey program, not only the daw.

| # | What is generated | Evidence | Cost measured | Fix |
|---|---|---|---|---|
| 1 | **String `==` is an ordering compare with two locked length lookups.** `a == b` emits `call i32 @osp_string_compare` then `icmp eq 0`. `osp_string_compare` ([string_runtime.c:63](../../src/toolchain/compiler/runtime/string_runtime.c)) takes both lengths through `osp_strlen`, which calls `osp_mem_string_length` ([memory_arc.c:234](../../src/toolchain/compiler/runtime/memory_arc.c)): the heap lock and a registry lookup each, a `strlen` when the string is a literal, then `memcmp` | IR of `__closure_fn_12`; `osp_mem_string_length` + `memcmp` + `strlen` + `osp_string_compare` | tick 23%, resize 31%, render 7% | Emit `osp_string_equal` for `==` and `!=`: equal pointers are equal; otherwise one pass over the bytes with no length lookup. Against a literal, pass the literal's length, known at compile time |
| 2 | **`listGet (filterList (xs, p)) 0` scans the whole list and builds a list.** Emitted as: closure environment `osp_alloc_tagged`, `osprey_list_builder_new_of`, a loop over every element (`osprey_list_get`, indirect call, `osprey_list_builder_push_borrowed`), `osprey_list_builder_seal`, `osprey_list_in_bounds`, `osprey_list_get` | IR of `ui::Machines::knob` (143 lines for a one-line function) | `ui::Lists::find`, `RenderInputs::parameter`, `Graphs::node`: tick 36%, render 23% + 4% | Recognise the shape, or add a `listFind` built-in: a loop that returns at the first match, with no list and no builder. (Stopping early while still building the one-element list was tried and rejected above; the list is the cost) |
| 3 | **A lambda written at a `mapList`, `filterList` or `foldList` call is still a heap closure called through a pointer.** `osp_alloc_tagged(16, 513)`, `osp_retain` of each capture, then per element `load` of the function pointer and `call i1 %r21(...)`, so `-O2` cannot inline the body | IR of `ui::Machines::knob`, lines 20 to 46 | One allocation and one release per call site per call, and an indirect call per element everywhere | When the argument is a literal lambda, emit its body in the loop (or a direct call with the captures on the stack); no heap environment |
| 4 | **Every element read in a generated loop is an out-of-line `osprey_list_get`**, which checks the bounds the loop has already established; a checked `listGet` is two calls (`osprey_list_in_bounds`, `osprey_list_get`) | IR of every `mapList`/`filterList`/`foldList` loop | 6.2% to 6.6% of leaf time in all four profiles | Read the element inline when the loop bound proves the index; or ship the runtime's fast paths (`osprey_list_get`, `osp_retain`, `osp_release`) as bitcode so `-O2` inlines them |
| 5 | **A list literal goes through a builder.** `[x]` is `osprey_list_builder_new_of`, `osprey_list_builder_push_borrowed`, `osprey_list_builder_seal`, with `osp_alloc` and `osp_alloc_tagged` | IR of `ui::Streams::step`: five builders for five one- or two-element literals | Part of `Streams::step` and `path` (tick 20%) | A literal of known length is one allocation with its elements stored directly |
| 6 | **A new list is tagged by a second locked call.** `osp_mem_set_layout` ([memory_arc.c:323](../../src/toolchain/compiler/runtime/memory_arc.c)) takes the heap lock and looks the block up again, straight after the allocation, for every list header and node ([list_trie.h:86, 125](../../src/toolchain/compiler/runtime/list_trie.h)) | Leaf samples | tick 4.9%, resize 2.0%, render 1.8% | Pass the layout to the allocation |
| 7 | **Every `Result` is a heap object**, and turning one error into another allocates a second. `listGet` allocates `osp_alloc_tagged(24, 1281)` for its answer; `match … Error _ => Error(…)` allocates another and retains the message literal | IR of `ui::Machines::knob`: three `osp_alloc_tagged(…, 1281)` for one lookup; function type `{ i64, i8, i8* }*` | Part of `arc_alloc_locked` + `arc_release_zero_locked`: tick 11%, render 25% | Return a `Result` by value (two registers or `sret`) when it is matched at once and never stored |
| 8 | **A pure call that does not depend on the lambda's argument is evaluated for every element.** `mapList (shapes, \shape => moved (shape, shift editor))` calls `shift editor` 400 times | `Header::placed`, tick 34.5% | See the first interface row | Hoist a call to an effect-free function out of the loop when none of its arguments mentions the lambda's parameters |
| 9 | **Retain and release are emitted for string literals.** `call void @osp_retain(i8* %r40)` where `%r40` is `@.str.174` in read-only data; the runtime probes to find it is not its own | IR of `ui::Machines::knob` | Part of `osp_retain` (tick 11.1%) | The compiler knows a literal: emit no retain or release for it |
| 10 | **`listConcat`, `flatten` and `mapList` are never fused.** `listConcat [head] (listConcat (Lists::flatten (mapList (steps, step))) …)` allocates each intermediate list | `ui::Streams::path` (stream.ospml:76-78) | tick 10% own plus `Lists::spanned` 17% | Lower nested concat, flatten and map into pushes on one builder |
| 11 | Dropping a list of records drops each element through the registry (`arc_drop_child`) | Leaf samples | tick 16.8%, resize 11.1%, render 18.2%: the largest single leaf in three of four profiles | Already open above ("Direct element drops for proved lists"); this round says it is the first runtime fix to make |
| 12 | A record update allocates and copies | Already open above | | In-place reuse of a unique record |

Order by what it buys across all four profiles: 11 (drops), 1 (string equality), 2 (find), 7 (`Result` by value), 3 (inline lambdas), 4 (inline element reads), then 5, 6, 9, 10, 8.

- [x] 1. String compare decides at the first unequal byte with no length lookup (`osp_string_compare`, [string_runtime.c:63](../../src/toolchain/compiler/runtime/string_runtime.c)); only strings equal up to a NUL take the two locked lengths. No new entry point was needed, so `==`, `!=`, ordering, list search and structural equality all gain. Timeline tick: `osp_mem_string_length` + `osp_string_compare` + `memcmp` 23% to 1.4% of samples. Still open: equal strings (one hit per search) take the slow path; a literal's length known at compile time would remove it.
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

- [x] The redundant-annotation analyser ran in every build and re-inferred the whole program once per proof: `--check` took 4 min 30 s. It is now its own pass (`--analyse`, and `--warnings-as-errors`), never part of a plain build (cb7515505): `--check` 21 s, full compile 50 s.

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

## Verification

- [x] The 22 DAW journeys passed under GC and ARC. The generated field codec also round-tripped the edited 17-track project with all 2,913 notes and every JSON value equal.
- [x] 21 Nov 01:16: the daw suite passes 24/24 (Render song, buses and sidechain, arrangements), checked on a clean `git archive` of HEAD. `make -C src/toolchain/targets/macos/tests daw-events-headless` passes on the FerroUI host: Mix fader drags 1.16 ms mean (7.35 ms worst, 136 events), Timeline taps 0.51 ms (0.95 ms, 119), Prism drags 3.68 ms (4.00 ms, 55).
- [ ] The duplication gate is green: 4.1% against the 4.00% ceiling on 21 Nov.
- [ ] `make ci` passes; a coordinated push is made under the TMC lock.
