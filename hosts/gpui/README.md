# Osprey GPUI host

The [GPUI](https://gpui.rs) host of [Osprey Native](../../packages/native/README.md): a Rust crate that links an Osprey application compiled as a macOS C ABI archive, calls its `start`/`dispatch` exports, keeps the framework's keyed regions and draws them with GPUI through [gpui-kit](https://crates.io/crates/gpui-kit). Contract: [spec 0046](../../docs/specs/platforms/0046-GpuiDesktopHost.md). Delivery: [plan 0036](../../docs/plans/0036-gpui-desktop-host.md).

Rust is only the seam. It never interprets an action, holds application state or invents an event; a node kind or capability it does not know is rejected with a visible diagnostic, never drawn as a guess ([GPUI-HOST-SEAM]).

## Build and run

Apple silicon Mac with Xcode selected. From the repository root, once:

```sh
make macos                      # Mac ARC runtime and the compiler
```

Then:

```sh
hosts/gpui/run.sh               # compile the probe app to libOspreyApp.a, build the crate, open the window
hosts/gpui/run.sh --smoke       # start, tap Add, assert "Count 1" through the real boundary; no window
```

`OSPREY_GPUI_APP=<project dir>` points `run.sh` at another Osprey Native application whose entry exports `start` and `dispatch` under namespace `app` (the shape Vigil uses: see [probe/main.ospml](probe/main.ospml)). `OSPREY_BIN` selects another compiler.

## Layout

| Path | Contents |
| --- | --- |
| `probe/` | The probe application: two slices, two regions, a counter, a note and a store read. Namespace `probe` holds the app; `main.ospml` exports only `start` and `dispatch`, so nothing else becomes a C export. |
| `src/ffi.rs` | The C boundary: `osprey_main`, `osprey_app_start`, `osprey_app_dispatch`, and the `gpui_slice` import for host-held slices ([GPUI-HOST-BOUNDARY]). |
| `src/envelope.rs` | The envelope as `@osprey/native` emits it: `version`, `state`, `slices`, `patches`, `removed`, `requests`, `diagnostics`, `theme` ([NATIVE-HOSTS-TRANSPORT]). |
| `src/regions.rs` | Regions by key, patched from each envelope. |
| `src/events.rs` | The event JSON the framework decodes: `ui`, `completed`, `lifecycle`, and the start context. |
| `src/host.rs` | The window, the dispatch loop and the frame clock. |
| `src/render.rs` | One mapping per `Node` kind to GPUI elements ([GPUI-HOST-RENDER]). |
| `src/smoke.rs` | The `--smoke` run. |
| `bench/` | An Osprey-only measurement of the framework's per-frame cost: a flock of `count` sprites moved by `frames` frame events, timed as a native executable (plan 0036 phase 1). |
| `build.rs` | Links `libOspreyApp.a` (`OSPREY_GPUI_APP_DIR`) and `libosprey_runtime_macos_arc.a` (`OSPREY_RUNTIME_DIR`). |

## Measuring the framework alone

```sh
./target/release/osprey build hosts/gpui/bench --memory=arc -o /tmp/gpui-bench && time /tmp/gpui-bench
cd hosts/gpui/bench && ../../../target/release/osprey . --run --memory=arc --profile --quiet   # sampling profile
```

`count`, `frames` and `mode` at the top of `bench/main.ospml` select the flock size, the frame count and what the run times. The numbers and the profile breakdown live in [plan 0036](../../docs/plans/0036-gpui-desktop-host.md), phase 1.

## Status

Plan 0036 phase 0: the boundary and the smoke run work; the window and renderer are being built. Phase 1: the framework side (`Frame`, `Animate`, `Scene`, scene input) is in `packages/native` and measured; the scene painter and frame clock in this crate are not built. Capabilities beyond `Animate`, Linux and Windows are not built; see the plan for order and gates.
