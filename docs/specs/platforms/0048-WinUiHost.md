# WinUI Host [WINUI-HOST]

**Status:** `src/toolchain/hosts/winui` draws the view row kinds its renderer has WinUI controls for, the scene rows its painter draws, shows a visible diagnostic for anything refused, answers the folder walk, the pickers, `run`, the store and the frame clock, and publishes with Native AOT. Its tests run with no window and cover the row readers, scenes, asks, events, capabilities, the walk, the store, the guards and the boundary; `--self-test` then drives each app in a real window on a desktop session. The Core and test projects also build and run on macOS; only the WinUI app project needs Windows. Delivery is tracked by [plan 0037](../../plans/0037-osprey-native.md).

The WinUI host renders an [Osprey Native](../../../src/osprey/packages/native/README.md) application with WinUI, the UI framework of the Windows App SDK. It is a host in the [Osprey Native host family](../applications/0047-OspreyNativeHosts.md) and the native Windows one: the controls are the ones Windows draws its own apps with, on Mica, with Segoe Fluent Icons.

## One thin host [WINUI-HOST-THIN]

The host is one generic executable, `OspreyHost.exe <app.dll> [arguments]`. It loads an Osprey application compiled to a DLL, leases each call's frame, checks its rows and draws them, and answers its capabilities. Everything an application decides is Osprey: its store, its signals, its rules, its words and its screens. The host contains:

- `OspreyHost.Core`: the DLL boundary and import slots, the shared-memory bridge and whole-frame leases, the one checked row reader, the scene row reader, the typed host events, the capability adapters, the Windows folder walk and the store. None of it needs a window, so its tests run anywhere.
- `OspreyHost`: the window, the shared view page, the scene painter, the frame clock, the pickers and `--self-test`, built on WinUI.

The host names no application, holds no application's words or slices, and contains no C# UI framework. A test fails when a host source names an example or one of an example's slices ([NATIVE-HOSTS-NEUTRAL](../applications/0047-OspreyNativeHosts.md)).

## No XAML [WINUI-HOST-NO-XAML]

Every element is built in C#. The application class implements `IXamlMetadataProvider` through `XamlControlsXamlMetaDataProvider`, and merges `XamlControlsResources` in code. `DISABLE_XAML_GENERATED_MAIN` hands `Main` to the host, which calls `Application.Start`. Lists hold elements, never a `DataTemplate`, and theme resources are looked up in code. A test fails if any `.xaml` file appears in the host.

## Packages [WINUI-HOST-PACKAGES]

The host references `Microsoft.WindowsAppSDK.WinUI`, which brings the Foundation component and its folder picker, and `Microsoft.Windows.SDK.BuildTools`, and nothing else. The Windows App SDK metapackage is not used: it would ship the AI, machine-learning, search and widget runtimes, 61 MB the host never loads. A test fails on any other package reference.

## The application DLL [WINUI-HOST-DLL]

`osprey build <project> --target=windows-x64 -o app.dll` links the application, `@osprey/native` and the ARC runtime into one DLL with MinGW clang, with winpthreads linked statically. Only the boundary is exported:

| Export | Meaning |
| --- | --- |
| `osprey_main` | Checks every import slot, then runs the program's entry once. It returns -2 while any slot is empty. |
| `osprey_native_bridge` | The shared-memory bridge table, which the host opens before the entry runs ([NATIVE-SHM-OWNERSHIP](../applications/0047-OspreyNativeHosts.md)). |
| `osprey_app_argument` | `int64 (const char *text)`: one launch argument, in order, before `open`. |
| `osprey_app_open` | `int64 (int64 now, int64 offset, const char *platform, const char *home, bool dark)`: draws the first frame. |
| `osprey_app_event` | `int64 (int64 now, int64 kind, int64 ordinal, const char *words, const char *detail, double x, double y, bool down, bool shift)`: delivers one typed host event and draws the next frame ([NATIVE-SHM-EVENT](../applications/0047-OspreyNativeHosts.md)). |
| `osprey_import_<name>` | `void (fn)`: binds one import slot ([WINUI-HOST-IMPORTS]). |

The header written beside the DLL declares each binder with the import's C signature. The CLI's tests check the slots, the C `bool` convention, the entry's check of every slot, that only the boundary is exported, and that a binder name the program already defines is refused.

## Imports [WINUI-HOST-IMPORTS]

An extern the application declares, such as `native_slice`, becomes an internal slot and a function that calls through it, because a DLL cannot leave a symbol for its loader to supply. The host loads the DLL with `NativeLibrary`, opens its bridge, binds each import it answers through its `osprey_import_<name>` export, and only then runs `osprey_main`. It answers `native_slice`, for host-held slices. An application that imports anything else fails to start with a diagnostic naming what this host answers.

## Calls and memory [WINUI-HOST-THREAD]

Every call into the application is made on the UI thread, one at a time. An event that arrives while a call runs waits for it. Strings the host passes are NUL-terminated UTF-8 copies, freed when the call returns, and text holding a NUL character is refused, because no C string can carry it. Every call writes one whole frame, which the host commits (`osp_native_frame_commit`) whether the call answered or refused, then leases whole and checks: nothing is copied per row and nothing is encoded. Host-held slices are copied once into unmanaged memory and never freed, because the runtime may keep the pointer it was given; a replaced slice's buffer is retired, not released.

## Rendering [WINUI-HOST-RENDER]

`FrameReader` ([`Rows.cs`](../../../src/toolchain/hosts/winui/OspreyHost.Core/Rows.cs)) is the one reader of view and ask rows, and `SceneReader` ([`SceneRows.cs`](../../../src/toolchain/hosts/winui/OspreyHost.Core/SceneRows.cs)) reads the scene rows, with the same checks as the reference host's `rows.rs`: every kind, flag, code and role must be known; every parent must be an earlier row; every text range must lie inside the text run on UTF-8 boundaries; every float must be finite; every count must stay inside the rows that follow it. A frame that fails any check is refused whole and the last good view stays.

[`SharedViewPage`](../../../src/toolchain/hosts/winui/OspreyHost/SharedViewPage.cs) draws every row kind with a real WinUI control and restyles it in place on later frames, so an entry keeps its focus and caret:

| Kind | WinUI |
| --- | --- |
| `column`, `line` | a `Grid` stacking its children vertically or side by side, sharing free space by their `grow` |
| `scroll` | `ScrollViewer` |
| `layer` | a `Grid` with its children on top of one another, the last in front |
| `label`, `option` | a `TextBlock` in a card; a label is led by the icon its hint names, an option's hint is its key |
| `press` | `Button` |
| `entry` | `TextBox`, or `PasswordBox` when its secret flag is set |
| `editor` | a multiline `TextBox` |
| `gap` | a `Border` that takes the leftover space |
| `toggle` | `ToggleSwitch` |
| `picker` | `ComboBox`; its `option` children become its items |
| `rule` | a `Border` hairline in the row's ink |
| `image` | `Image` in a card |
| `canvas` | a scene view ([WINUI-HOST-SCENE]) |
| `menu` | its first child in a card, the rest its `MenuFlyout` |
| `sheet` | its children floating in the page's overlay; dismissing it delivers its action |

Every colour, size, weight and space comes from the rows; the host holds no style of its own. `Coverage.cs` declares every row kind drawn or refused and every capability answered or refused, and the tests read the framework's sources, so a kind added to `@osprey/native` fails them until this host declares it ([NATIVE-HOSTS-COVERAGE](../applications/0047-OspreyNativeHosts.md)). Tests also fail when a framework symbol has no glyph, or when the renderer reads a field the framework's rows never carry.

## Capabilities [WINUI-HOST-CAPABILITIES]

Adapters run off the UI thread and complete on it. They do what Osprey asked within the bounds Osprey chose, and sort, hide, format and decide nothing. Each request arrives as an ask row and each completion is one `completed` event carrying the ticket and its result text; a chooser completes with the bare path, empty when cancelled.

| Kind | Adapter |
| --- | --- |
| `measure-folders` | The Windows folder walk below; at most 100,000 entries per folder and ten minutes per walk |
| `reveal` | `SHOpenFolderAndSelectItems`; the result is `{}` |
| `choose-file`, `choose-folder` | The Windows App SDK pickers |
| `save-file` | The save picker with the suggested name |
| `run` | One bounded, cancellable executable run ([`Processes.cs`](../../../src/toolchain/hosts/winui/OspreyHost.Core/Processes.cs)) |
| `cancel` | Cancels the walk running under the target ticket; that walk completes with the error `cancelled` |
| `store-read`, `store-write`, `store-list`, `store-delete` | Files under `%APPDATA%\Osprey\<app>`, as the GPUI host keeps them: names relative to that folder, reads bounded and UTF-8, writes atomic |
| `animate` | The frame clock ([WINUI-HOST-FRAME]); completes at once |

Every other kind completes with the error "The Windows host does not answer *kind* requests".

**The Windows folder walk** ([UI-EFFECTS-MEASURE](../../../src/osprey/packages/native/README.md)) opens each folder once with `FILE_FLAG_BACKUP_SEMANTICS` and reads its children in 64 KiB batches of `FileIdExtdDirectoryInfo`, or `FileIdBothDirectoryInfo` on file systems without 128-bit ids, such as FAT. No child is opened to learn its size, attributes, reparse tag or id. It keeps an explicit stack, so no tree is deep enough to exhaust the thread's stack, and opens paths in their `\\?\` form, so a path past 260 characters is still read. The rules:

- `bytes` are allocated bytes. A file with several hard links counts once per walk, keyed by its volume's serial number and its 128-bit file id.
- Hidden means `FILE_ATTRIBUTE_HIDDEN`, as File Explorer hides; a leading dot is not hidden. `visible` leaves out hidden items and everything below a hidden folder.
- Only symbolic links and mount points, which junctions use, are links. A link is listed with no bytes and never followed, so the walk stays on its volume.
- Cloud placeholders and offline files are never opened or entered, so nothing downloads. App execution aliases count as files of no bytes.
- A folder that cannot be opened adds one to `unreadable` at any depth; a direct child that cannot be opened is listed `guarded`.
- The deadline runs from the request across all its folders. Past it, the walk enters nothing more and marks what it measured `truncated`.
- Names that are not valid UTF-16, which NTFS allows, are written with U+FFFD in place of each unpaired surrogate rather than failing the walk.

A `HostCall` in the `windows` namespace, "restart as administrator", is answered with `ShellExecuteExW` and the `runas` verb, and passes `--elevated` back in the launch arguments. Osprey decides when to offer it: on Windows, when the walk could not read some folders and the process is not elevated. Not implemented yet: `host-call` is unanswered in this host, so a `windows` request completes with an error.

## Deployment [WINUI-HOST-DEPLOY]

The host is unpackaged and self-contained: the Windows App SDK runtime ships beside the executable, so nothing is installed first. `EnableMsixTooling` publishes the resource index, `OspreyHost.pri`, that holds WinUI's theme dictionaries; without it the published host cannot load its controls' styles. Before WinUI starts, the host checks that its window station is visible and, in a Windows service's session, says it needs a desktop instead of ending with WinUI's `E_ACCESSDENIED`. `dotnet publish OspreyHost -c Release` compiles it with Native AOT, with trim and AOT warnings as errors. MSIX packaging and its AppContainer sandbox follow below ([WINUI-HOST-SANDBOX]); ARM64 is plan 0037.

## The AppContainer sandbox [WINUI-HOST-SANDBOX]

One package per app. [`app.ps1 -App <name>`](../../../src/toolchain/hosts/winui/app.ps1) lays out `OspreyHost.exe`, the Windows App SDK runtime (self-contained), the app's DLL, a logo drawn from the app's name, and an `AppxManifest.xml` generated from the app's entry in [`src/toolchain/targets/windows/apps.json`](../../../src/toolchain/targets/windows/apps.json), which holds the app's name, description and capabilities. The manifest sets `uap10:TrustLevel="appContainer"` and `uap10:RuntimeBehavior="packagedClassicApp"`, and does not declare `runFullTrust`. The layout is registered with `Add-AppxPackage -Register`, which needs Developer Mode and no signing, and launched through `shell:AppsFolder`. CI signs a real `.msix` with a test certificate; that is not implemented yet, and CI registers the loose layout.

The host finds its app beside itself. With no DLL argument, a packaged host loads `app\app.dll` and reads its default arguments from `app\arguments.txt` in its package folder. An AppContainer process can read its own package and nothing it was not granted.

The store moves into the package. When packaged, the store capabilities use `ApplicationData.GetDefault()` (the package's LocalState) in place of `%APPDATA%\Osprey\<app>`.

Files go through the broker. The folder walk opens directories with `CreateFile2FromApp`, which honours picker grants and `broadFileSystemAccess`. A folder the sandbox refuses counts as `unreadable`, as a folder its ACL denies does today.

Elevation is refused in the sandbox. Storage's "restart as administrator" `HostCall` completes with an error while the host runs in an AppContainer, and Osprey does not offer it there. Not implemented yet: the `windows` namespace it travels in is unanswered ([WINUI-HOST-CAPABILITIES]).

## Scenes [WINUI-HOST-SCENE]

`SceneReader` decodes the scene rows exactly as the application wrote them and refuses anything malformed: a marker (code 20) names the canvas's view row, and the rows after it — atlas entries, shapes, accessible elements — belong to that canvas; a count that runs past the rows is reported. The whole list is ordered back to front, stably by depth.

The window draws a scene with WinUI's own shapes on a canvas: rects and sprites centred on their point and turned by their angle, discs, strips, glyph runs from their top-left corner, and paths. A path's steps become a `PathGeometry` (lines, quadratic and cubic curves, SVG arcs whose rotation is in degrees, polygons); its own move and scale apply to the geometry and not the stroke; and it is filled even-odd or stroked with its width, bevel or miter joins and dashes. Paints are sRGB or HSL tints, the hue in turns, or a two-stop linear gradient whose angle follows CSS, blended in Oklab when asked. Sprites draw as tinted quads, as the GPUI host draws them, until atlases load. A scene with a size is drawn at its fixed `zoom` from its region's top left and clipped to itself, so resizing the window shows more or less canvas and never resizes what is on it, or with a `zoom` of 0 as large as fits the region whole; one without is in window pixels.

Each scene keeps its elements between frames and reuses them, so a frame that moves a thousand discs only moves them. Pressing a scene focuses it; its keys (the arrows, space, enter, escape, tab, backspace, single letters and digits) the pointer, and the region's size when a sized scene first shows and whenever it changes, all in scene units, reach its binding as typed events. Every drag movement is sent, and a hover movement at most once a frame.

## Frames [WINUI-HOST-FRAME]

`animate` with `on` starts the frame clock and `off` stops it; the ask completes at once. While it runs, every `CompositionTarget.Rendering` sends a `frame` event whose elapsed time is 1/60 s for the first frame and the seconds since the previous one after that, at most 0.1 s. A refresh that arrives while a frame is still being applied is skipped, so frames never queue behind a slow update. Under `--self-test` the clock is steady, so a script's `frames` step sends exactly that many 1/60 s frames and the window stands still between steps.

## Self-test [WINUI-HOST-SELF-TEST]

`OspreyHost.exe --self-test <script.json> <shots> <app.dll> [arguments]` drives the real window from a script that lives beside the application and exits with the verdict. The script names only what a person sees, so the host learns nothing about the application from it. Its steps are `wait` and `gone` for words on screen, `invoke` a button or anything a scene announces, `set` a slider, `drag` a scene's movable element, `toggle` a switch, `choose` a row, `type` into a field, `key` in a scene, `sleep` while a scene animates, and `shot`, which saves the window as Windows composes it, its content as WinUI renders it and, when the window is on screen, its content's area as the screen shows it. Buttons and switches act through their own automation peers, and a row through the handler its clicks run, which the list names by a key it carries itself: a control's managed wrapper can be collected and remade while the control is on screen, so it is never the key. `choose` collects garbage first to keep that so, and scrolls a row its list has not made yet into view, as a person would scroll to it. Any diagnostic the window shows fails the test. The host appends each phase of start-up and each step to `progress.txt` as it reaches it, so a process that dies leaves the last thing it reached behind, and `src/toolchain/hosts/winui/self-test.ps1` prints that trail, a picture of the desktop and the debugger's analysis of a crash dump when a run fails.

### Screenshot goldens [WINUI-HOST-GOLDENS]

`src/toolchain/hosts/winui/goldens.ps1` compares every content picture with the golden of the same name in `<app>/goldens/windows/<percent>`, where the percent is the display scale the self-test wrote to `scale.txt`: WinUI draws at the display's scale, so a picture taken at 150% is held only to goldens taken at 150%. A pixel differs when a channel is off by more than 24, and a picture fails when more than 0.2% of its pixels differ, its size differs or it has no golden. A self-test on screen is held to the same goldens a second time, through the pictures taken from the screen. Every pixel is compared with the golden drawn over the backdrop, which is the screen's mean colour where the golden is clear, since the Mica backdrop follows the wallpaper. The two are lined up within 8 pixels, since WinUI's content picture can reach a few pixels past the area the content is laid out in. So a run on screen checks what a person sees, not only what WinUI renders. Under `-Headless` the window is off every screen and only the content is compared. `-Update` rewrites the goldens for the display's scale from the content pictures.

A `live.json` beside a scale's goldens lists `[x, y, width, height]` rectangles of the golden that show the machine rather than the app, and those pixels are not compared. Storage's holds the disk bar and the used and available figures of the C drive, since they change as the disk fills; the rest of that card is still compared.

## Verification [WINUI-HOST-VERIFICATION]

```sh
osprey build src/osprey/examples/storage --target=windows-x64 -o src/osprey/examples/storage/build/storage.dll
cd src/toolchain/hosts/winui && dotnet test --project OspreyHost.Tests     # from PowerShell
```

The tests cover the row readers' checks, the scene decoder, the ask fields and typed events, the capability adapters and the folder walk on temporary trees (allocated bytes, hidden items, a junction, a hard link, a folder its ACL denies, a missing folder, the entry limit, the deadline, cancellation, a tree past 260 characters, non-ASCII names and the slice's key order), coverage, glyphs, field names, no XAML, the package list, neutrality, and the DLL boundary's exports, slots and bridge.

Run them from PowerShell. Git Bash's runtime enables the backup privilege, which reads folders their ACLs deny, so under Git Bash the denied-folder case is skipped and says why. [`winui.yml`](../../../.github/workflows/winui.yml) runs the suite, builds the examples' DLLs, publishes the host and runs each example's self-test on the runner's desktop, keeping the pictures as an artifact. Storage runs twice: on a fixture tree, and on the whole C drive, opening Users from its row. A window cannot open in session 0, where Windows services run, so a shell that is a service, such as an SSH session, can run every test but not the window: run `OspreyHost.exe` from the desktop session.
