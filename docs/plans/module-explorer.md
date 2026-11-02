# Module explorer delivery plan

**Status: the sidebar view runs on real compiler data; local overrides and installed-VSIX acceptance remain.** Record this requirement during stabilization; implementation starts after existing delivery gates are green. The binding contract is [EDITOR-MODULE-EXPLORER](../specs/tooling/EditorIntegration.md#module-explorer-editor-module-explorer). This is the Osprey counterpart to the module/solution explorers in Basilisk and SharpLsp.

## Delivery steps

- [x] Inventory: `osprey <project> --source-graph` already gives sources and checked dependency edges, and now gives each source's namespaces and modules with positions (`declarations`). Direct and transitive packages, locked versions, overrides and verification findings come from the package frontend behind `osprey.packages.open`, so no second resolver is needed.
- [x] Add the Module Explorer to the existing Osprey sidebar with project, module, namespace and source nodes and organization-qualified direct dependencies (`client/src/module-explorer.ts`). The tree holds no application decisions: membership comes from the compiler and dependencies from the Osprey package client.
- [x] Transitive dependencies nest from `osprey-pkg tree --graph`, showing each locked release and what is not locked.
- [ ] Distinguish local overrides, which the graph report does not carry yet.
- [x] Document/declaration navigation, active-file reveal, refresh on edits, stable ids across refreshes, one node per project, generated caches excluded, and keyboard operation through the native tree.
- [x] In an untrusted workspace the view runs nothing, says why, and still navigates source files; traversal and symlink escapes outside the workspace are refused.
- [x] The existing Manage Packages command is on the view's toolbar, and manifest and lock changes refresh the tree without duplicating mutation logic.
- [x] Each project row has a Manage Packages action carrying that project.
- [x] Real analysis and dependency errors show as items, and an incomplete graph or report is refused rather than read as an empty result.
- [x] Missing-lock, stale and loading states, and package verification findings on the affected items.
- [x] Editor E2E coverage against the actual compiler graph for cross-flavor membership, source navigation, refresh after edits, analysis failures and untrusted refusal (`test/suite/module-explorer.test.ts`).
- [x] E2E coverage for the locked graph, verification findings, unsaved edits and root containment.
- [ ] E2E coverage for local dependencies, package changes, project selection/cancellation and keyboard access.
- [ ] Verify the installed VSIX and current-head CI gates before marking the explorer delivered. Record exact runnable acceptance commands and verified results in AGENTS.md at implementation time.

## Completion evidence

No explorer acceptance is claimed. The immediate package-manager discovery fix uses the existing sidebar toolbar and command; it does not satisfy the module explorer milestones above.
