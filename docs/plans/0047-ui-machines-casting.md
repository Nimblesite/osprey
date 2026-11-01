# Plan 0047 — UI machines, the casting player and the UI test framework

Specs: [UiMachines](../specs/platforms/UiMachines.md) ([UI-MACHINE-TEST], [UI-MACHINE-A11Y]), [0019](../specs/compiler/0019-FFI.md) ([FFI-TLS-CLIENT], [FFI-FILE-SERVER-FOLDER], [FFI-MEDIA-RENDER]), casting design ([CAST-DESIGN-NATIVE]). Windows: [plan 0046](0046-apps-folder-windows-sandbox.md) step 5. Status: **macOS done 2026-11-01; Windows and a real-TV run open.**

## What the owner asked for

Finish the casting app, the UI toolkit it runs on with full accessibility, and the automated test framework that drives it. Keep JSON at the edges: one decoder, typed records past it.

## Checklist

- [x] Typed machine document: `MachineDocs::load` decodes once into `Value`, `Condition`, `Template`, `Stamp` and `Area` records; nothing past it reads JSON
- [x] `@osprey/json` (`Doc`) is the only JSON layer for machines, encode and uitest
- [x] Machines, Templates and Canvases on typed records: layers, transitions, `each`, menus, skins
- [x] Accessible semantics: role, label, bounds, value, min and max for every control
- [x] GPUI host maps each element to an AccessKit node and turns screen-reader click, set-value, increment and decrement into pointer events
- [x] `@osprey/uitest`: `Read`, `Views`, `Semantics`, `Events` and `Drive`, with its own suite
- [x] Casting test drives the player by accessible label alone, asserting roles, values and ranges
- [x] Signal-flow suite ported to the framework
- [x] `make ui-test` and the CI step run the framework, casting and signal-flow suites
- [x] Casting: open files, MP3 association, TLS deadline fix, HLS visualiser
- [x] `make -C apps/macos casting`; desktop apps split into `apps/macos` and `apps/windows`
- [x] Cast to a real TV and record the result here: 2026-11-01, "Family room TV" (Chromecast Ultra), `packages/chromecast/live` through `Wire.Native`, the player the casting app runs: the HLS visualiser played, paused, resumed, sought, kept its volume and finished, 7 of 7. The first run found the receiver refusing the media because the file server sent no CORS header; it now sends `Access-Control-Allow-Origin: *` ([FFI-FILE-SERVER-FOLDER])
- [x] Windows: the WinUI host runs UI machines; signal flow is a sandboxed package with a self-test (`make -C apps/windows signal-flow`, `make -C apps/windows test`)
- [ ] Windows casting with `DnsServiceBrowse`, the MinGW TLS and file-server runtime and `privateNetworkClientServer` (plan 0046 step 5), then `make -C apps/windows casting`
  - 2026-11-01: the file server and TLS client compile under MinGW, and media render returns "" there, so a Windows sender casts plain audio
  - 2026-11-01, built: the WinUI host answers `run` and `choose-file` (`Processes`, a `FileOpenPicker`; host tests compile on the Mac, they run only on Windows); the casting app takes `--browse <helper>` (casting suite, 5 of 5); `examples/casting/windows/cast-browse.c` browses with `DnsServiceBrowse` and `DnsServiceResolve` and prints the SRV and TXT lines `Discovery.browse` reads (MinGW, `-Wall -Wextra -Werror`, imports only `DNSAPI` and the CRT); a Windows DLL that uses TLS, the file server or media render now links the HTTP runtime and static OpenSSL; `apps/windows.json` has the casting entry with `privateNetworkClientServer`, and `app.ps1` builds its helper
  - Open: `make -C apps/windows casting` on a Windows machine on the same network as a Cast device. `osprey build --target=windows-x64` runs only on Windows, so neither the DLL link nor the browse has run yet
- [ ] `make ui-test` on the Windows CI runner: the step is in ci-windows.yml; tick once a PR run passes
