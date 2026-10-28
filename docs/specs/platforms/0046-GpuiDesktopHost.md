# GPUI Desktop Host [GPUI-HOST]

**Status:** running on macOS in `hosts/gpui`: the window, renderer, scene painter, frame clock and recorded-event smoke run work, and two examples run on it (`examples/gpui/flock` and `examples/gpui/painting`, the latter pixel-identical to GPUI's own painting example). Linux and Windows have library targets and CI ([GPUI-HOST-TARGETS]). Delivery is tracked by [plan 0036](../../plans/0036-gpui-desktop-host.md).

GPUI is a UI framework that draws with the GPU. It is not Osprey's GPU compute surface ([0034](../runtime/0034-GPUComputation.md)): an Osprey program here never touches the GPU itself. It describes a scene as data, and the Rust host hands that scene to GPUI, which batches, tessellates and draws it with Metal (Vulkan, DirectX later). [GPU-VS-GPUI](../runtime/0034-GPUComputation.md#gpu-compute-and-gpui-rendering--gpu-vs-gpui) sets out the difference.

A GPUI host renders an [Osprey Native](../../../packages/native/README.md) application with [GPUI](https://gpui.rs), the GPU-accelerated Rust UI framework from Zed. It is a host in the [Osprey Native host family](../applications/0047-OspreyNativeHosts.md), after Vigil's SwiftUI host and the VS Code webview host, and the first that can run on macOS, Linux and Windows from one renderer. The first target is macOS.

## The seam [GPUI-HOST-SEAM]

The Rust crate is only the seam between GPUI and Osprey Native. It contains:

- a loader that calls `osprey_main()` once and then the application's `start`/`dispatch` exports from the Osprey static library;
- a store of regions keyed by the framework's region keys, patched from each envelope;
- a renderer that maps every `Node` kind to GPUI elements;
- adapters for the capabilities the host supports;
- the frame clock ([GPUI-HOST-FRAME]).

The crate contains no application logic. It never decides what a tap means, what a score is, what a row shows, where an entity moves or which entities collide. Any rule a test can state about the application belongs in Osprey. A reviewer rejects a Rust change that encodes application behaviour, with one exception: the renderer may clamp, cull and batch drawing for the GPU, provided the same scene drawn without those steps looks the same.

## Boundary [GPUI-HOST-BOUNDARY]

The host builds the application with `--target=macos` (later `linux-x64` and `windows-x64`, [GPUI-HOST-TARGETS]) and links the archive. The exports are the ones the SwiftUI host already calls:

```c
const char *start(const char *context);
const char *dispatch(const char *saved, const char *event);
```

Calls are synchronous and serialized on GPUI's main thread. Input strings are borrowed for the call; the host copies the returned string immediately. The envelope is the one `@osprey/native` encodes ([NATIVE-HOSTS-TRANSPORT](../applications/0047-OspreyNativeHosts.md)): `version`, `state`, `slices`, `patches`, `removed`, `requests`, `diagnostics` and, at start, `theme`. The host rejects an envelope it cannot decode, an unknown node kind or an unknown capability, and shows the diagnostic. It never draws a fallback.

The start context's `platform` is the operating system as `std::env::consts::OS` spells it (`macos`, `linux`, `windows`), which the framework decodes into its `Platform` union ([UI-CONTEXT-PLATFORM](../../../packages/native/README.md)).

## Rendering [GPUI-HOST-RENDER]

The renderer keeps one GPUI view per region key. A patch replaces that region's node tree; `removed` drops the view and its bindings. A `Slot key` node embeds another region's view. Every stateful node's GPUI element id is its framework key, so focus, scroll offsets, text selection and expanded disclosures survive a patch that keeps the key.

Each `Node` kind has exactly one mapping. The mapping table lives beside the renderer and the plan tracks its completeness; a kind without a mapping is a rejected envelope, not a blank area.

Theme tokens from the start envelope resolve to GPUI colours, type scale and spacing. The renderer follows the system appearance for light and dark.

Every framework symbol ([UI-COMPONENTS-SYMBOLS](../../../packages/native/README.md)) maps to one of the Lucide icons that ship with `gpui-kit`; the host's unit tests fail if a symbol has no icon. An unknown symbol name draws a visible placeholder and a diagnostic. A `pair` node draws its label muted beside its value.

## Events [GPUI-HOST-EVENTS]

A control bound by the framework sends back the binding id it was issued, with its value (`Tap`, `TextInput`, `FlagInput`, `ChoiceInput`, `SizeInput`). The host adds nothing else. Keyboard input reaches Osprey as key events on the region that has focus; the host does not translate keys into actions.

## Frame clock [GPUI-HOST-FRAME]

An application turns the host's frame clock on with an `Animate` request (`{"kind":"animate","on":true,"ticket":…}`) and off with `on` false. The host completes the request like any other capability and, while the clock runs, delivers one event per display refresh:

```json
{"kind":"frame","now":<unix seconds>,"elapsed":<seconds since the previous frame>}
```

The host requests the next animation frame from GPUI after each dispatch, so the clock runs at the display's rate, and it stops while the window is hidden. A frame reaches `update` as the `Frame` member of `Message<A>` with its elapsed seconds; `update` returns the next store and the regions that changed, exactly as for any other event. The host never runs a simulation step of its own.

## Scene [GPUI-HOST-SCENE]

A `Scene` node is a surface in the application's own unit space, `width` by `height`, that the host scales uniformly to fit its region and letterboxes. It is drawn from `shapes`: one small array per primitive, a type code first, so the painter reads it without a schema ([UI-SCENE](../../../packages/native/README.md)):

| Code | Primitive | Values after the code |
| --- | --- | --- |
| 0 | rectangle | x, y (centre), w, h, angle in radians, colour, depth |
| 1 | disc | x, y, radius, colour, depth |
| 2 | line strip | pair count, x0, y0, …, colour, stroke width, depth |
| 3 | sprite | atlas index, x, y, w, h, angle, tint colour, depth |
| 4 | glyphs | text, x, y, size, colour, depth |
| 5 | path | steps, outline, paint, transform, depth (each of the first four an array, below) |

A path's `steps` are arrays in the order a path builder takes them: `[0,x,y]` move, `[1,x,y]` line, `[2,x,y,cx,cy]` quadratic curve (end point first), `[3,x,y,ax,ay,bx,by]` cubic, `[4,rx,ry,rotation,large,sweep,x,y]` elliptical arc, `[5,closed,x0,y0,…]` polygon, `[6]` close. `outline` is `[0]` fill or `[1,width,bevel,dash…]` stroke. `paint` is `[0,tint]` solid or `[1,angle,tint,at,tint,at,oklab]`, a two-stop linear gradient in degrees. A tint is `[0,r,g,b,a]` sRGB or `[1,h,s,l,a]`, float channels from 0 to 1, kept in the application's space so no conversion rounds it. `transform` is `[]` or `[tx,ty,scale]`, applied as a translation then a scale. The host replays the steps on its own path builder in that order and adds no call of its own when the scene is in window pixels, so a path built the same way in a native GPUI program and in Osprey tessellates to the same pixels.

A scene of zero width and height is in **window pixels**: no unit space, no letterbox and no clip, its origin the window's top left, as a GPUI canvas paints. `Ui::canvas` builds one.

A colour is one integer packed `0xRRGGBBAA`; `depth` orders layers from the back; a sprite names its image by index into the node's `atlas`, a list of names the host resolves to textures it owns. The painter draws one pass per depth and instances primitives that share a sprite.

The node carries one binding (`event`). The host sends keys as `{"key":<name>,"down":<bool>}` and the pointer as `{"x":…,"y":…,"down":<bool>,"shift":<bool>}` in scene units through that binding, and nothing else; the application decides what a key or a position means. Key names are host-neutral: `left`, `right`, `up`, `down`, `space`, `enter`, `escape`, `tab`, `backspace`, and a single lowercase character for letters and digits; a key outside that vocabulary is not sent. A pointer event is sent on press, release and on movement while pressed, and once per movement when not pressed, at most once per frame.

A resident application ([UI-RESIDENT](../../../packages/native/README.md)) streams a changed scene's rects, discs and sprites through the `native_shape` import instead of the envelope ([UI-SCENE-STREAM](../../../packages/native/README.md)). The host clears the stream before every `start` and `dispatch`, gives each scene node with a nonzero `streamed` the next run of that many shapes in the encoder's walk order (a screen's toolbar before its children, an aside's pane before its column), and paints the node's own shapes then the streamed ones, stably sorted by depth. A run of 64 or more quads that share a depth paints inside one GPUI layer, so it takes one draw order rather than one bounds-tree insertion per quad; shorter runs, paths, strips and glyphs keep their own orders, so a scene of a few shapes paints exactly as a native GPUI program paints it.

A scene is the only node whose patch may be large every frame. The host measures the time from `dispatch` return to the end of paint and reports it in its diagnostics overlay. The acceptance gate in the plan sets the entity count and frame rate the macOS build must hold; the framework's own per-frame cost is measured by `hosts/gpui/bench` and recorded in the plan.

## Capabilities [GPUI-HOST-CAPABILITIES]

The host answers `Run`, `ListDirectories`, `ReadTexts`, `StoreRead`, `StoreWrite`, `StoreList`, `StoreDelete`, `Http`, `ChooseFile`, `ChooseFolder`, `SaveFile`, `Reveal` and `Cancel` with Rust adapters. Each completion is an event through `dispatch`. A capability the host does not answer is rejected with a diagnostic naming it. Platform-only capabilities that remain in the framework are reached through `HostCall` and answered per host, or rejected.

## Library targets [GPUI-HOST-TARGETS]

A desktop static-library target exists per platform, with the same export selection, generated header and memory rules as the Mac slice of the iOS target:

| `--target` | Built on | Archive contents | The host links |
| --- | --- | --- | --- |
| `macos` | macOS (Xcode) | the application and the Mac ARC runtime, with the Metal adapter | the archive, `libosprey_runtime_macos_arc.a` |
| `linux-x64`, `linux-arm64` | Linux of that architecture | the application and the native ARC runtime (`libfiber_runtime_arc.a`) | the archive, `pthread`, `dl`, `m` |
| `windows-x64` | Windows under MSYS2 UCRT64 | the application and the MinGW-built native ARC runtime | the archive, `ws2_32`, built by MSYS2's UCRT64 Rust (`x86_64-pc-windows-gnu`), so the host, GPUI and the runtime link one UCRT |

A Linux or Windows slice is built on its own system, because it bundles that system's native runtime; asking for one elsewhere is refused with the system to build it on (`--check` still works anywhere). The CLI compiles the IR with `clang -O2 -fPIC` (`OSPREY_CC`) and bundles the runtime with `llvm-ar` (`OSPREY_AR`) (`crates/osprey-cli/src/desktop.rs`). `hosts/gpui/run.sh` picks the target for the system it runs on, and the host's `build.rs` links what the table says. All desktop slices default to ARC.

## Verification [GPUI-HOST-VERIFICATION]

Acceptance requires: the Osprey application's own test suite passing against fixture messages; the host rendering the `packages/native/tests` probe application; a deterministic smoke run that drives the example application through recorded events and asserts the envelope; and a measured frame-rate run of the example application at the gated entity count on an Apple silicon Mac. Linux and Windows acceptance requires the same smoke run on those platforms in CI.
