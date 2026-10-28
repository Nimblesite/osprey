# WinUI host

`OspreyHost.exe <app.dll> [arguments]` opens one window and draws the [Osprey Native](../../packages/native/README.md) application compiled into `app.dll` with WinUI, the native UI framework of the Windows App SDK. It uses Mica and Segoe Fluent Icons, with no XAML anywhere. The host is generic: every application decision, word and screen is Osprey, and the host only draws regions, returns events by the ids the framework issued and answers capabilities. The contract is [spec 0048](../../docs/specs/platforms/0048-WinUiHost.md) [WINUI-HOST]; delivery is [plan 0038](../../docs/plans/0038-winui-windows-hosts.md).

| Project | Holds |
| --- | --- |
| `OspreyHost.Core` | The DLL boundary and import slots, envelopes, the region store, events, the scene decoder, the capability adapters, the Windows folder walk and the store; nothing here needs a window |
| `OspreyHost` | The window, the code-only renderer and scene painter, the frame clock, the folder picker and `--self-test`; Native AOT |
| `OspreyHost.Tests` | xUnit v3 on Microsoft.Testing.Platform |

## Run

From the repository root on Windows, with MSYS2 UCRT64's clang and llvm-ar as `OSPREY_CC` and `OSPREY_AR`, build an application into a DLL and pass it to the host:

```sh
./target/release/osprey build examples/storage --target=windows-x64 -o examples/storage/build/storage.dll
./target/release/osprey build examples/gpui/flock --target=windows-x64 -o examples/gpui/flock/build/flock.dll
./target/release/osprey build examples/gpui/painting --target=windows-x64 -o examples/gpui/painting/build/painting.dll
dotnet run --project hosts/winui/OspreyHost -c Release -- examples/gpui/flock/build/flock.dll
```

`examples/storage/run.sh --host=winui` does the build and run for Storage. The window must open in a desktop session: a shell running as a Windows service, such as an SSH session in session 0, can run every test but not the window, and the host says so and exits with code 3.

## Test, publish and self-test

From PowerShell in `hosts/winui`, after building the DLLs as above:

```powershell
dotnet test --project OspreyHost.Tests     # 75 tests: boundary, protocol, scenes, capabilities, walk, store, guards
dotnet publish OspreyHost -c Release       # Native AOT; trim and AOT warnings fail the build
./self-test.ps1 -Exe OspreyHost/bin/Release/net10.0-windows10.0.26100.0/win-x64/publish/OspreyHost.exe `
    -Dll ../../examples/gpui/flock/build/flock.dll -Script ../../examples/gpui/flock/self-test.json -Shots shots
```

Native AOT links with Visual Studio's C++ tools, so `vswhere.exe` must be on `PATH`: add `%ProgramFiles(x86)%\Microsoft Visual Studio\Installer`. Run the tests from PowerShell, not Git Bash. Git Bash's runtime enables the backup privilege, which reads folders their ACLs deny, so under Git Bash the denied-folder test is skipped and says why.

`--self-test` drives the real window from a script beside each application ([WINUI-HOST-SELF-TEST]) and keeps a picture of the window after each `shot` step; `examples/storage/self-test.ps1` builds Storage's fixture tree first. CI runs Storage, Flock and Painting this way on the runner's desktop and keeps the pictures, then runs Storage again on the whole C drive with `examples/storage/self-test-drive.json` and `-Arguments 'C:\'`.

## Coverage

[`Coverage.cs`](OspreyHost.Core/Coverage.cs) is this host's column of the [coverage matrix](../../docs/specs/applications/0047-OspreyNativeHosts.md): every node kind drawn, scenes included, and `measure-folders`, `reveal`, `choose-folder`, `cancel`, the four store kinds and `animate` answered. Every other capability completes with an error naming it. The tests read the framework's sources, so a kind added to `@osprey/native` fails them until this host draws, answers or refuses it.
