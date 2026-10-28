# Osprey GPUI host

The [GPUI](https://gpui.rs) host of [Osprey Native](../../packages/native/README.md): a Rust crate that links an Osprey application compiled as a C ABI archive for this system (`macos`, `linux-x64`, `linux-arm64` or `windows-x64`, [GPUI-HOST-TARGETS]), calls its `start`/`dispatch` exports, keeps the framework's keyed regions and draws them with GPUI through [gpui-kit](https://crates.io/crates/gpui-kit). Contract: [spec 0046](../../docs/specs/platforms/0046-GpuiDesktopHost.md). Delivery: [plan 0036](../../docs/plans/0036-gpui-desktop-host.md).

Rust is only the seam. It never interprets an action, holds application state or invents an event; a node kind or capability it does not know is rejected with a visible diagnostic, never drawn as a guess ([GPUI-HOST-SEAM]).

## Build and run

From the repository root, once: on an Apple silicon Mac with Xcode, `make macos` (the Mac ARC runtime, with the Metal adapter, and the compiler); on Linux, `make _runtime` and `cargo build --release -p osprey-cli` with clang, llvm-ar and GPUI's system libraries (see `.github/workflows/gpui.yml`); on Windows, the same under MSYS2 UCRT64 with `CARGO_BUILD_TARGET=x86_64-pc-windows-gnu` and `OSPREY_CC`/`OSPREY_AR` set to its clang and llvm-ar.

Then:

```sh
hosts/gpui/run.sh               # compile the probe app to libOspreyApp.a, build the crate, open the window
hosts/gpui/run.sh --smoke       # start, tap Add, assert "Count 1" through the real boundary; no window
```

`run.sh` compiles the application for the system it runs on. `OSPREY_GPUI_APP=<project dir>` points it at another Osprey Native application whose entry exports `start` and `dispatch` (see [probe/main.ospml](probe/main.ospml), or [Flock's](../../examples/gpui/flock/main.ospml) for a resident session that streams its scene). `OSPREY_BIN` selects another compiler, `OSPREY_GPUI_WINDOW` the window size (`default` or `WxH`), and `--bench <seconds>` prints fps, dispatch, paint and frame time.

## Layout

| Path | Contents |
| --- | --- |
| `probe/` | The probe application: two slices, two regions, a counter, a note and a store read. Namespace `probe` holds the app; `main.ospml` exports only `start` and `dispatch`, so nothing else becomes a C export. |
| `src/ffi.rs` | The C boundary: `osprey_main`, `osprey_app_start`, `osprey_app_dispatch`, the `native_slice` import for host-held slices ([GPUI-HOST-BOUNDARY]) and the `native_shape` import for streamed scene primitives ([UI-SCENE-STREAM]). |
| `src/envelope.rs` | The envelope as `@osprey/native` emits it: `version`, `state`, `slices`, `patches`, `removed`, `requests`, `diagnostics`, `theme` ([NATIVE-HOSTS-TRANSPORT]). |
| `src/regions.rs` | Regions by key, patched from each envelope, with each streamed scene's run of shapes. |
| `src/scene.rs`, `src/scene/` | Scene decoding, the painter (quads, paths, strips, glyphs; same-depth quads in one GPUI layer) and scene input ([GPUI-HOST-SCENE]). |
| `src/clock.rs` | The frame clock and the `--bench` measurements ([GPUI-HOST-FRAME]). |
| `src/store.rs`, `src/adapters/` | Capability adapters: the app store, processes, directories, texts, file panels and reveal ([GPUI-HOST-CAPABILITIES]). |
| `src/events.rs` | The event JSON the framework decodes: `ui`, `completed`, `lifecycle`, and the start context. |
| `src/host.rs` | The window, the dispatch loop and the frame clock. |
| `src/render.rs` | One mapping per `Node` kind to GPUI elements ([GPUI-HOST-RENDER]). |
| `src/smoke.rs` | The `--smoke` run. |
| `bench/` | An Osprey-only measurement of the framework's per-frame cost: a flock of `count` sprites moved by `frames` frame events, timed as a native executable (plan 0036 phase 1). |
| `build.rs` | Links `libOspreyApp.a` (`OSPREY_GPUI_APP_DIR`), plus `libosprey_runtime_macos_arc.a` (`OSPREY_RUNTIME_DIR`) on macOS or the system libraries the bundled runtime calls on Linux and Windows. |

## Measuring the framework alone

```sh
./target/release/osprey build hosts/gpui/bench --memory=arc -o /tmp/gpui-bench && time /tmp/gpui-bench
cd hosts/gpui/bench && ../../../target/release/osprey . --run --memory=arc --profile --quiet   # sampling profile
```

`count`, `frames` and `mode` at the top of `bench/main.ospml` select the flock size, the frame count and what the run times. The numbers and the profile breakdown live in [plan 0036](../../docs/plans/0036-gpui-desktop-host.md), phase 1.

## Status

Plan 0036 phases 0 to 3 are built: the window, renderer, scene painter, frame clock, capabilities and two examples, Flock and a pixel-identical port of GPUI's painting example. Phase 4's Linux and Windows targets and CI are built; their first CI run is pending. Measurements are in the plan and in [Flock's README](../../examples/gpui/flock/README.md).
