# @osprey/vscode

An Osprey SDK for hosting [Osprey Native](../native/README.md) applications inside an existing VS Code extension. Application state, typed component trees, signals, bindings and capability decisions stay in Osprey. The VS Code adapter renders the framework's keyed patches, returns framework-issued event IDs and executes authorized host operations. It keeps the framework session opaque.

`vscode::Editor` constructs bounded framework requests for workspace snapshots, opening documents, scoped configuration, revealing registered views and running an authorized executable with explicit stdin and workspace. It rejects missing tickets, invalid resource schemes and invalid limits. The host independently enforces workspace trust, resource confinement, executable authorization, process serialization and hard bounds. No command uses a shell.

`vscode::Webview::respond` routes a WASI frame to the application's actual `Runtime::start` or `Runtime::dispatch` entry points. A `start` frame carries a host context; a `dispatch` frame carries the opaque session and a framework event. Invalid frames return an explicit boundary error. The application entry supplies only stdin and stdout. Existing VSIX features stay native to the extension; only the Osprey application is compiled to WASM.

Pending text edits retain their original region, revision and ordinal. The host selects the original opaque session from bounded issued history, and Osprey checks the actual typed binding against current controls. Explicit host receipts prevent normalized or rejected values from being replayed. Missing or ambiguous bindings and expired snapshots report refusal. The browser preserves a newer draft when an older receipt arrives, and keeps edit errors alongside later host diagnostics.

Run the canonical SDK tests:

```sh
target/release/osprey packages/vscode/tests --run --quiet
python3 scripts/package_coverage.py --collect --package vscode
```

Expect six passing cases and 100% own-source line coverage. [Tests](tests/main.ospml) cover resource scopes, subprocess input and bounds, invalid requests, and start/dispatch routing. [The package-manager application](../manager/editor/) is the first consumer. `OSPREY_TEST_GREP='package workspace|compiled SDK renderer E2E' npm test --prefix vscode-extension` exercises its actual VS Code/WASM host, including normalization, pending edits, removed bindings and security warnings.
