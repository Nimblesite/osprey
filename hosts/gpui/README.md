# Osprey GPUI host

The [GPUI](https://gpui.rs) host of [Osprey Native](../../packages/native/README.md): a Rust crate that links an Osprey application compiled as a C ABI archive for this system (`macos`, `linux-x64`, `linux-arm64` or `windows-x64`, [GPUI-HOST-TARGETS]), calls its `start`/`dispatch` exports, keeps the framework's keyed regions and draws them with GPUI through [gpui-kit](https://crates.io/crates/gpui-kit). Contract: [spec 0046](../../docs/specs/platforms/0046-GpuiDesktopHost.md). Delivery: [plan 0036](../../docs/plans/0036-gpui-desktop-host.md).

Rust is only the seam. It never interprets an action, holds application state or invents an event; a node kind or capability it does not know is rejected with a visible diagnostic, never drawn as a guess ([GPUI-HOST-SEAM]).

## Build and run

From the repository root, once: on an Apple silicon Mac with Xcode, `make -C apps macos` (the Mac ARC runtime, with the Metal adapter, and the compiler); on Linux, `make _runtime` and `cargo build --release -p osprey-cli` with clang, llvm-ar and GPUI's system libraries (see `.github/workflows/gpui.yml`); on Windows, the same under MSYS2 UCRT64 with `OSPREY_CC`/`OSPREY_AR` set to its clang and llvm-ar, and the host built by MSYS2's own Rust (`mingw-w64-ucrt-x86_64-rust`) ahead of rustup's on `PATH`. Its build scripts run as GNU programs and everything links one UCRT; rustup's Rust with the GNU target builds GPUI's resource script as an MSVC library and links msvcrt beside the runtime's UCRT. `run.sh --build` builds without running, for a host started some other way. `run.sh --compile` compiles only the application's archive.

Then:

```sh
hosts/gpui/run.sh               # compile the probe app to libOspreyApp.a, build the crate, open the window
hosts/gpui/run.sh --compile     # compile only the probe app's archive
hosts/gpui/run.sh --smoke       # start, tap Add, assert "Count 1" through the real boundary; no window
```

`run.sh` compiles the application for the system it runs on unless `OSPREY_GPUI_PREBUILT=1` selects an archive already compiled for that application. `make _gpui-apps` compiles all six desktop application archives in parallel; `_gpui-smoke` and `_gpui-self-test` depend on it and reuse those archives, so one make invocation compiles each application once. `OSPREY_GPUI_APP=<project dir>` points `run.sh` at another Osprey Native application whose entry exports `start` and `dispatch` (see [probe/main.ospml](probe/main.ospml), or [Flock's](../../examples/gpui/flock/main.ospml) for a resident session that streams its scene). `OSPREY_BIN` selects another compiler, `OSPREY_GPUI_WINDOW` the window size (`default` or `WxH`), and `--bench <seconds>` prints fps, dispatch, paint and frame time.

`python3 hosts/gpui/dependencies.py` stages the dependency backports declared in [dependencies.json](dependencies.json). It verifies each published archive, applies the exact preimages in [patches/](patches/), and checks the complete resulting source tree. Both this host and the Painting reference must declare the same inventory. A successful run prints `GPUI dependency source integrity: N backports PASS`, where `N` equals the inventory size. Within each declared cached crate, modified, missing or additional source files and symlinks are refused. Invalid package paths and mismatched workspace overrides are also refused; unrelated sibling cache directories are outside this inventory. `run.sh` and `make audit` run this check automatically.

`python3 hosts/gpui/dependencies_test.py` checks the staging and refusal contracts, including concurrent staging and verification of the winning source tree. `make -C apps/macos test` and `make audit` run these guards before building or scanning; `run.sh` only stages the sources, so running an app runs no tests. Dependency repairs must also pass the unchanged repository audit and host/rendering checks; staging verifies source identity, while the two workspace lockfiles describe the dependencies scanned by `make audit`.

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

Plan 0036 phases 0 to 3 are built: the window, renderer, scene painter, frame clock, capabilities and two examples, Flock and a pixel-identical port of GPUI's painting example. Phase 4 CI runs every native, UI and example suite on Linux, where a separate host job runs the host tests, clippy and every app's headless smoke and self-test. The Mac runs every scripted app in a real window, clippy, and its archive and string-ownership tests. Windows runs the probe smoke and opens Flock and Painting in real windows on the runner's desktop. Measurements are in the plan and in [Flock's README](../../examples/gpui/flock/README.md).
