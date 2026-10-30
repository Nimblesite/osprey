# WinUI Host [WINUI-HOST]

**Status:** `hosts/winui` draws the node kinds its renderer has WinUI controls for, scenes included, shows a visible diagnostic for the rest, answers the folder walk, the pickers, the store and the frame clock, and publishes with Native AOT. Its tests run with no window and drive [Osprey Storage](../../../examples/storage/README.md), [Flock](../../../examples/gpui/flock/README.md) and [Painting](../../../examples/gpui/painting/README.md) through their real DLLs; `--self-test` then drives each in a real window on the CI runner's desktop ([WINUI-HOST-SELF-TEST]), where all three passed on 2026-10-28. That historical checkpoint does not establish acceptance of the newly combined framework; fresh Windows execution is required. Delivery is tracked by [plan 0038](../../plans/0038-winui-windows-hosts.md).

The WinUI host renders an [Osprey Native](../../../packages/native/README.md) application with WinUI, the UI framework of the Windows App SDK. It is a host in the [Osprey Native host family](../applications/0047-OspreyNativeHosts.md) and the native Windows one: the controls are the ones Windows draws its own apps with, on Mica, with Segoe Fluent Icons.

## One thin host [WINUI-HOST-THIN]

The host is one generic executable, `OspreyHost.exe <app.dll> [arguments]`. It loads an Osprey application compiled to a DLL, draws its regions and answers its capabilities. Everything an application decides is Osprey: its store, its signals, its rules, its words and its screens. The host contains:

- `OspreyHost.Core`: the DLL boundary, envelope decoding, the region store, events, the capability adapters and the folder walk. None of it needs a window, so its tests run anywhere.
- `OspreyHost`: the window, the renderer and the pickers, built on WinUI.

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
| `osprey_app_start` | `const char *(const char *context)`: the first envelope. |
| `osprey_app_dispatch` | `const char *(const char *saved, const char *event)`: the next envelope. |
| `osprey_import_<name>` | `void (fn)`: binds one import slot ([WINUI-HOST-IMPORTS]). |

The header written beside the DLL declares each binder with the import's C signature. The CLI's tests check the slots, the C `bool` convention, the entry's check of every slot, that only the boundary is exported, and that a binder name the program already defines is refused.

## Imports [WINUI-HOST-IMPORTS]

An extern the application declares, such as `native_slice`, becomes an internal slot and a function that calls through it, because a DLL cannot leave a symbol for its loader to supply. The host loads the DLL with `NativeLibrary` and binds each import it answers through its `osprey_import_<name>` export before calling `osprey_main`. It answers `native_slice`, for host-held slices, and `native_shape`, for streamed scene primitives ([UI-SCENE-STREAM](../../../packages/native/README.md)). An application that imports anything else fails to start with a diagnostic naming what this host answers.

## Calls and memory [WINUI-HOST-THREAD]

Every call into the application is made on the UI thread, one at a time. An event that arrives while a call runs waits for it. Strings the host passes are NUL-terminated UTF-8 copies, freed when the call returns, and the returned envelope is copied before the next call. Host-held slices are copied once into unmanaged memory and never freed, because the runtime may keep the pointer it was given; a replaced slice's buffer is retired, not released. Text holding a NUL character is refused, because no C string can carry it.

## Rendering [WINUI-HOST-RENDER]

Each region is drawn into a holder that its `slot` placed, so a region Osprey did not resend keeps its controls, and the opaque state's revisions name which controls an event came from. Each node kind maps to WinUI's own control:

| Kind | WinUI | Kind | WinUI |
| --- | --- | --- | --- |
| `screen` | title, subtitle and a `CommandBar` of the toolbar | `rows` | `ListView` with group headers |
| `split` | a 280-pixel sidebar beside layered panes | `tabs` | `SelectorBar` |
| `section` | a card | `aside` | two panes and a draggable splitter |
| `stack`, `layer` | `Grid` | `context-menu` | `MenuFlyout` |
| `text`, `label`, `pair`, `badge` | `TextBlock` on the type ramp | `button` | `Button` or `AppBarButton` |
| `icon` | `FontIcon` | `field` | `TextBox` or `PasswordBox` |
| `meter`, `progress` | `ProgressBar` | `toggle` | `ToggleSwitch` or `AppBarToggleButton` |
| `empty` | an icon, a title and a line | `picker` | `ComboBox` |
| `link` | `HyperlinkButton` for `https` addresses only | `disclosure` | `Expander` |
| `segments`, `swatch`, `divider`, `spacer`, `slot` | shapes and grids | `alert` | `ContentDialog` |
| `scene` | a canvas of WinUI shapes ([WINUI-HOST-SCENE]) | | |

`rich-text`, `framed`, `themed`, `dialog`, `evidence`, `brand`, `math`, `image`, `video`, `editor` and `board` have no WinUI control in this host yet. Each is replaced by a visible diagnostic naming the kind and reported to the info bar, so an application loses no content silently, and `Coverage.cs` declares it refused. The renderer refuses to start unless it draws exactly the kinds declared drawn.

A child fills when it is rows, an empty state, an aside, a scene, a page or a split, a column holding one of those, or a slot whose region fills. In a column, a child that fills takes the room left over and scrolls or scales on its own, and a page fits the window when a child fills or is a slot; otherwise the page scrolls. A region can change what it shows without its column being redrawn, so a slot's row is settled again each time its region is drawn. A split's panes take the window's full height. Text roles map to the WinUI type ramp, tones to the theme's brushes and symbols to Segoe Fluent Icons. A kind the renderer does not draw is shown as a visible diagnostic. Tests fail when a framework symbol has no glyph, or when the renderer reads a node field the framework's encoder never writes.

## Capabilities [WINUI-HOST-CAPABILITIES]

Adapters run off the UI thread and complete on it. They do what Osprey asked within the bounds Osprey chose, and sort, hide, format and decide nothing.

| Kind | Adapter |
| --- | --- |
| `measure-folders` | The Windows folder walk below; at most 100,000 entries per folder and ten minutes per walk |
| `reveal` | `SHOpenFolderAndSelectItems`; the result is `{}` |
| `choose-folder` | `Microsoft.Windows.Storage.Pickers.FolderPicker`; the result is `{"path","cancelled"}` |
| `cancel` | Cancels the walk running under the target ticket; that walk completes with the error `cancelled` |
| `store-read`, `store-write`, `store-list`, `store-delete` | Files under `%APPDATA%\Osprey\<app>`, as the GPUI host keeps them: names relative to that folder, reads bounded and UTF-8, writes atomic |
| `animate` | The frame clock ([WINUI-HOST-FRAME]); completes at once |

Every other kind completes with the error "The Windows host does not answer *kind* requests". `Coverage.cs` declares every node kind as drawn or refused and every capability as answered or refused, and tests derive the framework's kinds from its sources, so a kind added to `@osprey/native` fails them until this host declares it ([NATIVE-HOSTS-COVERAGE](../applications/0047-OspreyNativeHosts.md)).

**The Windows folder walk** ([UI-EFFECTS-MEASURE](../../../packages/native/README.md)) opens each folder once with `FILE_FLAG_BACKUP_SEMANTICS` and reads its children in 64 KiB batches of `FileIdExtdDirectoryInfo`, or `FileIdBothDirectoryInfo` on file systems without 128-bit ids, such as FAT. No child is opened to learn its size, attributes, reparse tag or id. It keeps an explicit stack, so no tree is deep enough to exhaust the thread's stack, and opens paths in their `\\?\` form, so a path past 260 characters is still read. The rules:

- `bytes` are allocated bytes. A file with several hard links counts once per walk, keyed by its volume's serial number and its 128-bit file id.
- Hidden means `FILE_ATTRIBUTE_HIDDEN`, as File Explorer hides; a leading dot is not hidden. `visible` leaves out hidden items and everything below a hidden folder.
- Only symbolic links and mount points, which junctions use, are links. A link is listed with no bytes and never followed, so the walk stays on its volume.
- Cloud placeholders and offline files are never opened or entered, so nothing downloads. App execution aliases count as files of no bytes.
- A folder that cannot be opened adds one to `unreadable` at any depth; a direct child that cannot be opened is listed `guarded`.
- The deadline runs from the request across all its folders. Past it, the walk enters nothing more and marks what it measured `truncated`.
- Names that are not valid UTF-16, which NTFS allows, are written with U+FFFD in place of each unpaired surrogate rather than failing the walk.

## Deployment [WINUI-HOST-DEPLOY]

The host is unpackaged and self-contained: the Windows App SDK runtime ships beside the executable, so nothing is installed first. `EnableMsixTooling` publishes the resource index, `OspreyHost.pri`, that holds WinUI's theme dictionaries; without it the published host cannot load its controls' styles. Before WinUI starts, the host checks that its window station is visible and, in a Windows service's session, says it needs a desktop instead of ending with WinUI's `E_ACCESSDENIED`. `dotnet publish OspreyHost -c Release` compiles it with Native AOT, with trim and AOT warnings as errors. On 2026-10-28 the published folder held a 6.1 MB `OspreyHost.exe` and 237 files totalling 90 MB, most of them the WinUI runtime. MSIX packaging and ARM64 are plan 0038 phase 5.

## Scenes [WINUI-HOST-SCENE]

`OspreyHost.Core` decodes a scene exactly as the encoder wrote it, an array per primitive with its type code first, and refuses anything malformed. The scene's own shapes come first, then the shapes the application streamed for it through `native_shape` ([UI-SCENE-STREAM]), and the whole list is ordered back to front, stably by depth. Each scene in an envelope takes the next shapes of the stream in the order the framework wrote the scenes, and a count that does not match is reported.

The window draws a scene with WinUI's own shapes on a canvas: rects and sprites centred on their point and turned by their angle, discs, strips, glyph runs from their top-left corner, and paths. A path's steps become a `PathGeometry` (lines, quadratic and cubic curves, SVG arcs whose rotation is in degrees, polygons); its own move and scale apply to the geometry and not the stroke; and it is filled even-odd or stroked with its width, bevel or miter joins and dashes. Paints are sRGB or HSL tints, the hue in turns, or a two-stop linear gradient whose angle follows CSS, blended in Oklab when asked. Sprites draw as tinted quads, as the GPUI host draws them, until atlases load. A scene with a size is scaled whole into the room it has and letterboxed; one without is in window pixels.

Each scene keeps its elements between frames and reuses them, so a frame that moves a thousand discs only moves them. Pressing a scene focuses it; its keys (the arrows, space, enter, escape, tab, backspace, single letters and digits) and the pointer in scene units reach its binding. Every drag movement is sent, and a hover movement at most once a frame.

## Frames [WINUI-HOST-FRAME]

`animate` with `on` starts the frame clock and `off` stops it; the request completes at once. While it runs, every `CompositionTarget.Rendering` sends a `frame` event whose `elapsed` is 1/60 s for the first frame and the seconds since the previous one after that, at most 0.1 s. A refresh that arrives while an envelope is still being applied is skipped, so frames never queue behind a slow update.

## Self-test [WINUI-HOST-SELF-TEST]

`OspreyHost.exe --self-test <script.json> <shots> <app.dll> [arguments]` drives the real window from a script that lives beside the application and exits with the verdict. The script names only what a person sees, so the host learns nothing about the application from it. Its steps are `wait` and `gone` for words on screen, `invoke` a button, `toggle` a switch, `choose` a row, `type` into a field, `key` in a scene, `sleep` while a scene animates, and `shot`, which saves the window as Windows composes it and its content as WinUI renders it. Buttons and switches act through their own automation peers, and a row through the handler its clicks run, which the list names by a key it carries itself: a control's managed wrapper can be collected and remade while the control is on screen, so it is never the key. `choose` collects garbage first to keep that so, and scrolls a row its list has not made yet into view, as a person would scroll to it. Any diagnostic the window shows fails the test. The host appends each phase of start-up and each step to `progress.txt` as it reaches it, so a process that dies leaves the last thing it reached behind, and `hosts/winui/self-test.ps1` prints that trail, a picture of the desktop and the debugger's analysis of a crash dump when a run fails.

## Verification [WINUI-HOST-VERIFICATION]

```sh
osprey build examples/storage --target=windows-x64 -o examples/storage/build/storage.dll
cd hosts/winui && dotnet test --project OspreyHost.Tests     # from PowerShell
```

The tests cover:

- the envelope's members, bounds and versions, the events and the region store;
- the capability adapters and the folder walk on temporary trees: allocated bytes, hidden items, a junction, a hard link, a folder its ACL denies, a missing folder, the entry limit, the deadline, cancellation, a tree past 260 characters, non-ASCII names and the slice's key order;
- coverage, glyphs, field names, no XAML, the package list and neutrality;
- Osprey Storage started, walked and dispatched through its real DLL with no window, drawing only kinds this host draws and returning no diagnostic; Flock's 1,000 birds streamed into its scene frame after frame; and a pointer stroke on Painting's canvas coming back as a path;
- the scene decoder, the store, and the protocol's scene input and frame events.

Run them from PowerShell. Git Bash's runtime enables the backup privilege, which reads folders their ACLs deny, so under Git Bash the denied-folder case is skipped and says why. [`winui.yml`](../../../.github/workflows/winui.yml) runs the suite, builds the examples' DLLs, publishes the host and runs each example's self-test on the runner's desktop, keeping the pictures as an artifact. Storage runs twice: on a fixture tree, and on the whole C drive, opening Users from its row. A window cannot open in session 0, where Windows services run, so a shell that is a service, such as an SSH session, can run every test but not the window: run `OspreyHost.exe` from the desktop session.
