# WinUI host

`OspreyHost.exe <app.dll> [arguments]` opens one window and draws the [Osprey Native](../../packages/native/README.md) application compiled into `app.dll` with WinUI, the native UI framework of the Windows App SDK. It uses Mica and Segoe Fluent Icons, with no XAML anywhere. The host is generic: every application decision, word and screen is Osprey, and the host only draws regions, returns events by the ids the framework issued and answers capabilities. The contract is [spec 0048](../../docs/specs/platforms/0048-WinUiHost.md) [WINUI-HOST]; delivery is [plan 0038](../../docs/plans/0038-winui-windows-hosts.md).

| Project | Holds |
| --- | --- |
| `OspreyHost.Core` | The DLL boundary and import slots, envelopes, the region store, events, the capability adapters and the Windows folder walk; nothing here needs a window |
| `OspreyHost` | The window, the code-only renderer and the folder picker; Native AOT |
| `OspreyHost.Tests` | xUnit v3 on Microsoft.Testing.Platform |

## Run

From the repository root on Windows, with MSYS2 UCRT64's clang and llvm-ar as `OSPREY_CC` and `OSPREY_AR`:

```sh
examples/storage/run.sh --host=winui      # builds storage.dll and opens Osprey Storage
```

Or build any application yourself and pass its DLL:

```sh
./target/release/osprey build examples/storage --target=windows-x64 -o examples/storage/build/storage.dll
dotnet run --project hosts/winui/OspreyHost -c Release -- examples/storage/build/storage.dll
```

The window must open in a desktop session. A shell running as a service, such as an SSH session in session 0, can run every test but cannot create the window: WinUI fails with `E_ACCESSDENIED` there.

## Test and publish

From PowerShell in `hosts/winui`, after building `examples/storage/build/storage.dll` as above:

```powershell
dotnet test --project OspreyHost.Tests     # 51 tests: boundary, protocol, capabilities, folder walk, guards
dotnet publish OspreyHost -c Release       # Native AOT; trim and AOT warnings fail the build
```

Native AOT links with Visual Studio's C++ tools, so `vswhere.exe` must be on `PATH`: add `%ProgramFiles(x86)%\Microsoft Visual Studio\Installer`. Run the tests from PowerShell, not Git Bash. Git Bash's runtime enables the backup privilege, which reads folders their ACLs deny, so under Git Bash the denied-folder test is skipped and says why.

## Coverage

[`Coverage.cs`](OspreyHost.Core/Coverage.cs) is this host's column of the [coverage matrix](../../docs/specs/applications/0047-OspreyNativeHosts.md): 29 node kinds drawn, `scene` refused until plan 0038 phase 4 paints it with Win2D, and `measure-folders`, `reveal`, `choose-folder` and `cancel` answered. Every other capability completes with an error naming it. The tests read the framework's sources, so a kind added to `@osprey/native` fails them until this host draws, answers or refuses it.
