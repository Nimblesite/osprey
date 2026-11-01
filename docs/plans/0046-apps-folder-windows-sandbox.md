# Plan 0046 — Every app in one make folder, on Windows in a sandbox, tested by screenshot goldens

Rules: [AGENTS.md § Apps](../../AGENTS.md#apps). Hosts: WinUI ([spec 0048](../specs/platforms/0048-WinUiHost.md), [plan 0038](0038-winui-windows-hosts.md)), GPUI ([spec 0046](../specs/platforms/0046-GpuiDesktopHost.md)). Ports: [plan 0039](0039-examples-on-osprey-native.md). Graphics: [plan 0040](0040-winui-graphics-scenes.md). Status: **started 2026-11-01.**

## What the owner asked for (2026-11-01)

1. **Every app has a working Windows implementation.** Use WinUI on the latest Windows App SDK, not WPF, WinForms or raw Win32 windows. The GPUI examples may use whatever approach works best on Windows. Here that is the WinUI host, because it already draws them and can be sandboxed.
2. **Sandbox the apps and lock them down with permissions.** Prefer AppContainer over unsandboxed full-trust Win32. Each app declares only the capabilities it uses.
3. **Group the root make targets into folders, as `Book/` is grouped.** One folder holds all the apps: `apps/Makefile`. Running an app's target builds what it needs and opens the app, with no other steps.
4. **Automated app tests run headless or with a window.** Both modes take screenshots and compare them with committed goldens. These tests are part of the required `CI` check.
5. Commit and push regularly, even though others work on the same branch. Hand legwork to pi agents.

## Apps and their Windows state

| App | Sources | Windows before this plan | Windows target |
| --- | --- | --- | --- |
| Storage | `examples/storage` | WinUI host, unpackaged, full trust | WinUI host, MSIX AppContainer: picker-granted folders plus `broadFileSystemAccess` for a whole drive |
| Flock | `examples/gpui/flock` | WinUI host, unpackaged | MSIX AppContainer, no capabilities |
| Painting | `examples/gpui/painting` | WinUI host, unpackaged | MSIX AppContainer, no capabilities |
| Signal flow | `packages/signal_flow/app` | none (GPUI on macOS only) | WinUI host, MSIX AppContainer, no capabilities |
| Casting | `examples/casting` | none (GPUI, `dns-sd`, Homebrew OpenSSL) | WinUI host, AppContainer with `privateNetworkClientServer` and `internetClientServer`: Windows often classes a home Wi-Fi as Public, where only the second lets the app dial the TV and the TV fetch the file. Discovery uses `DnsServiceBrowse`, the file comes from the picker, and TLS uses the MinGW OpenSSL runtime |
| Graphics scenes | `examples/graphics` | Raw Win32 window with Direct3D 12 | `OspreyHost.exe --scene` draws into a `SwapChainPanel` (plan 0040 phase 2). The Win32 window code is deleted. AppContainer |
| Issue Inbox | `examples/mobile` | none (iOS, Android) | Osprey Native port (plan 0039 phase 4) on the WinUI host. AppContainer with `internetClient` |
| iPhone counter | `examples/ios` | none (iOS) | Osprey Native port on the WinUI host. AppContainer, no capabilities |
| Talon Bank | `examples/projects/modules` | Web server; the browser is the client | Web server in an AppContainer with `internetClientServer`, opened in the default browser |
| Proof Workbench | `apps/workbench` | none (Mac host, VS Code webview) | WinUI host, AppContainer |
| GPT-2 | `examples/gpt2` | console | Console in an AppContainer launched by the apps Makefile. Weights are read from the package |

## Decisions

1. **One package per app.** `hosts/winui/package.ps1 -App <name>` lays out `OspreyHost.exe`, the Windows App SDK runtime (self-contained), the app's DLL and an `AppxManifest.xml` generated from `apps/<name>.appx.json`. That file holds the app's name, display name, logo and capabilities. The manifest sets `uap10:TrustLevel="appContainer"` and `uap10:RuntimeBehavior="packagedClassicApp"`, and does not declare `runFullTrust`. The layout is registered with `Add-AppxPackage -Register`, which needs Developer Mode and no signing, and launched through `shell:AppsFolder`. CI signs a real `.msix` with a test certificate.
2. **The host finds its app beside itself.** With no DLL argument, a packaged host loads `app.dll` and reads `app.args` from its package folder. An AppContainer process can read its own package and nothing it was not granted.
3. **The store moves into the package.** When packaged, the store capabilities use `ApplicationData.GetDefault()` (the package's LocalState) in place of `%APPDATA%\Osprey\<app>`.
4. **Files go through the broker.** The folder walk opens directories with `CreateFile2FromApp`, which honours picker grants and `broadFileSystemAccess`. A folder the sandbox refuses counts as `unreadable`, as a folder its ACL denies does today.
5. **Elevation is refused in the sandbox.** Storage's "Restart as administrator" `HostCall` completes with an error while the host runs in an AppContainer, and Osprey does not offer it there.
6. **Screenshot goldens.** `--self-test` already captures the window after each `shot` step. Each app keeps goldens in `<app>/goldens/windows/<shot>.png`. `hosts/winui/self-test.ps1 -Goldens <folder>` compares every shot by per-channel tolerance and the fraction of differing pixels, writes a diff image beside each failure and fails the run. `-Update` rewrites the goldens. Headless runs render each region into a `RenderTargetBitmap` off screen in an offscreen window (`--headless`), so they need no visible desktop, and their goldens are the same files.
7. **Make folders.** Public targets move out of the root into folder Makefiles, as `Book/Makefile` does. Each folder target calls a private `_` target in the root that holds its recipe, so recipes are not copied. The root keeps `build`, `test`, `lint`, `fmt`, `ci`, `clean`, `install`, `audit`, `setup`, `run` and `help`, and `help` lists every folder's targets.

| Folder | Targets |
| --- | --- |
| `apps/` | One per app: `storage`, `flock`, `painting`, `signal-flow`, `casting`, `graphics`, `inbox`, `counter`, `bank`, `workbench`, `gpt2`. Plus `test` (every app's suite and screenshot goldens) and the per-platform build, smoke and bench targets of today's `gpui-*`, `ios*`, `android*`, `mobile-*`, `bank-*`, `macos*`, `workbench-*` and `graphics-*` |
| `benchmarks/` | `bench`, `bench-osprey` |
| `packages/` | `package-coverage`, `package-checks`, `package-coverage-record`, `registry-*` |
| `tests/hunt/` | `hunt`, `hunt-sample`, `hunt-crashes`, `hunt-corpus`, `hunt-demo` |
| `vscode-extension/` | `vsix-*` |
| `website/` | `wasm`, `wasm-site`, `wasm-serve`, `docs-html-test` |
| `examples/proof/` | `proof` |

## Phases

1. **Make folders.** Add the folder Makefiles and move the targets. Update every workflow, doc and script that names a moved target. `make help` lists them all.
2. **Sandboxed packages for the apps that already run on WinUI.** Storage, Flock and Painting: the package script, the packaged host (decisions 2 to 5) and `make -C apps <app>` on Windows. Add AppContainer checks to the host's tests: the process token is an AppContainer, a write outside the package fails, and the store writes to LocalState.
3. **Screenshot goldens in CI.** Add golden comparison and the headless mode. Commit goldens for Storage, Flock and Painting. `winui.yml` runs the packaged apps, compares their shots with the goldens and uploads the diffs. It is called from `ci.yml`, so the root `CI` job gates on it.
4. **Graphics in WinUI.** Plan 0040 phases 2 and 3.
5. **Signal flow and Casting on Windows.** Signal flow first; it needs only the WinUI host. Casting runs as its table row says. Built as of 2026-11-01:
   - The file server and TLS client compile under MinGW. Media render returns "" there, so a Windows sender casts plain audio.
   - The WinUI host answers `run` and `choose-file` (`Processes`, a `FileOpenPicker`).
   - `--browse` names `examples/casting/windows/cast-browse.c`, which prints the dns-sd lines `Discovery.browse` reads.
   - DLLs that use TLS link static OpenSSL.
   - `apps/windows.json` has the casting entry, and `app.ps1` builds its helper.

   Not yet run on Windows. The `make ui-test` step is in `ci-windows.yml`.
6. **Counter, Inbox and Workbench on Osprey Native.** Counter, then Inbox (plan 0039 phase 4), then Workbench on the WinUI host.
7. **Bank and GPT-2 sandboxed.** Both are launched in an AppContainer.

Each phase ends with its apps opened by `make -C apps <app>` on Windows and their goldens passing in CI.

## Risks

- **WinUI in an AppContainer.** Packaged WinUI 3 apps support `appContainer` trust, but some Windows App SDK components refuse it. One example is `DeploymentManager.Initialize`, which needs the `packageManagement` restricted capability. Self-contained deployment avoids it. Phase 2 proves this on Flock before the other apps move.
- **The whole drive in a sandbox.** `broadFileSystemAccess` is a restricted capability. The person grants it in Settings › Privacy › File system. Without it, Storage measures only picker-granted folders and says so.
- **Golden flakiness.** Animation and fonts vary by machine. Goldens are captured at fixed frames (`OSPREY_GFX_FRAMES`, the self-test's frame clock), and the comparison uses a tolerance, not exact bytes. Goldens are recorded on the CI runner image.
