# Module explorer delivery plan

**Status: the sidebar view runs on real compiler data; an automated keyboard test remains.** Record this requirement during stabilization; implementation starts after existing delivery gates are green. The binding contract is [EDITOR-MODULE-EXPLORER](../specs/tooling/EditorIntegration.md#module-explorer-editor-module-explorer). This is the Osprey counterpart to the module/solution explorers in Basilisk and SharpLsp.

## Delivery steps

- [x] Inventory: `osprey <project> --source-graph` already gives sources and checked dependency edges, and now gives each source's namespaces and modules with positions (`declarations`). Direct and transitive packages, locked versions, overrides and verification findings come from the package frontend behind `osprey.packages.open`, so no second resolver is needed.
- [x] Add the Module Explorer to the existing Osprey sidebar with project, module, namespace and source nodes and organization-qualified direct dependencies (`client/src/module-explorer.ts`). The tree holds no application decisions: membership comes from the compiler and dependencies from the Osprey package client.
- [x] Transitive dependencies nest from `osprey-pkg tree --graph`, showing each locked release and what is not locked.
- [x] Local overrides are named on their package, from the development lock through `tree --graph`.
- [x] Document/declaration navigation, active-file reveal, refresh on edits, stable ids across refreshes, one node per project, generated caches excluded, and keyboard operation through the native tree.
- [x] In an untrusted workspace the view runs nothing, says why, and still navigates source files; traversal and symlink escapes outside the workspace are refused.
- [x] The existing Manage Packages command is on the view's toolbar, and manifest and lock changes refresh the tree without duplicating mutation logic.
- [x] Each project row has a Manage Packages action carrying that project.
- [x] Real analysis and dependency errors show as items, and an incomplete graph or report is refused rather than read as an empty result.
- [x] Missing-lock, stale and loading states, and package verification findings on the affected items.
- [x] Editor E2E coverage against the actual compiler graph for cross-flavor membership, source navigation, refresh after edits, analysis failures and untrusted refusal (`test/suite/module-explorer.test.ts`).
- [x] E2E coverage for the locked graph, verification findings, unsaved edits and root containment.
- [x] Local dependencies are covered through the graph report (`tree_graph_tests.osp`) and the view.
- [x] E2E coverage through the real package client: per-project answers, a manifest change after refresh, and a rejected manifest.
- [ ] An automated keyboard test; the view is VS Code's native tree, whose keyboard operation is not separately exercised here.
- [x] The installed VSIX contributes the view and its commands (`test/installed/packages.test.ts`, run by `npm run test:vsix` in the required extension job).

## Completion evidence

`test/suite/module-explorer.test.ts` runs against the real compiler and package client, and `test/installed/packages.test.ts` against the installed VSIX, both in the required extension job.
