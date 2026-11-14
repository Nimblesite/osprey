# WinUI host

`OspreyHost.exe <app.dll> [arguments]` opens one window and draws the [Osprey Native](../../../osprey/packages/native/README.md) application compiled into `app.dll` with WinUI, the native UI framework of the Windows App SDK. It uses Mica and Segoe Fluent Icons, with no XAML anywhere. The host is generic: every application decision, word and screen is Osprey. The contract is [spec 0048](../../../../docs/specs/platforms/0048-WinUiHost.md) [WINUI-HOST]; delivery is [plan 0037](../../../../docs/plans/0037-osprey-native.md).

There is one host path: the shared-memory bridge and its typed frame — each call's four sealed runs, committed, leased whole and read in place ([spec 0053](../../../../docs/specs/applications/0053-NativeSharedViews.md) explains the mechanism end to end). The application tells its component tree into that frame as typed rows [NATIVE-SHM-NODES], and the host draws it with main's WinUI renderer.

| Project | Holds |
| --- | --- |
| `OspreyHost.Core` | The DLL boundary and import slots, the shared-memory bridge and whole-frame leases, the one checked row reader, the scene row reader, the typed host events, the capability adapters, the Windows folder walk and the store; nothing here needs a window |
| `OspreyHost` | The window, the component renderer, the row view, the scene painter, the frame clock, the pickers and `--self-test`; Native AOT |
| `OspreyHost.Tests` | xUnit v3 on Microsoft.Testing.Platform |

## Run

From the repository root on Windows, with MSYS2 UCRT64's clang and llvm-ar as `OSPREY_CC` and `OSPREY_AR`, build an application into a DLL and pass it to the host:

```sh
./target/release/osprey build src/osprey/examples/storage --target=windows-x64 -o src/osprey/examples/storage/build/storage.dll
./target/release/osprey build src/osprey/examples/gpui/flock --target=windows-x64 -o src/osprey/examples/gpui/flock/build/flock.dll
./target/release/osprey build src/osprey/examples/gpui/painting --target=windows-x64 -o src/osprey/examples/gpui/painting/build/painting.dll
./target/release/osprey build src/osprey/examples/tally --target=windows-x64 -o src/osprey/examples/tally/build/tally.dll
dotnet run --project src/toolchain/hosts/winui/OspreyHost -c Release -- src/osprey/examples/gpui/flock/build/flock.dll
```

## The one host

Every application exports exactly three calls [NATIVE-SHM-EVENT]:

```c
int64 osprey_app_argument(const char *text);
int64 osprey_app_open(int64 now, int64 offset, const char *platform, const char *home, bool dark);
int64 osprey_app_event(int64 now, int64 kind, int64 ordinal, const char *words, const char *detail,
                       double x, double y, bool down, bool shift);
```

The host opens the `osprey_native_bridge` table the DLL exports before its entry runs. Every call writes one whole frame, and the host always commits it (`osp_native_frame_commit`), even when the application refuses the event and answers 1. The frame is one branch chunk with four sealed leaf runs, leased whole: scene rows (child 0), view rows (child 1), UTF-8 text (child 2) and ask rows (child 3). Nothing is copied per row and nothing is encoded.

[`FrameReader`](OspreyHost.Core/Rows.cs) is the one reader of view and ask rows, and [`SceneReader`](OspreyHost.Core/SceneRows.cs) reads the scene rows, with the same checks as the reference host's `rows.rs`: every kind, flag, code and role must be known; every parent must be an earlier row; every text range must lie inside the text run on UTF-8 boundaries; every float must be finite; every count must stay inside the rows that follow it. A frame that fails any check is refused whole and the last good view stays.

Asks answer through the existing adapters with a typed [`Ask`](OspreyHost.Core/Rows.cs): a field-0 row starts a request (codes 1 to 25, 100 for a slice write, 101 for a refused event's diagnostic), and the rows after it are its arguments by position. A chooser completes with the bare path, empty when cancelled; every other completion carries the same result document its adapter produced.

[`Told`](OspreyHost.Core/Nodes.cs) reads a checked frame's `field`, `record` and `list` rows into the component tree main's renderer drew: a record is a component with a `kind` and the members the application told, a list its children. [`RegionStore`](OspreyHost.Core/Regions.cs) keeps each region as its own tree and leaves a region whose tree did not change untouched, so only what moved is drawn again. [`Renderer`](OspreyHost/Renderer.cs), [`Containers`](OspreyHost/Containers.cs), [`Content`](OspreyHost/Content.cs), [`Controls`](OspreyHost/Controls.cs), [`Panels`](OspreyHost/Panels.cs) and [`Styles`](OspreyHost/Styles.cs) are main's renderer, drawing each component kind with WinUI's own controls and the system's theme resources; a kind this host does not draw is a visible diagnostic, never a guess. A scene component names the scene rows the frame gave its record, drawn and painted through [`Scenery.cs`](OspreyHost/Scenery.cs) and [`ScenePaint.cs`](OspreyHost/ScenePaint.cs).

[`SharedViewPage`](OspreyHost/SharedViewPage.cs) still draws a frame of plain view rows — column, line, label, press, entry, gap, scroll, layer, toggle, picker, option, rule, image, canvas, editor, menu and sheet, the last floating in the page's overlay — restyling them in place so an entry keeps its focus and caret. Every colour, size, weight and space comes from the rows; the host holds no style of its own. A press or label hint names its icon, a field's hint is its prompt and an option's hint its key.

Events are typed all the way down: control events, key, pointer, viewed, completed, phase `active`, appearance, and frames while an `animate` ask keeps the clock on. Under `--self-test` the clock is steady, so a script's `frames` step sends exactly that many 1/60 s frames and the window stands still between steps.

## Sandboxed apps

Each app ships as its own MSIX package that runs in an AppContainer ([WINUI-HOST-SANDBOX]): `uap10:TrustLevel="appContainer"`, no `runFullTrust`, and only the capabilities its entry in [`src/toolchain/targets/windows/apps.json`](../../../../src/toolchain/targets/windows/apps.json) lists. [`app.ps1`](app.ps1) builds the compiler, the MinGW runtime, the app's DLL and the host, lays them out from [`AppxManifest.xml`](AppxManifest.xml), registers the layout (Developer Mode, no signing) and opens it. The packaged host finds the app as `app\app.dll` beside itself, keeps the store in its LocalState and opens folders through the broker (`CreateFile2FromApp`), which honours a picked folder and `broadFileSystemAccess`. `make -C src/toolchain/targets/windows <app>` opens one; `make -C src/toolchain/targets/windows/tests <app>` self-tests one in a real window.

`src/osprey/examples/storage/run.sh --host=winui` does the unpackaged build and run for Storage. The window must open in a desktop session: a shell running as a Windows service, such as an SSH session in session 0, can run every test but not the window, and the host says so and exits with code 3.

## Test, publish and self-test

From PowerShell in `src/toolchain/hosts/winui`, after building the DLLs as above:

```powershell
dotnet test --project OspreyHost.Tests     # the row readers, scenes, asks, events, capabilities, walk, store, guards and boundary
dotnet publish OspreyHost -c Release       # Native AOT; trim and AOT warnings fail the build
./self-test.ps1 -Exe OspreyHost/bin/Release/net10.0-windows10.0.26100.0/win-x64/publish/OspreyHost.exe `
    -Dll ../../../osprey/examples/gpui/flock/build/flock.dll -Script ../../../osprey/examples/gpui/flock/self-test.json -Shots shots
```

The Core and test projects also build and run on macOS (`dotnet test OspreyHost.Tests`); only the WinUI app project needs Windows. Native AOT links with Visual Studio's C++ tools, so `vswhere.exe` must be on `PATH`: add `%ProgramFiles(x86)%\Microsoft Visual Studio\Installer`. Run the tests from PowerShell, not Git Bash. Git Bash's runtime enables the backup privilege, which reads folders their ACLs deny, so under Git Bash the denied-folder test is skipped and says why.

`--self-test` drives the real window from a script beside each application ([WINUI-HOST-SELF-TEST]) and keeps a picture of the window after each `shot` step. Its clock is steady: a `frames` step lets exactly that many 1/60 s frames run and the scene stands still otherwise, so the same script takes the same pictures on every run. `invoke` and `set` reach a scene's elements through the real controls UI Automation sees ([UI-MACHINE-A11Y]), `choose` picks a row or a picker item, and `drag` moves a movable element by a distance in scene units. `--headless` draws the window off screen without activating it.

`app.ps1 -App <app> -Test` runs the self-test inside the app's sandbox through its execution alias, then [`goldens.ps1`](goldens.ps1) compares each picture of the window's content with `<app>/goldens/windows/<shot>.png` and writes a diff picture for any that differs; `-Update` rewrites the goldens. A fixture the script opens, such as Storage's folder tree (`src/osprey/examples/storage/fixture.ps1`), is built in the package's LocalState. CI runs every app this way on the runner's desktop and keeps the pictures, then runs Storage unpackaged on the whole C drive with `src/osprey/examples/storage/self-test-drive.json` and `-Arguments 'C:\'`.

## Coverage

[`Coverage.cs`](OspreyHost.Core/Coverage.cs) is this host's column of the [coverage matrix](../../../../docs/specs/applications/0047-OspreyNativeHosts.md): every row kind view.ospml numbers and every told row told.ospml numbers is read here, every component kind is drawn or refused by name, and every capability kind is answered or refused by name. It answers `measure-folders`, `reveal`, `choose-file`, `choose-folder`, `save-file`, `run`, `cancel`, the four store kinds and `animate`; every other capability completes with an error naming it. The tests read the framework's sources, so a capability added to `@osprey/native` fails them until this host answers or refuses it.
