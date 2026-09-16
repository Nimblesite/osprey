# Testing the Osprey VS Code extension

The extension has two complementary suites. Every assertion is enforced: failures are neither caught as expected outcomes nor skipped when the compiler is missing.

Run the complete extension gate from the repository root:

```sh
make build
make _test_vscode_extension _coverage_check_vscode_extension
```

This builds the extension, runs the existing development-host suite with V8 coverage, packages a VSIX, installs it into an isolated VS Code profile, and runs the installed-package editor tests. All four coverage metrics remain subject to `coverage-thresholds.json`.

## Development-host tests

`test/suite/**/*.test.ts` tests language features, activation, debugging, profiling, test discovery and documentation. `.vscode-test.js` loads these tests inside VS Code and measures only `out/client/src/**/*.js`; test helpers are excluded from product coverage.

```sh
cd vscode-extension
npm ci
npm test
```

For a focused development run, set `OSPREY_TEST_GREP` to a Mocha grep expression. Do not use this filter for the complete gate.

```sh
OSPREY_TEST_GREP='Debugger E2E Workflows' npm test
```

The debugger suites require the real compiler and `lldb-dap`. The development suites prefer the compiler under `target/release`; the Makefile restages that binary into the extension bundle before running them.

## Installed VSIX tests

`test/installed/` runs against an installed package, not the development extension. `scripts/test-installed-vsix.mjs` creates temporary workspace, user-data and extension directories, installs the actual VSIX using VS Code's CLI, and launches an external Mocha runner. VS Code requires a development path to run tests; that path points to an inert test-driver extension containing no Osprey code. The Osprey extension itself is loaded only from the installed VSIX.

The suite proves that the installed extension lives in the isolated extension directory, contains no shipped test files, and bundles a compiler whose SHA-256 matches `target/release/osprey`. The installed activation and warning-fix JavaScript must also match the compiled sources. Compiler-path settings stay empty. A failing `osprey` executable at the front of PATH detects any accidental fallback to a different compiler; Shipwright's deliberate `--version` candidate probes are allowed, but any other invocation fails the test. The temporary profile never changes the user's installed extensions or settings.

To run only this suite after a compiler build:

```sh
make _vsix_bundle _vsix_package
cd vscode-extension
npm run test:vsix
```

An explicit package can also be supplied:

```sh
npm run test:vsix -- /absolute/path/to/osprey-0.0.0-dev.vsix
```

The installed tests open real Default and ML documents, wait for published compiler diagnostics, request code actions, invoke the editor's Quick Fix and fix-all commands, and exercise edits, saves, undo and redo. Assertions pin diagnostic count, code, severity, message, source, range and unused-symbol tags, together with exact resulting source. They verify comments, Unicode, line endings, required signatures and changing documents. The compiler and language server are not mocked.

Cached actions are tested after edits to the target document, an unsaved project sibling, an unopened sibling on disk, and the manifest's source roots. The VSIX checks document versions and contents again when the action runs because the language-client conversion discards the protocol edit's version. It also requests a fresh compiler proof and rechecks the editor snapshots after that request finishes. Required signatures stay intact when project changes alter what inference can prove.

The separate installed suite does not replace or narrow development-host coverage. `make _test_vscode_extension` requires both suites to pass.

## Adding assertions

Extend an existing suite where its fixture naturally fits. Use the real VS Code API for editor behavior and exact expected values for diagnostics and edits. Tests must fail if a provider, compiler, diagnostic or action is missing. Do not catch failures as expected behavior, use permissive count assertions, or add sleeps that conceal missing synchronization.

Compile TypeScript before running a focused test:

```sh
npm run test-compile
```

VS Code APIs are available only inside the test host; running these suites with plain Node/Mocha will fail to resolve `vscode`.
