# GPUI Desktop Host [GPUI-HOST]

**Status:** running on macOS in `src/toolchain/hosts/gpui`: the window, row renderer, scene painter, frame clock and the smoke and self-test runs work, and two examples run on it (`src/osprey/examples/gpui/flock` and `src/osprey/examples/gpui/painting`, the latter pixel-identical to GPUI's own painting example). The same host code draws through the Android backend, and Linux and Windows have library targets and CI ([GPUI-HOST-TARGETS]). Delivery is tracked by [plan 0037](../../plans/0037-osprey-native.md).

GPUI is a UI framework that draws with the GPU. It is not Osprey's GPU compute surface ([0034](../runtime/0034-GPUComputation.md)): an Osprey program here never touches the GPU itself. It describes a scene as data, and the Rust host hands that scene to GPUI, which batches, tessellates and draws it with Metal (Vulkan, DirectX later). [GPU-VS-GPUI](../runtime/0034-GPUComputation.md#gpu-compute-and-gpui-rendering--gpu-vs-gpui) sets out the difference.

A GPUI host renders an [Osprey Native](../../../src/osprey/packages/native/README.md) application with [GPUI](https://gpui.rs), the GPU-accelerated Rust UI framework from Zed. It is a host in the [Osprey Native host family](../applications/0047-OspreyNativeHosts.md), and the first that can run on macOS, Linux and Windows from one renderer. The first target is macOS.

## The seam [GPUI-HOST-SEAM]

The Rust crate is only the seam between GPUI and Osprey Native. It contains:

- a loader that calls `osprey_main()` once and then the application's `argument`, `open` and `event` exports from the Osprey static library ([GPUI-HOST-BOUNDARY]);
- each call's frame, sealed and leased from the bridge: scene, view, text and ask runs read in place (`frame.rs`);
- `rows.rs`, which checks every row and reads it as typed controls, scenes and asks; a malformed frame is refused whole ([NATIVE-SHM-SAFETY]);
- a renderer that maps every view row kind to GPUI elements, every style taken from the row ([GPUI-HOST-RENDER]);
- adapters for the capabilities the host supports ([GPUI-HOST-CAPABILITIES]);
- the frame clock ([GPUI-HOST-FRAME]).

The crate contains no application logic. It never decides what a tap means, what a score is, what a row shows, where an entity moves or which entities collide. Any rule a test can state about the application belongs in Osprey. A reviewer rejects a Rust change that encodes application behaviour, with one exception: the renderer may clamp, cull and batch drawing for the GPU, provided the same scene drawn without those steps looks the same.

## Boundary [GPUI-HOST-BOUNDARY]

The host builds the application with `--target=macos` (later `linux-x64` and `windows-x64`, [GPUI-HOST-TARGETS]) and links the archive. The exports are the three every Osprey Native application has ([NATIVE-SHM-BOUNDARY]):

```c
int64 osprey_app_argument(const char *text);
int64 osprey_app_open(int64 now, int64 offset, const char *platform, const char *home, bool dark);
int64 osprey_app_event(int64 now, int64 kind, int64 ordinal, const char *words, const char *detail,
                       double x, double y, bool down, bool shift);
```

Calls are synchronous and serialized on GPUI's main thread. Input strings are borrowed for the call, and the export returns only a status, because the frame is in the bridge: when a call returns, the host seals it, leases it and checks every row ([NATIVE-SHM-SAFETY]). A refused event still draws a whole frame, with the application's reason among its diagnostic asks. The host answers the application's `native_slice` import from the host-held slices its ask rows wrote ([NATIVE-SHM-ASK]).

A control event sends the control's number and its words; scene input arrives as the typed key, pointer and size calls, and completions, lifecycle phases, display frames and appearance changes each as their own kind ([NATIVE-SHM-EVENT]). The host adds nothing else, and keyboard input reaches Osprey as key events on the scene that has focus; the host does not translate keys into actions.

The start context's `platform` is the operating system as `std::env::consts::OS` spells it (`macos`, `linux`, `windows`), which `Context.platform` retains as a string; `Symbols::platform` converts it to the `Platform` union when needed ([UI-CONTEXT-PLATFORM](../../../src/osprey/packages/native/README.md)).

### Opened files [GPUI-HOST-OPEN]

Files the desktop asks the application to open (Finder on macOS) are held until a frame can carry them. Each is then delivered as the lifecycle phase `open:<path>`, with a `file://` URL turned back into its path.

## Rendering [GPUI-HOST-RENDER]

The renderer draws the frame's view rows in drawing order; a row's `parent` names the row it sits in. Every stateful row's GPUI element id is its row index, so focus, scroll offsets, text selection and expanded disclosures survive a frame that keeps the row. Each of the seventeen row kinds has exactly one mapping in `view.rs`, and the coverage test fails if one is missing; a kind the frame reader does not know refuses the whole frame before the renderer sees it.

Every colour, size, weight and space comes from the row: the host applies the application's look and holds no style of its own. It follows the system appearance and sends an `appearance` event when that changes. Every framework symbol ([UI-COMPONENTS-SYMBOLS](../../../src/osprey/packages/native/README.md)) maps to one of the Lucide icons that ship with `gpui-kit`; the host's unit tests fail if a symbol has no icon. An unknown symbol name draws a visible placeholder and a diagnostic. The host's own diagnostic strip shows why a frame, event or capability was refused, newest last.

## Frame clock [GPUI-HOST-FRAME]

An application turns the host's frame clock on with an `Animate` ask and off the same way. The host completes the request like any other capability and, while the clock runs, delivers one `frame` event per display refresh, its `x` the seconds since the previous frame. The host requests the next animation frame from GPUI after each event, so the clock runs at the display's rate, and it stops while the window is hidden. A frame reaches `update` as the `Frame` member of `Message<A>` with its elapsed seconds; `update` returns the next store and the regions that changed, exactly as for any other event. The host never runs a simulation step of its own.

## Scene [GPUI-HOST-SCENE]

A canvas row draws the scene rows that follow its marker: a surface in the application's own unit space, `width` by `height`, that the host draws from its region's top left at up to its requested `zoom` (window pixels per unit, 1 when absent). This zoom is an upper bound: it remains fixed while the whole artboard fits, and shrinks uniformly when either viewport dimension would clip it. Painting, pointer input and accessibility use the same scale. A `zoom` of 0 instead draws the scene as large as fits the region whole, so it grows and shrinks with the window. A scene of zero width and height is in **window pixels**: no clip, its origin the window's top left, as a GPUI canvas paints. `Ui::canvas` builds one.

Each shape is one fixed 88-byte row (`SceneShape` in [`native.td`](../../../src/toolchain/schemas/native.td)), so the painter reads it without a schema ([NATIVE-SHM-SCENE]):

| Code | Row | Values |
| --- | --- | --- |
| 0 | rectangle | x, y (centre), w, h, angle in radians, colour, depth |
| 1 | disc | x, y, radius (in `width`), colour, depth |
| 2 | strip | pair count in `index`, then that many point rows, stroke width in `width`, colour, depth |
| 3 | sprite | atlas index, x, y, w, h, angle, tint colour, depth |
| 4 | glyphs | text, x, y, size, colour, depth |
| 5 | path | its steps, outline and paint follow (below) |
| 6 | lettering | text, x, y, size, colour, depth, anchor in `index` (left 0, centre 1, right 2), weight in `height` |

A path is a code 5 row whose `index` is its step count and whose `x`, `y` and `width` move and scale it, then its steps as code 8 rows (`index` 0 move, 1 line, 2 quadratic curve, 3 cubic plus one point row, 4 arc with `colour` 1 large and 2 sweep, 5 polygon with `depth` the count of point rows and `colour` 1 closed, 6 close), then one code 9 outline (`index` 0 fill or 1 line of `x` width, `colour` 1 bevel, `depth` the dash count, dash lengths as point rows) and one code 10 paint (`index` 0 solid or 1 linear), each followed by its tints as code 11 rows (`index` 0 sRGB or 1 HSL in `x`..`height`; a linear paint's `x` is its angle and `colour` 1 marks Oklab). A tint's channels are floats from 0 to 1, kept in the application's space so no conversion rounds it. The host replays the steps on its own path builder in that order and adds no call of its own when the scene is in window pixels, so a path built the same way in a native GPUI program and in Osprey tessellates to the same pixels.

An atlas entry (code 21) names its image by text; a sprite names its image by index into that atlas, which the host resolves to textures it owns. A colour is one integer packed `0xRRGGBBAA`; `depth` orders layers from the back, and the painter draws one pass per depth and instances primitives that share a sprite.

The scene rows are streamed shape by shape into the frame's scene run, after the canvas row they belong to, so a scene that changes every frame costs only its own rows: 5,000 sprites cost about 7 ms a frame, `update` included. The host measures the time from the event call's return to the end of paint and reports it in its diagnostics overlay. The acceptance gate in the plan sets the entity count and frame rate the macOS build must hold; the framework's own per-frame cost is measured by `src/toolchain/hosts/gpui/bench` and recorded in the plan.

The canvas row carries one binding. The host sends keys as `key` events (`words` the name, `down` pressed), the pointer as `pointer` events (`x`, `y` in scene units, `down`, `shift`) and, when a sized scene first paints and whenever its region changes size, a `viewed` event (`x` width, `y` height), all in scene units through that binding, and nothing else; the application decides what a key or a position means. Key names are host-neutral: `left`, `right`, `up`, `down`, `space`, `enter`, `escape`, `tab`, `backspace`, and a single lowercase character for letters and digits; a key outside that vocabulary is not sent. A pointer event is sent on press, release and on movement while pressed, and once per movement when not pressed, at most once per frame.

### Text accessibility [GPUI-HOST-SCENE-TEXT-A11Y]

Visible glyph and lettering runs containing words or numbers contribute passive AccessKit `Label` nodes. Each label carries its rendered string and bounds measured through the painter's text shaper, with the same font, weight, anchor and fitted scale. Passive labels have no activation or value-adjustment actions; described controls retain their own roles, ranges and action bindings. Platform adapters expose these shared semantics through the native accessibility API.

## Capabilities [GPUI-HOST-CAPABILITIES]

The host answers `Run`, `ListDirectories`, `ReadTexts`, `StoreRead`, `StoreWrite`, `StoreList`, `StoreDelete`, `ChooseFile`, `ChooseFolder`, `SaveFile`, `Reveal`, `Cancel` and `Animate` with Rust adapters. Each request arrives as an ask row ([NATIVE-SHM-ASK]) and each completion is one `completed` event. A capability the host does not answer is completed with an error naming it. Platform-only work is reached through `HostCall`, answered only through the mobile OS bridge on iOS and Android, or completed with an error.

## Library targets [GPUI-HOST-TARGETS]

A desktop static-library target exists per platform, with the same export selection, generated header and memory rules as the Mac slice of the iOS target:

| `--target` | Built on | Archive contents | The host links |
| --- | --- | --- | --- |
| `macos` | macOS (Xcode) | the application and the Mac ARC runtime, with the Metal adapter | the archive, `libosprey_runtime_macos_arc.a` |
| `linux-x64`, `linux-arm64` | Linux of that architecture | the application and the native ARC runtime (`libfiber_runtime_arc.a`) | the archive, `pthread`, `dl`, `m` |
| `windows-x64` | Windows under MSYS2 UCRT64 | the application and the MinGW-built native ARC runtime | the archive, `ws2_32`, built by MSYS2's UCRT64 Rust (`x86_64-pc-windows-gnu`), so the host, GPUI and the runtime link one UCRT |

A Linux or Windows slice is built on its own system, because it bundles that system's native runtime; asking for one elsewhere is refused with the system to build it on (`--check` still works anywhere). The CLI compiles the IR with `clang -O2 -fPIC` (`OSPREY_CC`) and bundles the runtime with `llvm-ar` (`OSPREY_AR`) (`src/toolchain/crates/osprey-cli/src/desktop.rs`). `src/toolchain/hosts/gpui/run.sh` picks the target for the system it runs on, and the host's `build.rs` links what the table says. All desktop slices default to ARC.

## Verification [GPUI-HOST-VERIFICATION]

Acceptance requires: the Osprey application's own test suite passing against fixture messages; the host rendering the `src/osprey/packages/native/tests` probe application; a deterministic smoke run (`run.sh --smoke`) that starts the probe, taps Add and asserts "Count 1" through the real boundary, with no window; and a measured frame-rate run of the example application at the gated entity count on an Apple silicon Mac. Linux and Windows acceptance requires the same smoke run on those platforms in CI. Every app's `self-test.json` also runs on this host without a window on Mac and Linux, and in the real window on the Mac runner ([UI-MACHINE-SELF-TEST]).
