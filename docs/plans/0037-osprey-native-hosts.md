# Plan 0037 — Osprey Native hosts: one framework, every renderer

Contract: [spec 0047](../specs/applications/0047-OspreyNativeHosts.md). Framework: [`packages/native`](../../packages/native/README.md). Sibling: [plan 0036](0036-gpui-desktop-host.md) delivers the GPUI host; this plan delivers what every host shares.

## Summary

Osprey Native already has two hosts that were built separately: Vigil's SwiftUI host and the VS Code webview host. The GPUI host will be the third and a standalone browser host the fourth. Nothing today states what a host must do, which nodes each one draws, or proves two hosts agree. This plan makes the host family explicit: a written aim, a conformance suite every host runs, a generated coverage matrix, a shared web renderer, and a framework with nothing host-specific left in it.

## Decisions

- **The package keeps the name `@osprey/native`.** Renaming it would break Vigil and the VS Code SDK for a word. The spec defines the name; the guide explains it.
- **Hosts are named by what they draw with:** SwiftUI, Webview, Browser, GPUI. Not by platform.
- **HTML/CSS is one renderer, two hosts.** The webview renderer in `vscode-extension/client/src/osprey-ui` is extracted into a package the VS Code extension and a plain browser page both import. Editor capabilities stay in the extension.
- **A host is listed as running only when it runs the conformance suite.**

## Phases

### Phase 1 — say what it is

1. [`website/src/docs/osprey-native.md`](../../website/src/docs/osprey-native.md): the aim, the host family and the boundary, written for someone choosing a framework. Done with this plan.
2. [`packages/native/README.md`](../../packages/native/README.md): open with the aim and the host table, link spec 0047, and fix the contradictory memory statements (line 44 says the Apple allocator never frees; line 152 says Apple archives default to ARC).

### Phase 2 — conformance suite

1. `packages/native/conformance/`: the probe application's start context, a recorded event sequence and the expected envelopes, produced once from `Runtime::start`/`dispatch` and checked into the repository.
2. An Osprey test that regenerates the envelopes from the fixtures and diffs them, so a framework change that alters the wire shape fails here first.
3. Each host runs the set through its own entry: the SwiftUI host in Vigil's smoke, the webview host in `npm test --prefix vscode-extension`, GPUI in `run.sh --smoke`.

### Phase 3 — coverage matrix

1. A script reads `node.ospml` and `capability.ospml` for constructor names and each host's mapping table for what it handles, and writes `packages/native/COVERAGE.md`.
2. CI regenerates it and fails on a diff, so a new constructor cannot land without every host's cell.
3. Each host README links its column.

### Phase 4 — shared web renderer

1. Extract `osprey-ui` (`protocol.ts`, `render.ts`, `styles.ts`, `theme.ts`, `icons.ts`, `session.ts`, `wasi.ts`) into `packages/web` as an npm package; `capabilities.ts` and the webview plumbing stay in the extension.
2. A browser host page that loads the WASI module in a worker and renders into a `div`, with file and store capabilities on the browser APIs that exist.
3. The probe application running in a browser tab and passing the conformance set.

### Phase 5 — framework neutrality

Tracked in [plan 0036 phase 3](0036-gpui-desktop-host.md): typed `Symbol` union, Vigil capabilities behind `HostCall`, `Evidence` and `Brand` generalized or removed, `platform` as a union. Landed once, benefits every host.

## Risks

- **Symbol vocabulary.** Done: a typed `Symbol` union with neutral lowercase names replaced the SF Symbols strings. The GPUI and webview hosts map every member and test it. Vigil keeps its vendored copy of the package until it upgrades, when its SwiftUI host maps the same names to SF Symbols.
- **Two renderers drifting.** The conformance suite is the only thing that stops the SwiftUI and HTML hosts disagreeing about a node. Phase 2 comes before any new host code.
- **Webview extraction.** `capabilities.ts` and `webview-client.ts` are intertwined with VS Code's message channel; the split needs the existing `OSPREY_TEST_GREP='compiled SDK renderer E2E'` tests green before and after.
