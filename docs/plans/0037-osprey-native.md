# Plan 0037 — Osprey Native hosts

Contract: [spec 0047](../specs/applications/0047-OspreyNativeHosts.md), [spec 0046](../specs/platforms/0046-GpuiDesktopHost.md), [spec 0048](../specs/platforms/0048-WinUiHost.md), [AGENTS.md § Apps](../../AGENTS.md#apps).

Depends on: [plan 0023](0023-gpu.md) (its Graphics plumbing section puts the scenes in the WinUI host).

## Prerequisites in place

- GPUI, WinUI and in-repo SwiftUI hosts, Storage/Flock/Painting, the UI-machine framework and the sandboxed-WinUI-with-goldens CI all run (spec 0046, spec 0048, UiMachines).
- The `Symbol` and `Platform` unions are typed and host-neutral ([UI-COMPONENTS-SYMBOLS], [UI-CONTEXT-PLATFORM]); `HostCall` exists and the native suite encodes one ([UI-EFFECTS]).

## 1. Finish framework neutrality [NATIVE-HOSTS-NEUTRAL]

- [ ] Remove `ReadPropertyLists`, `Fingerprint`, `RunningApplications`, `OpenSettings` from `Capability` and `Requests::members` in `packages/native/src/capability.ospml`, and remove `Run`'s `plist` field; product work travels as `HostCall`. Drop `read-plists`, `fingerprint`, `running-applications`, `open-settings` from `hosts/gpui/src/coverage.rs` (UNANSWERED) and `hosts/winui/OspreyHost.Core/Coverage.cs` (Unanswered). Check: `osprey packages/native/tests --run --memory=arc` and `--memory=gc` green; none of the four constructors remain in `capability.ospml`.
- [ ] Remove `Evidence` and fold it into `Pair` (`Ui::pair`, wire `pair`): delete `Evidence` from `Node` in `packages/native/src/node.ospml`, delete `Ui::evidence` and the `encode.ospml` arm, and drop `evidence` from GPUI `coverage.rs` REFUSED and WinUI `Coverage.cs` Refused. Check: `pair` is in both hosts' drawn lists (already is) and `grep evidence packages/native/src` is empty.
- [ ] Delete `Brand` from `Node`/`Ui`/`encode.ospml`; `Keyed::region`'s placeholder stays `Spacer` (`packages/native/src/reactive.ospml` already uses it); drop `brand` from GPUI `coverage.rs` REFUSED and WinUI `Coverage.cs` Refused. Check: `grep brand packages/native/src` is empty; `cargo test coverage` and WinUI `CoverageTests` green.

## 2. Conformance suite [NATIVE-HOSTS-CONFORMANCE]

- [ ] Add `packages/native/conformance/`: the probe application's start context, a recorded event sequence and the expected envelopes, produced once from `Runtime::start`/`dispatch` and checked in.
- [ ] Add an Osprey test that regenerates the envelopes from the fixtures and diffs them, so a framework change that alters the wire shape fails here first. Check: the test lives under `packages/native/tests` and fails on a wire change.
- [ ] Wire each host's smoke to run the set: SwiftUI in Vigil's smoke, webview in `npm test --prefix vscode-extension`, GPUI in `hosts/gpui/run.sh --smoke`, WinUI in its smoke. Check: each smoke passes the conformance set.

## 3. Coverage matrix [NATIVE-HOSTS-COVERAGE]

- [ ] Add a script that reads `packages/native/src/node.ospml` and `capability.ospml` for constructor names plus each host's mapping table (`hosts/gpui/src/coverage.rs`, `hosts/winui/OspreyHost.Core/Coverage.cs`, `vscode-extension/client/src/osprey-ui/render.ts` + `capabilities.ts`), and writes `packages/native/COVERAGE.md`.
- [ ] Make CI regenerate it and fail on a diff, so a new constructor cannot land without every host's cell.
- [ ] Each host README links its column.

## 4. Shared web renderer [NATIVE-HOSTS-FAMILY]

- [ ] Extract `osprey-ui` (`protocol.ts`, `render.ts`, `styles.ts`, `theme.ts`, `icons.ts`, `session.ts`, `wasi.ts`) from `vscode-extension/client/src/osprey-ui` into `packages/web` as an npm package; `capabilities.ts` and the webview plumbing stay in the extension. Check: `vscode-extension` still builds and its renderer suite is green.
- [ ] Add a browser host page that loads the WASI module in a worker and renders into a `div`, with file and store capabilities on the browser APIs that exist.
- [ ] Run the probe application in a browser tab and pass the conformance set.

## 5. Storage on GPUI, plus the two unrecorded measurements

- [ ] Add the GPUI `MeasureFolders` adapter `hosts/gpui/src/adapters/measure.rs` ([UI-EFFECTS-MEASURE]): Rust on `std::fs`, `st_blocks` and `(st_dev, st_ino)` on Unix, the Windows rules from spec 0048's folder walk. Tests on a temporary tree: hidden file, symbolic link, a hard link counted once, an unreadable folder, deadline, cancel. Move `measure-folders` from UNANSWERED to ANSWERED in `coverage.rs`. Check: `cargo test --manifest-path hosts/gpui/Cargo.toml measure`.
- [ ] Map `split` and `context-menu` in `hosts/gpui/src/render.rs` (+ `render/controls.rs`) so Storage's root split and row menus draw; move them from REFUSED to DRAWN in `coverage.rs`. Check: `cargo test coverage`; `make -C apps/macos storage` opens with no diagnostics.
- [ ] Storage acceptance: `./target/release/osprey test examples/storage/test` green on every OS; no host source names Storage ([NATIVE-HOSTS-NEUTRAL]).
- [ ] Record flat memory over ten minutes of Flock in the GPUI window; put the numbers in `hosts/gpui/README.md`.
- [ ] Record a focused-window fps reading at 5,000 birds with the screen unlocked (M-series Mac); put it in `examples/gpui/flock/README.md`.

## 6. Storage: Windows walk, shortcuts, elevation [UI-EFFECTS-MEASURE] [UI-COMPONENTS-SHORTCUT]

- [ ] GPUI Windows walk: capacity from `GetDiskFreeSpaceExW` (available after quotas); one `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)` batch per directory; hidden is `FILE_ATTRIBUTE_HIDDEN`; only `IO_REPARSE_TAG_SYMLINK` and `IO_REPARSE_TAG_MOUNT_POINT` are links; `WOF` files count allocated bytes and `APPEXECLINK` 0; cloud placeholders never opened (`CLOUD_*`, `RECALL_ON_DATA_ACCESS`, `RECALL_ON_OPEN`, `OFFLINE`); hard links once by volume serial + 128-bit file id; `ERROR_ACCESS_DENIED` adds one to `unreadable`. Run a shared fixture tree with expected JSON against both the Rust and C# walkers. Check: both adapters produce the same JSON.
- [ ] Add `Shortcut`/`Chord` to `packages/native/src/node.ospml` ([UI-COMPONENTS-SHORTCUT]): a `Button` may carry a typed `Chord`; `Primary` is Command on Apple systems, Control elsewhere; the host shows the chord in its menu or tooltip; Osprey picks chords per platform (Alt+Up, F5, Ctrl+O on Windows; Command+Up, Command+R, Command+O on the Mac).
- [ ] Elevation on Windows: `HostCall` in the `windows` namespace for "restart as administrator", answered by the WinUI host with `ShellExecuteExW`/`runas` and `--elevated`; Osprey offers it only on Windows when the walk hit unreadable folders and the process is not elevated. Check: a Storage self-test drives it.
- [ ] Measure a full `C:` drive (walk time, peak memory, gap vs used capacity) and record in `examples/storage/README.md`.

## 7. WinUI remaining: adapters, C fixture host, 60 Hz gate, packaging

- [ ] Answer the remaining WinUI capabilities ([WINUI-HOST-CAPABILITIES]): `list-directories`, `read-texts`, `save-file`, `http` (on `HttpClient`) and the `windows` `HostCall` for elevated restart; move them from `Unanswered` to `Answered` in `hosts/winui/OspreyHost.Core/Coverage.cs`. Check: `dotnet test --project OspreyHost.Tests` (from PowerShell).
- [ ] Add the MinGW C fixture host that loads the probe's DLL, binds both imports and calls both exports; run it in `ci-windows.yml` ([WINUI-HOST-DLL]).
- [ ] Measure whether WinUI shapes hold Flock's 5,000 birds at 60 Hz on a named Windows machine; record it ([WINUI-HOST-SCENE]).
- [ ] MSIX: sign a real `.msix` with a test certificate in CI (today `app.ps1` registers a loose layout via `Add-AppxPackage -Register`). Check: CI produces and installs the signed package.
- [ ] Add a `windows-arm64` slice (MSYS2 CLANGARM64) and a `win-arm64` publish. Check: `--target` accepts `windows-arm64`; the host publishes for arm64.

## 8. SwiftUI host, and retiring the old Storage app

- [ ] Extend `hosts/swift` (the in-repo SwiftUI host) to macOS and iOS, map every node and symbol (SF Symbols) and every capability including `measure-folders`, and run the conformance set. Check: Storage runs on `hosts/swift` with no diagnostics.
- [ ] Move Storage's Apple-only requests to `HostCall`s only this host answers: Photos summary `HostCall "photos" "summary"`, Full Disk Access `HostCall "macos" "privacy-settings"`.
- [ ] Retire `examples/macos` once Storage runs on GPUI, WinUI and SwiftUI; move every link to `examples/storage`; keep a website redirect.

## 9. Issue Inbox [UI-COMPONENTS-RICH]

- [ ] Every host completes `Http` with its platform HTTPS client within the framework's limits.
- [ ] Add `Rich` ([UI-COMPONENTS-RICH]): a paragraph of styled runs — strong, emphasis, code, link; markdown parsing stays in Osprey (`markdown.ospml`).
- [ ] Move Inbox's SQL tables to JSON documents in the app store via `StoreRead`/`StoreWrite`; each query becomes an Osprey list function.
- [ ] Add `hosts/compose` (Android) from today's `NativeRenderer.kt`; delete Inbox's own protocol, `NativeRenderer.swift` and `NativeRenderer.kt`; Inbox runs on GPUI, WinUI, SwiftUI and Compose.

## 10. Remaining apps [NATIVE-APPS-ALL]

- [ ] iPhone counter on `hosts/swift`.
- [ ] Talon Bank's Android and iOS apps on the Compose and SwiftUI hosts; Talon Bank's web client on the browser host.
- [ ] Graphics scenes in the WinUI host: `OspreyHost.exe --scene` draws into a `SwapChainPanel` and the Win32 window code (`examples/graphics/ospgfx_d3d12_window.c`) is deleted — [plan 0023](0023-gpu.md) Graphics plumbing. Check: `apps/windows.json` gains a `graphics` entry.
- [ ] Counter, Inbox and Workbench on Osprey Native in the WinUI host as AppContainer packages (after the Inbox port); add entries to `apps/windows.json` with the capabilities listed in AGENTS.md § Apps.
- [ ] Bank and GPT-2 sandboxed in AppContainers launched by the apps Makefile.
