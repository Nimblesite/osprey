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
- [ ] Cast to a real TV and record the result here
- [x] Windows: the WinUI host runs UI machines; signal flow is a sandboxed package with a self-test (`make -C apps/windows signal-flow`, `make -C apps/windows test`)
- [ ] Windows casting with `DnsServiceBrowse`, the MinGW TLS and file-server runtime and `privateNetworkClientServer` (plan 0046 step 5), then `make -C apps/windows casting`
- [ ] `make ui-test` on the Windows CI runner
