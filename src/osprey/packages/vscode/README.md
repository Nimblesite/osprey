# @osprey/vscode

An Osprey SDK for hosting [Osprey Native](../native/README.md) applications inside an existing VS Code extension. Application state, typed component trees, signals, bindings and capability decisions stay in Osprey. The VS Code adapter draws the shared rows each frame leases, returns typed events that name a control by its number, and executes authorized host operations. The session never leaves the application.

## Capabilities [VSCODE-SDK-CAPABILITIES]

`vscode::Editor` constructs bounded framework requests for workspace snapshots, opening documents, scoped configuration, revealing registered views and running an authorized executable with explicit stdin and workspace. It rejects missing tickets, invalid resource schemes and invalid limits. The host independently enforces workspace trust, resource confinement, executable authorization, process serialization and hard bounds. No command uses a shell.

## Boundary [VSCODE-SDK-WASM]

An application's entry defines the same three exports every Osprey Native application has (`argument`, `open`, `event`), so its WebAssembly build is a live module that exports them with the bridge leases ([NATIVE-SHM-WASM]). The extension runs `_start` once in a worker, calls the exports, and leases each frame's view, scene, text and ask runs straight from linear memory; the webview receives one copy of those raw rows, checks every row and draws it. Existing VSIX features stay native to the extension; only the Osprey application is compiled to WASM.

Run the canonical SDK tests:

```sh
target/release/osprey src/osprey/packages/vscode/tests --run --quiet
python3 scripts/packages/package_coverage.py --collect --package vscode
```

Expect five passing cases and 100% own-source line coverage. [Tests](tests/main.ospml) cover resource scopes, subprocess input and bounds, and invalid requests. [The package-manager application](../manager/editor/) is the first consumer. `OSPREY_TEST_GREP='package workspace|compiled SDK renderer E2E' npm test --prefix src/toolchain/vscode-extension` exercises its actual VS Code/WASM host.

## Origin and license

Original Osprey code, not a port of another project. MIT, Copyright NIMBLESITE PTY LTD, Australia; see [LICENSE](LICENSE) and [NOTICE.md](NOTICE.md).
