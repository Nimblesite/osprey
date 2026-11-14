# Osprey GPUI host

The [GPUI](https://gpui.rs) host of [Osprey Native](../../../osprey/packages/native/README.md): a Rust crate that links an Osprey application compiled as a C ABI archive for this system (`macos`, `linux-x64`, `linux-arm64` or `windows-x64`, [GPUI-HOST-TARGETS]), calls its `argument`, `open` and `event` exports, leases the frame each call writes and keeps the framework's keyed regions, drawing them with GPUI through [gpui-kit](https://crates.io/crates/gpui-kit). Contract: [spec 0046](../../../../docs/specs/platforms/0046-GpuiDesktopHost.md). Delivery: [plan 0037](../../../../docs/plans/0037-osprey-native.md).

Rust is only the seam. It never interprets an action, holds application state or invents an event; a node kind or capability it does not know is rejected with a visible diagnostic, never drawn as a guess ([GPUI-HOST-SEAM]).

## Build and run

From the repository root, once: on an Apple silicon Mac with Xcode, `make -C src/toolchain/targets/macos examples` (the Mac ARC runtime, with the Metal adapter, and the compiler); on Linux, `make _runtime` and `cargo build --release --manifest-path src/toolchain/Cargo.toml -p osprey-cli` with clang, llvm-ar and GPUI's system libraries (see `.github/workflows/gpui.yml`); on Windows, the same under MSYS2 UCRT64 with `OSPREY_CC`/`OSPREY_AR` set to its clang and llvm-ar, and the host built by MSYS2's own Rust (`mingw-w64-ucrt-x86_64-rust`) ahead of rustup's on `PATH`. Its build scripts run as GNU programs and everything links one UCRT; rustup's Rust with the GNU target builds GPUI's resource script as an MSVC library and links msvcrt beside the runtime's UCRT. `run.sh --build` builds without running, for a host started some other way. `run.sh --compile` compiles only the application's archive.

Then:

```sh
src/toolchain/hosts/gpui/run.sh               # compile the probe app to libOspreyApp.a, build the crate, open the window
src/toolchain/hosts/gpui/run.sh --compile     # compile only the probe app's archive
src/toolchain/hosts/gpui/run.sh --smoke       # start, tap Add, assert "Count 1" through the real boundary; no window
```

`run.sh` compiles the application for the system it runs on unless `OSPREY_GPUI_PREBUILT=1` selects an archive already compiled for that application. `make _gpui-apps` compiles all six desktop application archives in parallel; `_gpui-smoke` and `_gpui-self-test` depend on it and reuse those archives, so one make invocation compiles each application once. `OSPREY_GPUI_APP=<project dir>` points `run.sh` at another Osprey Native application whose entry exports `argument`, `open` and `event` (see [probe/main.ospml](probe/main.ospml), or [Flock's](../../../osprey/examples/gpui/flock/main.ospml) for a resident session that streams its scene). `OSPREY_BIN` selects another compiler, `OSPREY_GPUI_WINDOW` the window size (`default` or `WxH`), and `--bench <seconds>` prints fps, dispatch, paint and frame time.

`python3 src/toolchain/hosts/gpui/dependencies.py` stages the dependency backports declared in [dependencies.json](dependencies.json). It verifies each published archive, applies the exact preimages in [patches/](patches/), and checks the complete resulting source tree. Both this host and the Painting reference must declare the same inventory. A successful run prints `GPUI dependency source integrity: N backports PASS`, where `N` equals the inventory size. Within each declared cached crate, modified, missing or additional source files and symlinks are refused. Invalid package paths and mismatched workspace overrides are also refused; unrelated sibling cache directories are outside this inventory. `run.sh` and `make audit` run this check automatically.

The verified source set determines a content-addressed directory under `target/gpui-dependencies`. Files within that directory use a fixed timestamp so identical restaging reuses Cargo artifacts. A changed source set gets a different Cargo package identity and rebuilds. Update the shared directory prefix in both Cargo manifests when changing the pinned source set.

`python3 src/toolchain/hosts/gpui/dependencies_test.py` checks the staging and refusal contracts, including concurrent staging and verification of the winning source tree. `make _gpui-test` and `make audit` run these guards before building or scanning; `run.sh` only stages the sources, so running an app runs no tests. Dependency repairs must also pass the unchanged repository audit and host/rendering checks; staging verifies source identity, while the two workspace lockfiles describe the dependencies scanned by `make audit`.

The Cargo integration test builds a consumer, restages identical source and checks that its binary is reused, then changes the same dependency version's bytes and checks that the rebuilt binary executes the new value.

Run `make -C src/toolchain/targets/macos/tests painting` or `flock` for the corresponding full windowed script. Each `shot` waits for a rendered frame and writes a PNG plus an accessibility text snapshot under `target/gpui-self-test/`. The desktop must be unlocked and rendering: a stale initial picture or text assertions alone do not verify the window. Headless runs retain accessibility snapshots only. PNG capture does not yet compare against committed pixel goldens.

Every application keeps its session in its own memory and draws each frame as view, scene and ask rows the host leases and reads in place ([NATIVE-SHM-VIEW], [NATIVE-SHM-SCENE], [NATIVE-SHM-ASK]); events go back as typed calls ([NATIVE-SHM-EVENT]). Nothing crosses as JSON; [shared-memory views](../../../../docs/specs/applications/0053-NativeSharedViews.md) explains the frame end to end. A component application tells its semantic tree as typed rows ([NATIVE-SHM-NODES]), which the host reads back as the same tree main's renderer drew, and a frame of plain view rows draws from those rows.

## Layout

| Path | Contents |
| --- | --- |
| `probe/` | The probe application: two slices, two regions, a counter, a note and a store read. Namespace `probe` holds the app; `main.ospml` exports only `argument`, `open` and `event`, so nothing else becomes a C export. |
| `src/ffi.rs` | The C boundary: `osprey_main`, `osprey_app_argument`, `osprey_app_open`, `osprey_app_event` and the `native_slice` import for host-held slices ([GPUI-HOST-BOUNDARY]). |
| `src/frame.rs` | Each call's frame, sealed and leased from the bridge: scene, view, text and ask runs read in place. |
| `src/rows.rs` | Every row checked and read as typed controls, told components, scenes and asks; a malformed frame is refused whole ([NATIVE-SHM-SAFETY]). |
| `src/tree.rs` | The component tree told rows carry: a typed value, a record and a list, and the theme ([NATIVE-SHM-NODES]). |
| `src/regions.rs` | Every region the frame told, its theme and the control numbers it issues ([GPUI-HOST-RENDER]). |
| `src/render.rs`, `src/render/` | The semantic component mapping to GPUI, restored from main: one mapping per component kind, every colour and space taken from the theme ([GPUI-HOST-RENDER]). |
| `src/scene.rs`, `src/scene/` | The painter (quads, paths, strips, glyphs; same-depth quads in one GPUI layer) and scene input ([GPUI-HOST-SCENE]). |
| `src/clock.rs` | The frame clock and the `--bench` measurements ([GPUI-HOST-FRAME]). |
| `src/store.rs`, `src/adapters/` | Capability adapters: the app store, processes, directories, texts, file panels and reveal ([GPUI-HOST-CAPABILITIES]). |
| `src/events.rs` | `Touch`: the typed host events `osprey_app_event` takes. |
| `src/host.rs` | The window, the event loop and the frame clock. |
| `src/view.rs`, `src/view/` | One mapping per plain view row kind to GPUI elements, every style taken from the row: the frames that tell no components ([GPUI-HOST-RENDER]). |
| `src/screen.rs` | What a frame shows by the words a person reads: the control a label names, a list's choices, a scene's elements and the accessibility tree ([UI-MACHINE-SELF-TEST]). |
| `src/smoke.rs` | The `--smoke` run. |
| `bench/` | An Osprey-only measurement of the framework's per-frame cost: a flock of `count` sprites moved by `frames` frame events, timed as a native executable (plan 0037). |
| `build.rs` | Links `libOspreyApp.a` (`OSPREY_GPUI_APP_DIR`), plus `libosprey_runtime_macos_arc.a` (`OSPREY_RUNTIME_DIR`) on macOS or the system libraries the bundled runtime calls on Linux and Windows. |

## Measuring the framework alone

```sh
./target/release/osprey build src/toolchain/hosts/gpui/bench --memory=arc -o /tmp/gpui-bench && time /tmp/gpui-bench
cd src/toolchain/hosts/gpui/bench && ../../../../target/release/osprey . --run --memory=arc --profile --quiet   # sampling profile
```

`count` and `frames` at the top of `bench/main.ospml` select the flock size and the frame count; each frame goes through `Hosting::event` and its rows are counted. 120 frames of 5,000 sprites take 0.87 s (about 7 ms a frame, `update` included) on an M-series Mac. The numbers and the profile breakdown live in [plan 0037](../../../../docs/plans/0037-osprey-native.md), phase 1.

## Status

The original host phases of plan 0037 provide the window, renderer, scene painter, frame clock, capabilities and the Flock and Painting examples. The component tree crosses as typed rows that main's renderer draws unchanged. Phase 4 CI runs the native, UI and example suites on Linux, with a separate host job for host tests, clippy and app smoke/self-tests. macOS and Windows also have windowed app scripts; their successful execution requires an interactive desktop. The latest local macOS run was blocked by a locked desktop after the screenshot check was strengthened to require a fresh frame. Measurements are in the plan and in [Flock's README](../../../osprey/examples/gpui/flock/README.md).
