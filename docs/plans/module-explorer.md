# Module explorer delivery plan

**Status: planned, not implemented.** Record this requirement during stabilization; implementation starts after existing delivery gates are green. The binding contract is [EDITOR-MODULE-EXPLORER](../specs/tooling/EditorIntegration.md#module-explorer-editor-module-explorer). This is the Osprey counterpart to the module/solution explorers in Basilisk and SharpLsp.

## Delivery steps

- [ ] Inventory the existing checked source graph, project manifest, lock, overlay and verification APIs. Specify missing semantic data explicitly; reuse the compiler and package manager rather than introducing another resolver.
- [ ] Add the Module Explorer to the existing Osprey sidebar with project, module, namespace and source nodes plus organization-qualified direct/transitive dependencies, distinguishing locked releases and local overrides. Keep application decisions in Osprey and VS Code host capability glue in the extension.
- [ ] Implement document/declaration navigation, active-file reveal, refresh and accessible keyboard operation. Preserve selection across refreshes and isolate multi-root projects. Preserve read-only navigation in untrusted workspaces, exclude generated caches and refuse traversal/symlink escapes outside authorized roots.
- [ ] Expose the existing Manage Packages command with the selected project's context. Reflect completed package operations without duplicating mutation logic.
- [ ] Show real analysis errors, stale/loading/missing-lock states and package verification findings attached to affected items. Prove that missing data cannot appear as a clean empty result or a safe dependency.
- [ ] Add real editor E2E coverage for cross-flavor module membership, locked and local dependencies, source navigation, refresh after edits and package changes, project selection/cancellation, untrusted read-only navigation and mutation refusal, root containment and keyboard access. Assert membership from the actual compiler graph, not a synthetic tree fixture.
- [ ] Verify the installed VSIX and current-head CI gates before marking the explorer delivered. Record exact runnable acceptance commands and verified results in AGENTS.md at implementation time.

## Completion evidence

No implementation or passing explorer acceptance is claimed. The immediate package-manager discovery fix uses the existing sidebar toolbar and command; it does not satisfy the module explorer milestones above.
