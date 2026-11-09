# Plan 0037 — Osprey Native hosts

Contract: [spec 0047](../specs/applications/0047-OspreyNativeHosts.md), [spec 0046](../specs/platforms/0046-GpuiDesktopHost.md), [spec 0048](../specs/platforms/0048-WinUiHost.md), [AGENTS.md § Apps](../../AGENTS.md#apps).

Depends on: [plan 0023](0023-gpu.md) (its Graphics plumbing section puts the scenes in the WinUI host).

## Prerequisites in place

- GPUI, WinUI and in-repo SwiftUI hosts, Storage/Flock/Painting, the UI-machine framework and the sandboxed-WinUI-with-goldens CI all run (spec 0046, spec 0048, UiMachines).
- The `Symbol` and `Platform` unions are typed and host-neutral ([UI-COMPONENTS-SYMBOLS], [UI-CONTEXT-PLATFORM]); `HostCall` exists and the native suite encodes one ([UI-EFFECTS]).

## 1. Finish framework neutrality [NATIVE-HOSTS-NEUTRAL]

- [ ] Remove `ReadPropertyLists`, `Fingerprint`, `RunningApplications`, `OpenSettings` from `Capability` and `Requests::members` in `src/osprey/packages/native/src/capability.ospml`, and remove `Run`'s `plist` field; product work travels as `HostCall`. Drop `read-plists`, `fingerprint`, `running-applications`, `open-settings` from `src/toolchain/hosts/gpui/src/coverage.rs` (UNANSWERED) and `src/toolchain/hosts/winui/OspreyHost.Core/Coverage.cs` (Unanswered). Check: `osprey src/osprey/packages/native/tests --run --memory=arc` and `--memory=gc` green; none of the four constructors remain in `capability.ospml`.
- [ ] Remove `Evidence` and fold it into `Pair` (`Ui::pair`, wire `pair`): delete `Evidence` from `Node` in `src/osprey/packages/native/src/node.ospml`, delete `Ui::evidence` and the `encode.ospml` arm, and drop `evidence` from GPUI `coverage.rs` REFUSED and WinUI `Coverage.cs` Refused. Check: `pair` is in both hosts' drawn lists (already is) and `grep evidence src/osprey/packages/native/src` is empty.
- [ ] Delete `Brand` from `Node`/`Ui`/`encode.ospml`; `Keyed::region`'s placeholder stays `Spacer` (`src/osprey/packages/native/src/reactive.ospml` already uses it); drop `brand` from GPUI `coverage.rs` REFUSED and WinUI `Coverage.cs` Refused. Check: `grep brand src/osprey/packages/native/src` is empty; `cargo test coverage` and WinUI `CoverageTests` green.

## 2. Conformance suite [NATIVE-HOSTS-CONFORMANCE]

- [ ] Add `src/osprey/packages/native/conformance/`: the probe application's start context, a recorded event sequence and the expected envelopes, produced once from `Runtime::start`/`dispatch` and checked in.
- [ ] Add an Osprey test that regenerates the envelopes from the fixtures and diffs them, so a framework change that alters the wire shape fails here first. Check: the test lives under `src/osprey/packages/native/tests` and fails on a wire change.
- [ ] Wire each host's smoke to run the set: SwiftUI in Vigil's smoke, webview in `npm test --prefix src/toolchain/vscode-extension`, GPUI in `src/toolchain/hosts/gpui/run.sh --smoke`, WinUI in its smoke. Check: each smoke passes the conformance set.

## 3. Coverage matrix [NATIVE-HOSTS-COVERAGE]

- [ ] Add a script that reads `src/osprey/packages/native/src/node.ospml` and `capability.ospml` for constructor names plus each host's mapping table (`src/toolchain/hosts/gpui/src/coverage.rs`, `src/toolchain/hosts/winui/OspreyHost.Core/Coverage.cs`, `src/toolchain/vscode-extension/client/src/osprey-ui/render.ts` + `capabilities.ts`), and writes `src/osprey/packages/native/COVERAGE.md`.
- [ ] Make CI regenerate it and fail on a diff, so a new constructor cannot land without every host's cell.
- [ ] Each host README links its column.

## 4. Shared web renderer [NATIVE-HOSTS-FAMILY]

- [ ] Extract `osprey-ui` (`protocol.ts`, `render.ts`, `styles.ts`, `theme.ts`, `icons.ts`, `session.ts`, `wasi.ts`) from `src/toolchain/vscode-extension/client/src/osprey-ui` into `packages/web` as an npm package; `capabilities.ts` and the webview plumbing stay in the extension. Check: `vscode-extension` still builds and its renderer suite is green.
- [ ] Add a browser host page that loads the WASI module in a worker and renders into a `div`, with file and store capabilities on the browser APIs that exist.
- [ ] Run the probe application in a browser tab and pass the conformance set.

## 5. Storage on GPUI, plus the two unrecorded measurements

- [ ] Add the GPUI `MeasureFolders` adapter `src/toolchain/hosts/gpui/src/adapters/measure.rs` ([UI-EFFECTS-MEASURE]): Rust on `std::fs`, `st_blocks` and `(st_dev, st_ino)` on Unix, the Windows rules from spec 0048's folder walk. Tests on a temporary tree: hidden file, symbolic link, a hard link counted once, an unreadable folder, deadline, cancel. Move `measure-folders` from UNANSWERED to ANSWERED in `coverage.rs`. Check: `cargo test --manifest-path src/toolchain/hosts/gpui/Cargo.toml measure`.
- [ ] Map `split` and `context-menu` in `src/toolchain/hosts/gpui/src/render.rs` (+ `render/controls.rs`) so Storage's root split and row menus draw; move them from REFUSED to DRAWN in `coverage.rs`. Check: `cargo test coverage`; `make -C src/toolchain/targets/macos storage` opens with no diagnostics.
- [ ] Storage acceptance: `./target/release/osprey test src/osprey/examples/storage/test` green on every OS; no host source names Storage ([NATIVE-HOSTS-NEUTRAL]).
- [ ] Record flat memory over ten minutes of Flock in the GPUI window; put the numbers in `src/toolchain/hosts/gpui/README.md`.
- [ ] Record a focused-window fps reading at 5,000 birds with the screen unlocked (M-series Mac); put it in `src/osprey/examples/gpui/flock/README.md`.

## 6. Storage: Windows walk, shortcuts, elevation [UI-EFFECTS-MEASURE] [UI-COMPONENTS-SHORTCUT]

- [ ] GPUI Windows walk: capacity from `GetDiskFreeSpaceExW` (available after quotas); one `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` batch per directory; hidden is `FILE_ATTRIBUTE_HIDDEN`; only `IO_REPARSE_TAG_SYMLINK` and `IO_REPARSE_TAG_MOUNT_POINT` are links; `WOF` files count allocated bytes and `APPEXECLINK` 0; cloud placeholders never opened (`CLOUD_*`, `RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN`, `OFFLINE`); hard links once by volume serial + 128-bit file id; `ERROR_ACCESS_DENIED` adds one to `unreadable`. Run a shared fixture tree with expected JSON against both the Rust and C# walkers. Check: both adapters produce the same JSON.
- [ ] Add `Shortcut`/`Chord` to `src/osprey/packages/native/src/node.ospml` ([UI-COMPONENTS-SHORTCUT]): a `Button` may carry a typed `Chord`; `Primary` is Command on Apple systems, Control elsewhere; the host shows the chord in its menu or tooltip; Osprey picks chords per platform (Alt+Up, F5, Ctrl+O on Windows; Command+Up, Command+R, Command+O on the Mac).
- [ ] Elevation on Windows: `HostCall` in the `windows` namespace for "restart as administrator", answered by the WinUI host with `ShellExecuteExW`/`runas` and `--elevated`; Osprey offers it only on Windows when the walk hit unreadable folders and the process is not elevated. Check: a Storage self-test drives it.
- [ ] Measure a full `C:` drive (walk time, peak memory, gap vs used capacity) and record in `src/osprey/examples/storage/README.md`.

## 7. WinUI remaining: adapters, C fixture host, 60 Hz gate, packaging

- [ ] Answer the remaining WinUI capabilities ([WINUI-HOST-CAPABILITIES]): `list-directories`, `read-texts`, `save-file`, `http` (on `HttpClient`) and the `windows` `HostCall` for elevated restart; move them from `Unanswered` to `Answered` in `src/toolchain/hosts/winui/OspreyHost.Core/Coverage.cs`. Check: `dotnet test --project OspreyHost.Tests` (from PowerShell).
- [ ] Add the MinGW C fixture host that loads the probe's DLL, binds both imports and calls both exports; run it in `ci-windows.yml` ([WINUI-HOST-DLL]).
- [ ] Measure whether WinUI shapes hold Flock's 5,000 birds at 60 Hz on a named Windows machine; record it ([WINUI-HOST-SCENE]).
- [ ] MSIX: sign a real `.msix` with a test certificate in CI (today `app.ps1` registers a loose layout via `Add-AppxPackage -Register`). Check: CI produces and installs the signed package.
- [ ] Add a `windows-arm64` slice (MSYS2 CLANGARM64) and a `win-arm64` publish. Check: `--target` accepts `windows-arm64`; the host publishes for arm64.

## 8. SwiftUI host, and retiring the old Storage app

- [ ] One SwiftUI host, not two: `src/osprey/workbench/hosts/apple/Sources/OspreyNative` is a diverged copy of `src/toolchain/hosts/swift/Sources/OspreyNative` (Capabilities, Store, Process, Network and Log adapters, Layout, Controls), so #355's store fix had to land twice. Move the shared sources into one Swift package both apps depend on; the workbench keeps only its own views (Film, Math, Cards). Check: `diff -rq` finds no file in both trees, and both `swift test` suites pass.
- [ ] Extend `src/toolchain/hosts/swift` (the in-repo SwiftUI host) to macOS and iOS, map every node and symbol (SF Symbols) and every capability including `measure-folders`, and run the conformance set. Check: Storage runs on `src/toolchain/hosts/swift` with no diagnostics.
- [ ] Move Storage's Apple-only requests to `HostCall`s only this host answers: Photos summary `HostCall "photos" "summary"`, Full Disk Access `HostCall "macos" "privacy-settings"`.
- [ ] Retire `src/osprey/examples/macos` once Storage runs on GPUI, WinUI and SwiftUI; move every link to `src/osprey/examples/storage`; keep a website redirect.

## 9. Issue Inbox [UI-COMPONENTS-RICH]

- [ ] Every host completes `Http` with its platform HTTPS client within the framework's limits.
- [ ] Add `Rich` ([UI-COMPONENTS-RICH]): a paragraph of styled runs — strong, emphasis, code, link; markdown parsing stays in Osprey (`markdown.ospml`).
- [ ] Move Inbox's SQL tables to JSON documents in the app store via `StoreRead`/`StoreWrite`; each query becomes an Osprey list function.
- [ ] Add `hosts/compose` (Android) from today's `NativeRenderer.kt`; delete Inbox's own protocol, `NativeRenderer.swift` and `NativeRenderer.kt`; Inbox runs on GPUI, WinUI, SwiftUI and Compose.

## 10. Remaining apps [NATIVE-APPS-ALL]

- [ ] iPhone counter on `src/toolchain/hosts/swift`.
- [ ] Talon Bank's Android and iOS apps on the Compose and SwiftUI hosts; Talon Bank's web client on the browser host.
- [ ] Graphics scenes in the WinUI host: `OspreyHost.exe --scene` draws into a `SwapChainPanel` and the Win32 window code (`src/osprey/examples/graphics/ospgfx_d3d12_window.c`) is deleted — [plan 0023](0023-gpu.md) Graphics plumbing. Check: `src/toolchain/targets/windows/apps.json` gains a `graphics` entry.
- [ ] Counter, Inbox and Workbench on Osprey Native in the WinUI host as AppContainer packages (after the Inbox port); add entries to `src/toolchain/targets/windows/apps.json` with the capabilities listed in AGENTS.md § Apps.
- [ ] Bank and GPT-2 sandboxed in AppContainers launched by the apps Makefile.

## 11. Replace the JSON bridge with shared memory [NATIVE-SHM]

The request, verbatim: "replace the json bridge with SHARED MEMORY, in a similar style to how React Native removed the JSON"; "the types must be typeDiagram on both sides"; "If we need to do any serialisation at all, we use tdbin, but we AVOID serialisation unless absolutely necessary. The aim is FULL SHARED MEMORY".

### Today

Every `dispatch` encodes and parses text on both sides of the boundary:

- `osprey_app_start` and `osprey_app_dispatch` take and return C strings ([src/toolchain/hosts/gpui/src/ffi.rs](../../src/toolchain/hosts/gpui/src/ffi.rs)), and the `saved` application state goes round the boundary as text on every event.
- `src/osprey/packages/native/src/encode.ospml` writes each node as JSON. `src/toolchain/hosts/gpui/src/envelope.rs`, `src/toolchain/hosts/gpui/src/scene/decode.rs`, `src/toolchain/hosts/swift/Sources/OspreyNative/Protocol.swift` and `JSONValue.swift` parse it again, and the WinUI and webview hosts each have their own parser.
- Renderers dispatch on kind strings, requests are `[String: JSONValue]` dictionaries, and `native_slice` looks up host text by string key under a mutex.
- `native_shape` already avoids JSON for scene primitives, but pays one C call and one `Mutex<Vec<_>>` push per shape.

### What React Native did, and where this goes further

React Native [removed its asynchronous bridge and replaced it with JSI](https://reactnative.dev/architecture/landing-page): each side holds a memory reference to the other's objects and calls methods "without serialization costs". [Fabric](https://reactnative.dev/architecture/fabric-renderer) moved from serialized JSON to direct access, shares one renderer core across platforms and made layout synchronous and thread-safe. [Codegen](https://reactnative.dev/docs/the-new-architecture/what-is-codegen) generates the glue from typed specs at build time.

The same moves apply here: references in place of messages, one typed schema generating both sides, and synchronous access. Osprey can go further because it has no JavaScript VM in the way. The compiler controls how values are laid out, so host and application can read the same bytes through the same generated layout, with no host-object wrapper between them.

### Design decisions

- **One address space where it exists.** Native hosts link the application into their own process, so "shared memory" on macOS, iOS, Android, Linux and Windows means the same pointers. No `mmap`, no IPC and no copying are needed to cross. In a browser host, the wasm `WebAssembly.Memory` is the shared memory, and it is backed by a `SharedArrayBuffer` when the host reads it from another thread.
- **typeDiagram is the only definition.** `schemas/native.td` declares nodes, commits, events, requests, completions, themes and scene columns. Osprey, Rust, Swift, C# and TypeScript views are generated from it at build time and never written by hand. `node.ospml`'s hand-written types and every host's parser go.
- **The in-memory layout is tdbin's layout.** Values are built in place using [tdbin's word layout](https://typediagram.dev/docs/tdbin-wire-format.html): schema-fixed data and pointer sections, little-endian 64-bit words and relative offsets. Relative offsets mean a value reads the same at any base address, including a wasm offset. There is no encode or decode step in-process, and if bytes ever must leave the process, a canonical tree is already tdbin. That gives one layout rather than a shared-memory layout plus a wire layout, which would be duplication.
- **Serialisation happens only at a real process boundary, and always as tdbin.** The boundary known today is the VS Code webview, a separate process that cannot map the extension host's memory. Saved state on disk is the other case. JSON stays only where an outside party defines it: HTTP APIs, model providers, MCP, the Cast protocol, and files people edit such as `.mcp.json`. Those are payloads for an adapter, and they never cross the bridge as JSON.
- **The bridge never parses.** Verification is one O(n) pass ([tdbin](https://typediagram.dev/docs/tdbin-wire-format.html)), so it runs only on bytes from outside the process, never on a commit from the linked application. The schema hash is checked once at load instead, and a mismatch refuses to start.
- **Named copies are allowed where a platform forces them, and only there.** `paint_quad` and `paint_image` in GPUI, SwiftUI `String` and XAML's UTF-16 strings all take their own values. Each such copy is listed in `src/osprey/packages/native/README.md` with its byte count, and a copy that is not listed fails the gate below.

### Techniques

| Data | Technique |
| --- | --- |
| A commit (patches, removals, slices, requests) | An immutable tree in a bump-allocated generation arena, published with one release-store of a root offset. A changed region allocates; an unchanged subtree is an offset into an older generation, so diffing a retained region compares offsets. |
| Application state | Stays in Osprey's memory as a handle. It is not passed round the boundary, and is written as tdbin only when saved. |
| Host to application, off the main thread | Latest-wins triple buffer for the newest commit, so a slow frame skips stale commits instead of queueing them. Generation refcounts reclaim arenas, with `MADV_FREE` or `MEM_RESET` on retired pages. |
| Events | A single-producer, single-consumer ring of fixed-size slots of the generated event union, cache-line separated, with batched index publication ([Tachyon](https://github.com/riyaneel/Tachyon) does this). Pointer moves coalesce in place. |
| Large payloads (files, HTTP bodies, media) | Only a descriptor (block, length) crosses the ring and the bytes stay put in a refcounted block pool ([crossbar](https://docs.rs/crate/crossbar/0.8.0/source/ARCHITECTURE.md) does this). Files are memory-mapped rather than read. |
| Text | UTF-8 views with a length and no NUL. Repeated keys and tokens are interned, so a binding or token is an integer rather than a string. |
| Scene primitives | Structure-of-arrays columns written directly by the application. The columns are the GPU instance data where the renderer owns its buffers (`newBufferWithBytesNoCopy` on Metal, a mapped buffer on wgpu). |
| Authored machine documents | `player.machine.json` is compiled at build time to tdbin in read-only data, so launch parses nothing. |
| Synchronous queries (measure, hit test) | A direct generated call that returns in the same frame, as Fabric's synchronous layout does. |

### Steps

Each surface is cut over on every host in one change, then its JSON path is deleted. There is no dual mode and no flag.

**Measure first**

- [ ] Baseline with the existing instruction-count harness on Storage with a large tree, Flock at 5,000 birds, Casting playback and a long Osprey Agent transcript: instructions, bytes parsed, bytes copied and heap allocations per `dispatch`, plus p50 and p99 frame time on the Mac. Check: the numbers are in `src/osprey/packages/native/README.md`; targets below are set from them, not guessed.
- [ ] Spike one patch end to end, Osprey to GPUI, to prove in-place Osprey writes coexist with ARC and GC: arena values are not collector objects, and an arena value may not reach into the managed heap. Decide how this meets [plan 0045](0045-compile-time-ownership.md), and report before building on it. Check: `--memory=arc` and `--memory=gc` both pass under ASan.
- [ ] Spike the reservation: one contiguous virtual range per application so that relative offsets cross generations. Measure what iOS and Android allow without extra entitlements, and the limit on wasm32. Check: a table of the size chosen for each target.

**Schema and generators**

- [ ] Update [spec 0047](../specs/applications/0047-OspreyNativeHosts.md) [NATIVE-HOSTS-TRANSPORT] and [spec 0046](../specs/platforms/0046-GpuiDesktopHost.md) [GPUI-HOST-BOUNDARY] to this contract, with `[NATIVE-SHM-*]` IDs cross-referenced from code and tests.
- [ ] Declare `schemas/native.td`, covering everything in `node.ospml`, `capability.ospml`, `message.ospml` and the theme.
- [ ] typeDiagram gaps, upstream: the language reference does not name Swift or Osprey as generation targets, tdbin generation exists only for Rust and partly for TypeScript, `[TDBIN-FUTURE-READER]` is future work, kinds 3 and 4 for narrower numerics are reserved (scene columns need `f32`), and generators neither compute nor enforce the schema hash. Add generators for Osprey, Rust, Swift, C# and TypeScript views, narrower numerics, generated hash enforcement and the zero-copy reader. Check: one generated fixture reads the same on all five, from the same bytes.
- [ ] Decide with typeDiagram how older-generation offsets fit tdbin, whose verifier rejects aliased pointer graphs by design. In-process trees may share; whatever leaves the process is first written out canonically. Check: a shared tree written out verifies, and matches the canonical bytes of the same value built fresh.
- [ ] Compile-time layout checks on each side: `size_of`, offsets and the schema hash. Check: a schema change without regenerating fails the build, and a mismatched archive refuses to start with a readable message.

**Cut over, one surface at a time**

- [ ] Commits and patches: replace `osprey_app_start`/`osprey_app_dispatch` strings with a typed event in and a root offset out, and match on discriminants. Delete `encode.ospml`, `envelope.rs`, `Protocol.swift`, `JSONValue.swift`, `osprey_release_string` and the WinUI equivalents.
- [ ] State: hold it as a handle and remove `saved` from the call. Restart reads tdbin from the store.
- [ ] Slices: replace `native_slice` and its mutex-guarded string map with typed views in the commit.
- [ ] Events and capabilities: replace ticketed JSON requests with the generated union, and move completions and large payloads to the ring and block pool.
- [ ] Scene: replace per-shape `native_shape` calls and `scene/decode.rs` with columns. Delete the `STREAM` mutex.
- [ ] Machine documents: compile them to tdbin at build time, and delete the runtime JSON loading in `machine_doc.ospml`.
- [ ] Threads: move Osprey off the main thread only if the baseline shows that it helps, using the triple buffer and event ring. Every export is on the main thread today, so the first cut needs no synchronization at all.
- [ ] Web: the browser host reads wasm memory through generated views. The VS Code webview takes tdbin over `postMessage` with transferred buffers. Replace WASI's JSON frames. Check: conformance passes in both.

**Prove it**

- [ ] Conformance ([NATIVE-HOSTS-CONFORMANCE], section 2) compares canonical tdbin bytes of each commit, deterministic by the format's rules, instead of JSON envelopes. Check: all hosts pass one fixture set.
- [ ] Self-test and smoke scripts stay readable files on disk and the test driver turns them into typed events; they never cross the bridge as JSON.
- [ ] Lifetime tests: debug builds `mprotect` retired generations, so a stale host read crashes the test rather than reading garbage. Run journeys under ASan and TSan, the Rust view under Miri, publication under `loom`.
- [ ] Fuzz the tdbin verifier at the process boundaries with hostile pointers, depth and amplification. Check: typed errors, never a panic.
- [ ] No-JSON gate in CI: a counting allocator and a parse counter assert zero parsing and zero unlisted copies across the bridge in every app's self-test, and a source check rejects JSON types in the bridge paths of `src/osprey/packages/native` and `src/toolchain/hosts/`. Adapters for outside formats are allowed by name.
- [ ] Performance gate: instruction-count ratios against the recorded baseline in the existing CI job, with targets set by the first step. Check: the numbers are in `src/osprey/packages/native/README.md`, and a regression fails CI.
- [ ] Every app on every host passes its existing journeys and goldens unchanged, with no test or golden weakened to get there.
