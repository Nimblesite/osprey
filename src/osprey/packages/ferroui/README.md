# ferroui-osprey-native: the FerroUI host of Osprey Native

An Osprey application compiled as a C library runs on [FerroUI](https://github.com/wieslawsoltes/FerroUI)
unchanged: each call's frame is read in place from the shared-memory boundary
([NATIVE-SHM-BOUNDARY]), its told regions are drawn with FerroUI controls and its scenes with
FerroUI's drawing context. The host began as FerroUI's `ferroui-osprey-native` by Wiesław Šoltés
([PR #967](https://github.com/wieslawsoltes/FerroUI/pull/967)); FerroUI is MIT, credited in
[NOTICE.md](NOTICE.md) with its licence in [LICENSE-FerroUI](LICENSE-FerroUI).

`make -C src/toolchain/targets/macos studio` builds the DAW with this host and installs Perception Studio in Applications. Open Perception Studio from Spotlight.

# Perception Studio on FerroUI: can FerroUI replace GPUI?

**Verdict: yes.** On the same Mac, from the same compiled Perception Studio library over the same
shared-memory boundary, FerroUI draws Perception Studio in the same layout as a current build of the GPUI
host, and is faster: dragging a module runs at 24–25 fps on FerroUI against 18 fps on GPUI. Its own
cost is about 1 ms per interactive frame, and it repaints the whole studio at the display's refresh
rate. Audio and accessibility were not checked; see the gaps below.

## How it runs

Perception Studio is Osprey code compiled as a C library (`osprey <synth> --compile --target=macos`). The host
(this folder) links it and calls its `argument`, `open` and `event` exports. Each
call's frame (view rows, scene rows, asks and their text) is written once by the Osprey runtime
into the shared bridge; the host seals it, leases it and reads it in place (`frame.rs`, `rows.rs`,
ported from Osprey's GPUI host). No JSON goes either way: the PR's envelope protocol is deleted.
FerroUI then maps the told component tree to controls (`view.rs`) and draws each scene with its
drawing context on the GPU (`surface.rs`): rects, discs, strips, atlas sprite images, glyphs,
lettering, and paths with gradients and dashes.

`scripts/studio.sh` builds and opens it, and bounds every run:

| Command | Result |
|---|---|
| `studio.sh` | Builds and opens Perception Studio (closes after 600 s) |
| `studio.sh --shot <png>` | Captures the window, then closes |
| `studio.sh --drag <secs>` | Drags the Oscillator module every frame, prints timings |
| `studio.sh --repaint <secs>` | Rebuilds and repaints the whole view every frame with no app call |

## Layout

Both captured on the same Mac, same 1152x756 window, bare chrome, same Perception Studio patch:

| FerroUI | GPUI (current host, built from Osprey `main` on the day) |
|---|---|
| ![FerroUI](docs/studio-ferroui.png) | ![GPUI](docs/studio-gpui-macos.png) |

The two match: the header with the toolbar and undo/redo, the patch canvas (LFO, Oscillator,
Filter, Master and Scope modules, solid audio cables, the dashed modulation cable), the
inspector, the legend, and the 135-point keyboard dock with "Now playing". The application lays
out its canvases itself, so the host only has to place the regions; FerroUI's `Grid` and `Border`
reproduce the GPUI host's flex layout (only `frameGrow` frames and unsized scenes take free space).

![Mid-drag](docs/drag-ferroui.png)

Mid-drag: the Oscillator module moves, its cable reroutes and the inspector switches to it.

## Speed against GPUI (same Mac, same archive, alternating runs)

Each run drags the Oscillator module from scene point (131, 376) around the same circle, one move
per display frame, for 10 seconds. FerroUI runs `studio.sh --drag 10`. GPUI runs a scratch copy of
its host with the identical run added ([gpui-drag-bench.rs](docs/gpui-drag-bench.rs)); Osprey's repository is
not changed.

| Host | Run 1 | Run 2 | Run 3 | Per event (app + apply) |
|---|---|---|---|---|
| FerroUI | 22.1 fps | 25.3 fps | 24.2 fps | 38–43 ms |
| GPUI | 17.6 fps | 18.1 fps | 17.9 fps | 40–41 ms |

Both spend almost all of a frame in Perception Studio's own `event` export. FerroUI turns the same work into
more frames per second.

**Since these runs, Perception Studio's own cost per drag move fell from about 43 ms to 4.8 ms** (worst 5.1 ms),
inside one 120 Hz frame on average: card artwork is sorted once per skin, each module's card is redrawn only
when that module changes, and the scenery and keyboard dock are kept while the selection and note stay. [tests/studio_events.rs](tests/studio_events.rs) holds it there. The tables in this
file were measured before that change.

## FerroUI's own cost (`fast-release`, 282–292 scene shapes)

| Run | fps | Osprey app per event | Frame read | Tree patch | Scene paint |
|---|---|---|---|---|---|
| Drag a module (`--drag 10`) | 22.1 | 42.8 ms | 0.07 ms | ~0.1 ms | 0.78 ms |
| Repaint only (`--repaint 10`) | 59.2 (the display's limit) | none | none | ~0.1 ms | 0.83 ms |

FerroUI's share of a drag frame is about 1 ms. The rest is Perception Studio's own `event` export (its graph
update and redraw), the same compiled code under either host.

## Profile: where the time goes (`sample`, `footprint`, `leaks`)

A 75-second drag run, profiled on the app's own process:

- **CPU:** the main thread's work is almost all inside Perception Studio's `event` export. The hot path is
  `synth::EditorView.canvas` → `nodeArtwork` → `StudioArt.card` → `owns`/`belongs`, with
  `ui::Canvases.artboard` and `ui::Templates`: every event rebuilds every module's artwork,
  finding art in the machine document by string search (`osp_string_find_internal`,
  `osp_mem_string_length`) and formatting numbers through `vsnprintf`, plus ARC traffic. That
  path took 2,079 of the 2,188 samples inside `osprey_app_event`. FerroUI's rendering is about 200
  samples on its render thread. **The ~40 ms per event is Perception Studio's own view code**, and GPUI runs
  the same code.
- **Memory:** a flat physical footprint of 523–526 MB for the whole run (peak 670 MB at start-up).
  Nothing grows.
- **Leaks:** 1,393 leaks, 72 KB in all, bounded (they don't grow with the run). The largest group (663 `String` clones) is
  in FerroUI's own `FontManager::try_get_glyph_typeface` → `GlyphTypeface::new`, reached from
  `FormattedText`; the rest are Metal/Skia Graphite and AppKit objects. Not caused by the host code.
- **Audio crackle:** Perception Studio reports "Audio queue ran empty" under both hosts (GPUI 4, FerroUI 1 and
  3 times per Audition), because its audio worker's exchanges complete through the same 40 ms
  `event` calls. The host now answers worker completions above input priority and sends at most one
  pointer move per frame (the latest), so a fast mouse no longer queues stale moves ahead of audio.
  Before that change, a 2-second drag left the host working through queued moves for 8 seconds
  after release.

## Gaps (not checked or not equal yet)

- **Traffic lights:** `OSPREY_NATIVE_TITLE_BAR=50` centres them 18 points down, as GPUI's
  `OSPREY_GPUI_LIGHTS=16,18` does, but FerroUI cannot move them sideways: they sit about 7 points
  left of GPUI's.
- **Scene accessibility** (`scene.access`) is not exposed to assistive technology.
- **Audio** plays under both hosts with the same dropouts (see the profile above). Without
  `--vst-worker`, playback fails with "Executable must be an absolute path"; `studio.sh` passes it.
- **Only macOS** was run.
- **Patch controls mode** (a tap on "Patch controls", `STUDIO_TAP=575,32`) has the same structure
  under both hosts: the heading and Close, Audition, Play and Stop, the message, the module
  picker, Parameters, then Connections, in the 280-point aside. The control styling differs:
  FerroUI's Fluent theme puts the picker's label beside it, draws Play in accent blue and gives the
  fields light borders, where GPUI stacks the label above a full-width picker.

| FerroUI | GPUI |
|---|---|
| ![Patch controls on FerroUI](docs/patch-controls-ferroui.png) | ![Patch controls on GPUI](docs/patch-controls-gpui.png) |
