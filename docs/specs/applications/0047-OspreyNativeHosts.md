# Osprey Native and its hosts [NATIVE-HOSTS]

**Status:** two hosts run today: Vigil's SwiftUI host over the C ABI and the VS Code webview host over WASI frames ([`vscode-extension/client/src/osprey-ui`](../../../vscode-extension/client/src/osprey-ui)). A third, the GPUI desktop host ([0046](../platforms/0046-GpuiDesktopHost.md)), runs on macOS. The host conformance suite in [NATIVE-HOSTS-CONFORMANCE] does not exist yet; [plan 0037](../../plans/0037-osprey-native-hosts.md) delivers it.

## Aim [NATIVE-HOSTS-AIM]

Osprey Native is the application framework of the Osprey ecosystem. An application written against it owns everything a test can state: state, update, screens, actions, policy and the words a person reads. A **host** owns what only a platform can do: drawing, input devices, windows, files, network and the clock. The framework's promise is that one application runs on every host unchanged, and that nothing an application does depends on which host is drawing it.

The name is historical and stays for now. "Native" says the application compiles to native code and reaches native platform services; it does not say the host draws with native widgets. The name invites comparison with React Native, which is explicitly not a web framework; Osprey Native is the opposite: a host may draw with SwiftUI, HTML and CSS, GPU primitives or a terminal, and the web hosts are first-class, with the VS Code webview already running. The package keeps its name; hosts are named by what they draw with.

## What a host is [NATIVE-HOSTS-CONTRACT]

A host:

1. runs the application's `Runtime::start` and `Runtime::dispatch` through one of the transports in [NATIVE-HOSTS-TRANSPORT];
2. keeps the opaque session string and returns it unchanged with the next event;
3. keeps regions by key, applies `patches`, drops `removed`, and stores host-held `slices` by key;
4. draws every `Node` kind ([NATIVE-HOSTS-COVERAGE]) and returns events by the binding ids the framework issued, adding nothing of its own;
5. answers the capabilities it supports with completion events and rejects, with a diagnostic naming it, every capability and node kind it does not;
6. resolves theme tokens and symbols to its own colours, type scale and icons;
7. never interprets an action, a model or a string. The exception is layout and drawing optimisation that leaves the drawn result unchanged.

A host that draws a fallback for an unknown kind, invents an event, or stores application state outside the session is non-conforming.

## Transports [NATIVE-HOSTS-TRANSPORT]

| Transport | Build | Entry | Hosts |
| --- | --- | --- | --- |
| C ABI | `--target=macos`, `ios`, `ios-sim`, `android-*`, desktop targets as delivered | exported `start`/`dispatch` C functions, borrowed input, copied output | SwiftUI, GPUI |
| WASI frames | `--target=wasm32-wasi` | one JSON frame on stdin, one envelope on stdout, routed by `vscode::Webview::respond` | VS Code webview, browser |

Both carry the same envelope: `version`, `state`, `slices`, `patches`, `removed`, `requests`, `diagnostics` and, at start, `theme`. A transport may add nothing to it.

## Host family [NATIVE-HOSTS-FAMILY]

| Host | Draws with | Transport | Platforms | Where | Status |
| --- | --- | --- | --- | --- | --- |
| SwiftUI | SwiftUI controls | C ABI | macOS, iOS | [Vigil](https://github.com/Nimblesite/vigil) | running |
| Webview | HTML, CSS, Codicons | WASI frames | VS Code on every desktop | [`osprey-ui`](../../../vscode-extension/client/src/osprey-ui) | running |
| Browser | HTML, CSS | WASI frames in a worker | any browser | planned, extracted from the webview host | not started |
| GPUI | GPU primitives via GPUI | C ABI | macOS first, then Linux, Windows | `hosts/gpui` | [spec 0046](../platforms/0046-GpuiDesktopHost.md), running on macOS |
| WinUI | WinUI controls, Segoe Fluent Icons | C ABI, as a DLL | Windows | `hosts/winui` | [spec 0048](../platforms/0048-WinUiHost.md), tested without a window |

The webview and browser hosts share one TypeScript renderer; the VS Code one adds editor capabilities through [`@osprey/vscode`](../../../packages/vscode/README.md). A new host gets a row here and a column in the coverage matrix before any code merges.

## Node and capability coverage [NATIVE-HOSTS-COVERAGE]

Every `Node` constructor in [`node.ospml`](../../../packages/native/src/node.ospml) and every `Capability` in [`capability.ospml`](../../../packages/native/src/capability.ospml) appears in a coverage matrix, one column per host, each cell `draws`, `rejects` or `n/a` (a capability with no meaning on that platform). The matrix is generated from the sources, not written by hand, and CI fails when a constructor is missing from it. A host's README links its column.

Each host declares its column in code, and its tests read the framework's sources and fail when a kind is missing from the declaration or declared twice:

| Host | Column | Test |
| --- | --- | --- |
| GPUI | `DRAWN`, `REFUSED`, `ANSWERED` and `UNANSWERED` in [`coverage.rs`](../../../hosts/gpui/src/coverage.rs) | `cargo test coverage`, which also checks the lists against the host's match arms |
| Webview | `RENDERED_KINDS` in [`render.ts`](../../../vscode-extension/client/src/osprey-ui/render.ts), `ANSWERED_CAPABILITIES` in [`capabilities.ts`](../../../vscode-extension/client/src/osprey-ui/capabilities.ts) | the renderer and capability suites in `vscode-extension/test/suite` |
| WinUI | [`Coverage.cs`](../../../hosts/winui/OspreyHost.Core/Coverage.cs) | `CoverageTests`, with the renderer refusing to start unless it draws exactly the declared kinds |

The typed symbol helpers provide a closed, host-neutral vocabulary: the `Symbol` union in [`symbol.ospml`](../../../packages/native/src/symbol.ospml), converted by `Symbols::name` to each member's lowercase string. Existing control constructors also accept string names and bundled SVG registrations. Each host maps every member to its own icon set and tests that it does (the GPUI host's `every_framework_symbol_has_a_kit_icon`, the webview's Codicon test), so no member is drawn as a fallback. The framework holds no host's icon names, capabilities or product vocabulary; work only one host can do is a `HostCall`.

## Conformance [NATIVE-HOSTS-CONFORMANCE]

One recorded-event fixture set, kept beside the framework in `packages/native/conformance`, drives every host: the probe application's start context and a sequence of events with the expected envelopes. A host passes when it produces the same envelopes, draws without a rejected kind, and returns the same binding ids for the same user actions scripted through its own input layer. Each host's smoke command runs the set; the plan names them. A host not in the conformance run is not listed as running in [NATIVE-HOSTS-FAMILY].

## Framework neutrality [NATIVE-HOSTS-NEUTRAL]

`@osprey/native` carries nothing that only one host or one product can honour. Platform-specific work goes through `HostCall` with a namespace the host answers or rejects. Constructors and capabilities that exist for one product are moved out or generalized; [plan 0036 phase 3](../../plans/0036-gpui-desktop-host.md) lists the current ones.
