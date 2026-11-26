# Writing about Osprey Native

This brief governs every word about **Osprey Native** (`@osprey/native`): its standalone website, README, docs, demos, talks, and posts. It is written to travel cleanly when Osprey Native is broken out of the Osprey repository. Pair it with the [Osprey Native design system](../../src/toolchain/osprey-native-website/design_system.md) and the [Osprey messaging guide](osprey.md).

## The answer

> **Osprey Native builds real native apps without forcing an either/or choice: render real platform controls (SwiftUI, WinUI, Android) or render straight to a GPU canvas (FerroUI / GPUI) — from one AOT-compiled codebase, in your syntax flavor, with signals and enforced accessibility from the ground up.**

Second sentence, when there is room:

> **Write your state, logic, and UI once against typed signals and effects. Ship ahead-of-time native binaries to macOS, Windows, iOS, and Android, and WebAssembly to the browser — with zero JavaScript engine on native targets and a built-in accessibility tree that tests and AI agents drive by label.**

Every longer pitch expands these two sentences and never replaces them. Lead with real apps and real capabilities, never abstract framework slogans.

## Core pillars

1. **Real native apps, AOT from the start.**
   Application state, logic, and screens compile ahead of time to native machine code (static archives on macOS/iOS/Android, DLLs on Windows) and WebAssembly on the web. Native builds ship no JavaScript engine, no JS bundle, and no JIT warm-up. Every frame crosses a zero-serialization, lease-checked shared-memory bridge (`ViewRow`, `SceneShape`, UTF-8 text run, `AskRow`) generated from one `typeDiagram` schema (`native.td`).

2. **Canvas OR native controls — not either/or.**
   Choose **real platform controls** (SwiftUI on macOS and iOS, WinUI on Windows, Android's own views on Android) when you want authentic OS look-and-feel, native text fields, scrolling, and system conventions. Choose **GPU canvas rendering** (FerroUI / GPUI — pixel-consistent across platforms, like Flutter) when you want custom scenes, instruments, DAWs, or skinnable interfaces. Mix them where the app demands it: both run on the same state, signals, and shared-memory bridge.

3. **Signals from the ground up.**
   Reactivity is built into the language and framework (`Signal<T>`, `Keyed::region`, `signalRefreshKeyed`), not bolted on via hooks, dependency arrays, or manual dirty flags. The compiler derives each view region's read dependencies automatically, refreshing only the regions whose signals changed.

4. **All platforms, including Web.**
   **macOS · Windows · iOS · Android · Web (WebAssembly) · VS Code webview.**
   Name Windows whenever listing desktop platforms. Every desktop app targets Windows via WinUI (Windows App SDK, sandboxed MSIX AppContainer by default).

5. **Write in the syntax flavor your team already knows.**
   One language, one type system, one AOT compiler — multiple syntax flavors:
   - **Default Osprey** (`.osp`, braces)
   - **ML flavor** (`.ospml`, indentation-significant)
   - **Python flavor** (`# osprey: flavor=python`)
   - **TypeScript flavor**
   - **Roadmap:** **Dart flavor** coming soon, and **Kotlin flavor** under consideration.

   Always say **syntax flavor** and define it on first mention: same Hindley-Milner type inference, same algebraic effects, same native compilation. Never imply arbitrary CPython, npm, Dart, or Kotlin packages run unchanged.

6. **Powerful automated testing framework (`@osprey/uitest`).**
   Updates are pure functions and platform I/O sits behind typed algebraic effects (`Wire`, `Slices`, `ViewStream`). `@osprey/uitest` drives the app's real `start` and `dispatch` exports headlessly — no window, no browser, no simulator, no flakiness. `tap` fires real pointer press/release events at control bounds; `letters` asserts what is actually drawn on the canvas; effect handlers swap in deterministic mocks for networks, disks, and devices.

7. **Accessibilitymaxxing — enforced at the framework level for humans and AI agents.**
   Accessibility is not an afterthought bolted onto divs or optional modifiers. Every control in a UI machine declares a typed `AccessRole` (`Pressable`, `Slidable`, `Popup`, `MenuItem`, `Unannounced`) and a live template label (`Accessible` in `accessibility.td`). `Canvases::semantics` emits the live accessibility tree on every frame, `Canvases::act` operates controls by label, and the GPUI host maps every entry to AccessKit (NSAccessibility on macOS, UI Automation on Windows, AT-SPI on Linux). Disabled or hidden controls disappear from the tree, so **both screen readers and AI agents** see the exact set of legal actions — tested on every build.

8. **Safety without panics.**
   No panics, no exceptions, no silent corrupt frames. Every failure is a typed `Result`, every action match is exhaustive, every bridge lease is generation-checked and bounds-checked, and malformed frames are refused whole while keeping the last good frame on screen.

## Positioning: Flutter, React Native, and Compose Multiplatform (CMP)

Position Osprey Native clearly and fairly through concrete architectural trade-offs — never vague hype:

| Dimension | Osprey Native | Compose Multiplatform (CMP) | Flutter | React Native |
| --- | --- | --- | --- | --- |
| **Rendering choice** | **Both:** real native controls (SwiftUI, WinUI, Android) **OR** GPU canvas (FerroUI / GPUI) | Primarily Skia canvas (native interop requires wrapping platform views) | Impeller / Skia canvas only (copies of platform widgets) | Native platform views only (custom GPU scenes require native modules) |
| **Runtime & compilation** | AOT native machine code (LLVM) + WebAssembly; no JS engine, no JVM | Kotlin/Native on iOS, JVM on desktop/Android, Wasm/JS on web | Dart AOT on mobile/desktop, JS/Wasm on web | JavaScript engine (Hermes) + native bridge / JSI |
| **Reactivity** | Compiler-derived signals (`Signal<T>`) & keyed regions from the ground up | Compose Snapshot state + compiler plugin recomposition | Widget tree rebuilds / third-party state packages | React hooks / virtual DOM / third-party signals |
| **Language choice** | Default Osprey, ML, Python, TypeScript flavors (Dart coming soon; Kotlin considered) | Kotlin only | Dart only | JavaScript / TypeScript only |
| **Desktop & Windows** | Native SwiftUI (macOS), native WinUI (Windows), or FerroUI GPU canvas | Skia-drawn desktop UI (JVM / Swing window host) | Custom canvas in Win32 / macOS shell | Community-maintained Windows/macOS forks |
| **Automated UI testing** | Headless deterministic `@osprey/uitest` by accessible label + algebraic effect mocks | Compose UI test rules (platform/JVM test runner) | Widget tests + integration_test | Jest + Detox / Appium |
| **Framework-level accessibility & AI agents** | Typed `Accessible` tree emitted by UI machines (`semantics` / `act`) + AccessKit mapping; tested by label on every build | Semantics modifiers (opt-in per composable) | Semantics widgets (opt-in) | Accessibility props per view (opt-in) |

### Emphasize where Osprey Native is ahead of Compose Multiplatform (CMP)

Be concrete and fair — never claim completeness where work is still in progress, and always keep qualifications beside claims:

1. **True native controls on iOS, macOS, and Windows — not Skia lookalikes.** CMP's shared UI draws pixels with Skia on iOS and desktop (and hosts desktop inside a JVM/Swing window). Osprey Native lets you render genuine SwiftUI on macOS/iOS, genuine WinUI on Windows, and native Android views — **or** switch to FerroUI's GPU canvas when you want identical pixel control. It is not either/or.
2. **No JVM on desktop, no serialization bridge.** Osprey Native compiles AOT to native libraries everywhere and passes fixed-width `ViewRow` / `SceneShape` frames in shared memory with zero JSON or tree serialization.
3. **Multi-flavor authoring instead of Kotlin lock-in.** Teams coming from Python, TypeScript, ML, or braces-style languages can write Osprey Native in their preferred syntax flavor (with Dart flavor coming soon and Kotlin flavor under consideration), sharing one type-checked AOT core.
4. **Enforced accessibility trees designed for AI agents.** Instead of relying on developers remembering to add semantics modifiers, UI machines make roles and labels structural, driving both screen readers (AccessKit) and headless `@osprey/uitest` journeys by label — ready for AI agents to automate without squinting at screenshots.
5. **Algebraic effects for deterministic headless testing.** Swapping the network, filesystem, or Chromecast wire in a test is a one-line effect handler — no DI container, no mocking framework, no simulator boot required.

## Real apps to show (proven today)

Name only products that run today, and state which hosts run them:

- **Tally** — cross-platform counter running on macOS (SwiftUI), iOS (SwiftUI), Android (native views), and Windows (WinUI).
- **Issue Inbox** — GitHub issue browser running on iOS and Android simulators/emulators.
- **Storage, Flock, Painting, Casting** — Windows WinUI apps and macOS FerroUI / GPUI GPU-canvas apps (Flock streams 5,000 sprites at ~7 ms/frame including `update`).
- **Prism DAW & Prism Synth** — audio studio and synthesizer rendered through the FerroUI / GPUI host on macOS (patch editor on Windows).
- **Talon Bank** — shared Osprey Native app running on WebAssembly in Chromium alongside mobile hosts.
- **Osprey Agent** — coding agent built with Osprey Native on macOS (note: not sandboxed).
- **Proof Workbench** — mathematical proof workbench running on desktop and WebAssembly.

## Qualifications to preserve

Keep every limit honest and beside the claim it qualifies:

- **Maturity:** **Osprey Native is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.** Never call it "alpha" or "experimental".
- **Host coverage:** Common host conformance across all hosts remains active delivery work (plan 0037); some examples (Issue Inbox, Talon Bank) still use app-specific host boundaries.
- **Accessibility mapping:** The GPUI / FerroUI host maps the UI-machine accessibility tree to AccessKit (NSAccessibility, UI Automation, AT-SPI). WinUI, VS Code, and Apple host AccessKit/accessibility-tree mapping and an external MCP agent server remain delivery work; today the accessibility tree is emitted by UI machine documents, not direct view code.
- **Syntax flavors:** Default, ML, Python, and TypeScript flavors exist in the Osprey compiler; **Dart flavor** is planned (coming soon) and **Kotlin flavor** is under consideration.
- **Web target:** Native targets ship no JS engine; the Web target compiles Osprey to WebAssembly and its browser/VS Code webview host uses JavaScript/TypeScript to draw rows into the DOM.
- **Banned wording:** Never use "zero-cost", "alpha", "experimental", "Logic, in flight", or "does everything".
