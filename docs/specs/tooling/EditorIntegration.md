# Editor integration and distribution

The corresponding LanguageServerAndEditors contract is [here](0020-LanguageServerAndEditors.md).

## Editor integrations

The VS Code integration is a thin client over `[LSP-TRANSPORT]`.

### VS Code `[EDITOR-VSCODE]`

- Extension id `nimblesite.osprey`; client in
  [`vscode-extension/client/src/extension.ts`](../../../vscode-extension/client/src/extension.ts)
  spawns `osprey lsp` over stdio.
- Packaged as a **per-platform VSIX** (`darwin-arm64`, `linux-x64`,
  `win32-x64`). Each VSIX **bundles** a version-matched `osprey` binary + runtime
  libs + a stamped `shipwright.json`, verified present inside the package at build
  time.
- Client resolves the server command in priority order: user setting
  (`osprey.server.compilerPath`) → bundled binary → `PATH` (per the Shipwright
  `sources` list in [`shipwright.json`](../../../shipwright.json)).
- The extension's native DAP integration is specified in
  [Debugger](0021-Debugger.md); it is separate from the LSP request path.
- Marketplace publication uses **OIDC** (no PAT) — see `[EDITOR-VERSIONING]` and
  the [release workflow](../../../.github/workflows/release.yml). Open VSX
  publication uses the same VSIX artifacts and an independent optional-token
  job, so either registry can succeed alone.

## Versioning & supply chain `[EDITOR-VERSIONING]`

The VS Code distributions obey the
[Shipwright](https://github.com/Nimblesite/Shipwright) version contract: the
extension and bundled binary MUST be version-matched.

- The binary is the source of truth: `osprey --version` → `osprey X.Y.Z`;
  `osprey --version --json` → the version manifest (`[SWR-VERSION-CLI-OUTPUT]`).
- Components are declared in [`shipwright.json`](../../../shipwright.json):
  `osprey` (the CLI — which _is_ the language server, via the `lsp`
  subcommand) and `osprey-vscode`. The component id **must** equal the name the
  binary reports from `osprey --version` (Shipwright matches the probed name
  against the component id), so the CLI component is `osprey`, not
  `osprey-compiler`. The LSP is **not** a separate component; it is
  the same binary, so no separate version surface exists to drift.
- Source version fields stay at `0.0.0-dev`; the release version is stamped
  from the git tag at build time (`[SWR-VERSION-BUILD-STAMPING]`). Hard-coding
  a version is a defect.
- VS Code activation verifies the bundled compiler against the manifest and
  prompts to reinstall on mismatch (`hosts.vscode.onMismatch`). PATH/registry
  sources are verified at startup (`verifyStartup`).
- Marketplace publishing uses GitHub OIDC and Microsoft Entra
  workload-identity federation, with no stored PAT.

## Package manager discovery `[EDITOR-PACKAGE-ACTION]`

The existing Osprey sidebar's Program and Processes headers expose a **Manage Packages** toolbar action, with an accessible name and package icon, invoking `osprey.packages.open`. The action remains available without a running debug session or profiler. It opens the existing package workspace rather than a second implementation. Multi-root workspaces require explicit project selection; cancellation performs no operation. The Command Palette and manifest context menu remain additional entry points.

## Module explorer `[EDITOR-MODULE-EXPLORER]`

**Status: planned; not implemented.** Delivery is tracked in the [module explorer plan](../../plans/module-explorer.md). Provide a Module Explorer in the existing Osprey sidebar, analogous in purpose to Basilisk's module browser and SharpLsp's Solution Explorer.

The tree groups workspace projects, their declared modules, namespaces and source files, then direct and transitive package dependencies with organization-qualified identities. Resolve semantic membership and imports through the compiler's checked project/source graph, including all supported syntax flavors; directory names alone cannot establish module membership. Distinguish local overrides and locked releases. Do not infer that a package is removable from source-graph data alone.

Opening a source item navigates to its actual document and declaration range when supplied by the compiler. Refresh, active-file reveal and keyboard navigation preserve project context. The explorer exposes the existing Manage Packages action for the selected project; it delegates mutations to the package manager and never edits manifests itself.

Loading, stale data, missing locks and failed project analysis have explicit states; unavailable information is never shown as an empty successful tree or invented zero counts. Display package verification findings with the affected item; do not expose a dependency inventory that cannot report its available security warnings. Never label an unchecked package safe. Read-only navigation works in untrusted workspaces; operations requiring trust are refused explicitly. Exclude generated caches and prevent navigation outside authorized workspace/dependency roots through traversal or symlinks.

Acceptance requires real editor E2E tests against multi-root projects, cross-flavor modules, local overrides and locked dependencies. Assert source navigation, project-scoped Manage Packages, cancellation, refresh after edits/add/remove, failed analysis, verification findings and keyboard operation. Tests must prove compiler-backed membership rather than a synthetic tree fixture.
