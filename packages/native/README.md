# @osprey/native

Osprey Native is an application and UI framework for native hosts. An application keeps all of its state in one Osprey store, reads it through Osprey's built-in signals, describes its screens with typed `Node` components, binds controls to typed action values and requests platform work as typed, bounded capabilities. A host renders the framework's keyed regions with its own native controls, returns events by framework-issued IDs and completes capabilities. Everything that crosses the boundary is produced by the framework through Osprey's built-in `Json`; an application never writes protocol text, kind strings or style names.

The first host is the SwiftUI backend of [Vigil](https://github.com/Nimblesite/vigil), a macOS and iOS security product whose screens, state, workflows and policy are Osprey composed with this package. The reference model is Osprey's [macOS storage example](../../examples/macos): Osprey compiled to a static library with `--target=macos` (and `ios`/`ios-sim`), a `start`/`dispatch` C boundary, an opaque model returned by the host and a native renderer. That sample proves the boundary; Osprey Native replaces its string-built node calls with typed components, a centralized store, signal-driven regions and typed capabilities.

```mermaid
flowchart LR
    E[Host event] --> D[Runtime::dispatch]
    D --> U["update: Context, Store, Message -> Step"]
    U --> S[Centralized store and signals]
    S --> R["Keyed::region: regions whose dependencies changed"]
    R --> P[Typed nodes encoded as keyed patches]
    U --> C[Typed capability requests]
    P --> H[Host region store and renderer]
    C --> A[Host capability adapters]
    A -->|completion events| D
    H -->|events with framework IDs| D
```

## Layout

| Path | Contents |
| --- | --- |
| `src/base.ospml` | `Pick::either` and `Lists`: a stable comparator sort, slices, flattening, distinctness, positions and `find`, `any`, `all` over Osprey's built-in list functions |
| `src/doc.ospml` | `Doc`: total readers and member builders over the built-in `Json`, for the framework's own trusted state |
| `src/node.ospml` | `Node<A>`, `Tab<A>`, `Row<A>`, `Binding<A>`, the tone, role, side and axis unions, `Splitters` and the `Ui` constructors ([UI-COMPONENTS]) |
| `src/encode.ospml` | `Encode::walk`: one depth-first walk that encodes a region and collects its bindings in ordinal order ([UI-PROTOCOL]) |
| `src/message.ospml` | `Message<A>`, `Events::bind` and `Messages::route`: typed delivery of taps, text, flags, choices, sizes, completions and lifecycle ([UI-EVENTS]) |
| `src/capability.ospml` | `Capability`, `Request`, `SliceWrite` and `Requests` ([UI-EFFECTS]) |
| `src/reactive.ospml` | The `Slices` effect for host-held slices, `Keyed::region` and `Signals::written` ([UI-SIGNALS], [UI-REGIONS]) |
| `src/theme.ospml` | `Theme`, `Token`, `Contrast` (WCAG 2 luminance and ratio) and `Themes` ([UI-THEME]) |
| `src/runtime.ospml` | `Runtime::start` and `Runtime::dispatch`: the session, the envelope and the serialized update ([UI-STORE], [UI-PROTOCOL]) |
| `tests/` | The package's suite: a two-slice probe application driven through the real `start`/`dispatch` path, plus encoding, ordinal, splitter, request, document, theme and list cases |

Every export carries a documentation comment, and the analyzer (`osprey packages/native/tests --check`) reports none missing.

## Centralized state [UI-STORE]

All application state is one Osprey store value per session, owned by the framework dispatcher. Components, screens and capability adapters never own application state; only native control state that Osprey cannot observe, such as an in-progress IME composition or a scroll offset, stays in the host.

The store is a record of **slices**, each with its own record type, because a slice's type is its signal identity ([UI-SIGNALS]). One serialized dispatcher applies `update : (Context, S, Message<A>) -> Step<S>`, where a `Step` is the next store, the capability requests to run and the host-held slices written ([UI-EFFECTS]). `Message<A>` is a framework union: a resolved UI action, a capability completion or a lifecycle phase. `Context` carries the host clock, UTC offset, platform (`macos` or `ios`), home folder and launch arguments. The framework, not application code, decodes events, rejects stale or malformed input, runs `update`, refreshes regions and encodes the envelope. Updates are pure, so tests drive the real application with fixture messages.

Small slices round-trip through the host as part of the opaque state string. Large slices are **host-held**: the host keeps their bytes by key, and the application fetches them through the `Slices.fetch` effect, answered by a host import in the native build and by fixtures in tests, before its static handler block; a write pending in the same update wins over the host's copy. This split exists because the Apple target's default allocator never frees ([Memory boundary](#memory-boundary)); decoding every slice on every keystroke would leak without bound.

## Signals and reactivity [UI-SIGNALS]

Reactivity uses Osprey's built-in signals ([reactive signals guide](https://www.ospreylang.dev/docs/reactive-signals/)), the way React or Flutter applications use signals: views read signals, and the runtime rebuilds only what depends on a change. Nobody maintains a dependency array, version counter or dirty flag; the compiler derives each view's dependencies.

- **Declaring a signal.** Slices are the built-in `Signal<T>` with `read : Unit => T`; neither the framework nor the application declares its own. A slice reader is `query () = (perform Signal<Query>.read ()).text`. Signal identity is the resolved instance, so `Signal<Query>` and `Signal<Selection>` are independent, and two slices must never share a payload type.
- **Supplying values.** The application has one `handle static Signal<…>` block per slice, supplying values from the centralized store or, for host-held slices, from `Slices.fetch`.
- **Regions.** A region is a keyed view function returning `Node<A>`. Each region is one `Keyed::region (frames, key, changed, view)` call inside that handler block, the built-in `signalRefreshKeyed` with the framework's placeholder node. Its compiler-derived may-read set is the snapshot's `snapshotDependencies`. Between dispatches the framework persists only each region's built-in `SignalFrame` (key, dependencies, revision) through `signalFramesJson`; the rendered value is not kept. A region whose revision did not change is not sent to the host.
- **Writes.** `update` returns the new store and the slices it wrote. The render receives the previous and next store: for each slice it asks `signalDependencies` for the slice reader's identities and supplies them as changed when the slice's encoding differs (the built-in `signalChanges`) or the host-held slice was written (`Signals::written`). Writes happen only through `update`.
- **Conservative by design.** Both arms of a runtime choice count as possible reads. An unresolved callback contributes `<unknown>` and forces a refresh, which is safer than ever retaining a stale view.

Constraints on the current compiler, each with its upstream issue:

- **Handler scope ([#274](https://github.com/Nimblesite/osprey/issues/274)).** `handle static` must lexically enclose the region refreshes; it does not discharge a callback passed in as a parameter. So the application, not the framework, writes the one signal-providing block.
- **[#272](https://github.com/Nimblesite/osprey/issues/272).** Passing a raw `signalRefresh` result straight into a call inside the handler block miscompiles. `signalRefreshKeyed` binds it internally, and regions are still bound to names before they are listed.
- **[#281](https://github.com/Nimblesite/osprey/issues/281)** (fixed in 19cf04c0). An export that reached `signalDependencies` or `signalSnapshot` vanished from Apple libraries; identities now come from `signalDependencies` at each slice reader.
- **[#283](https://github.com/Nimblesite/osprey/issues/283)** (fixed in 3de9639d and 7aa2f553). A closure passed to a generic function from a nested `match` arm reached codegen still generic, as did a lambda whose type only a later use pinned.
- **[#284](https://github.com/Nimblesite/osprey/issues/284)** (fixed in 5f9eccf8). A view passed through a helper keeps its dependencies, so the shared `Keyed::region` helper refreshes correctly (test "a tap refreshes only the region whose signal changed"). [#286](https://github.com/Nimblesite/osprey/issues/286) will derive the changed identities from the handler values, retiring the per-slice `signalChanges` calls.
- **[#277](https://github.com/Nimblesite/osprey/issues/277)** and **[#278](https://github.com/Nimblesite/osprey/issues/278).** Generic unification depends on source-root order (list this package's root first; its base module sorts first), and the mutually recursive encoder is instantiated at one action type, so a program has one action union. Group bindings in it by the value they deliver, each group a union per feature, so every handler matches exhaustively; the binding-kind check keeps a tap from arriving as text or a flag.
- **Library export selection** (fixed in d5b234df). Every function with a scalar C signature anywhere in the program became an Apple library export, so an inlined helper failed with "codegen emitted no definition" and an effectful one with "unhandled effect operations at library export".
- **[#282](https://github.com/Nimblesite/osprey/issues/282).** A constructor from another namespace cannot appear in a pattern, so applications take `Message` apart through `Messages::route`.
- **[#276](https://github.com/Nimblesite/osprey/issues/276)**, **[#279](https://github.com/Nimblesite/osprey/issues/279)** and **[#280](https://github.com/Nimblesite/osprey/issues/280)** (#279 and #280 fixed after 19cf04c0). Lists are transformed with the built-in `mapList`, `filterList`, `foldList` and `forEachList`; `Lists` keeps only what has no built-in yet.

## Regions, patches and identity [UI-REGIONS]

The application lists its regions in one function that holds the `handle static` block, one `Keyed::region` per region key. The root region is always present. A region embeds another with a `Slot key` node, so the tree is a set of keyed regions rather than one serialized document. A key that disappears produces a removal, and its pending events become stale.

Every stateful or reorderable node has a stable semantic key, unique within its region. Identity is never a row index or visible label. Reconciliation in the host preserves editing, selection, expanded groups, scroll anchors and focus for surviving keys.

## Typed components [UI-COMPONENTS]

`Node<A>` is a generic union of semantic components; `A` is the application's action union. Application code builds trees with the `Ui` constructors and exhaustive matching. The framework's encoder is the only code that turns a node into protocol text, and a host rejects unknown kinds and malformed fields instead of drawing a fallback.

| Group | Components |
| --- | --- |
| Structure | `Split` (navigation columns), `Tabs`, `Screen` (title, subtitle, toolbar), `Section` (title, footer), `Aside` (a pane with a column on its leading or trailing side, divided by a splitter whose width lives in the store, [UI-COMPONENTS-SPLITTER]), `Stack`, `Layer` (children drawn over one another), `Slot`, `Spacer`, `Divider` |
| Content | `Text` with a semantic text role and tone, `Label` (text and symbol), `Icon` (a large decorative symbol), `Link` (a labelled `https` page the host opens in the browser and refuses any other address), `Evidence` (label/value line), `Badge` (label, symbol, tone), `Segments` strip, `Empty` state, `Progress`, `Brand`, `Swatch` |
| Collections | `Rows` with stable row keys, selection and per-row content; a row may lead with a symbol and name a group, and consecutive rows of one group share its heading |
| Controls | `Button` with role and enabled flag, `Field` (secure or multiline, committing on edit or submit), `Toggle`, `Picker`, `Disclosure` (title and revealed children; the expansion is a flag binding taken before the children's ordinals) |
| Presentation | `Alert` bound to a typed dismissal action |

Components are semantic, so a host presents each one where it is placed:

- A `Split` is the platform's navigation split view, and its first pane is the sidebar on the theme's `inset` colour. There a `Screen` stacks its children, and `Rows` are the native source list grouped under their row groups' headings. Three panes are sidebar, content and detail.
- Every other `Screen` is a page on the `canvas` colour. Its toolbar buttons join the window toolbar and a toolbar `Field` is the platform search field. A page with a child that is `Rows`, an `Empty` state, a `Slot` or an `Aside` fits the window while its other children leave that child at least 280 points: the child fills the remaining height, and each column of an aside scrolls on its own. Otherwise the whole page scrolls and an aside inside it keeps its natural height. No content, however long, makes a page taller than its window.
- Outside the sidebar, `Rows` are cards: full-row buttons whose selection shows as an inset tint and an accent outline, never colour alone. A `Section` is a bordered surface headed by its title as an eyebrow, with its footer as explanatory text.
- A presented `Alert` keeps its text until it closes, because the platform cannot rewrite one in place; newer text waits for the next presentation, and closing sends one dismissal.

Expansion, like every other control value, lives in the store; the host keeps only the text being composed and the width of a splitter being dragged. Text roles (`display`, `large`, `title`, `heading`, `lead`, `headline`, `body`, `callout`, `caption`, `eyebrow`, `mono`) and tones (`primary`, `secondary`, `accent`, `good`, `caution`, `danger`) are typed unions resolved through the theme ([UI-THEME]). Badges always carry text and a symbol; colour is never the only signal.

### Splitters [UI-COMPONENTS-SPLITTER]

An `Aside` carries its column's width in points and a size binding, so the width is application state like any other control value: `Ui::aside (pane, side, column, width, resize)`. `Splitters` owns the limits. A column starts 340 points wide (`Splitters::initial`) and stays between 260 (`least`) and 640 (`most`) points, and the host never narrows the pane beside it below 340 (`keep`). The encoder sends the width and all three limits, and the splitter's binding takes its ordinal after the controls of both columns, so adding a splitter leaves their ordinals unchanged.

During a drag the host previews the width within the limits Osprey sent and the room the window leaves. When the drag ends it sends the width once, as a whole number of points, and Osprey's answer replaces the preview. A size binding accepts only an integer, and `Events::bind` clamps it to the limits before the application's `update` sees it, so an application never stores a width outside them. On a phone an aside stacks its column under its pane, so there is nothing to drag.

## Bindings and events [UI-EVENTS]

A control's binding is a **typed action value**, not a callback or an event dictionary. `Button` carries an `A`; `Field`, `Toggle`, `Picker`, `Tabs` and `Rows` selection carry an `A` that names the binding, and the framework delivers `Entered`, `Switched`, `Chose` or `Act` messages with the bound action and a value of the matching type. An `Aside`'s splitter delivers `Resized` with a width in points. A text binding can only produce a text message, a toggle only a boolean and a splitter only an integer.

When encoding a region, the framework assigns each binding an opaque ID: region key, region revision and depth-first ordinal. The host returns only that ID and a bounded value. To resolve an event, the framework renders the named region from the current store, checks the revision, finds the binding by ordinal, checks the value kind and produces the typed message. Events for removed regions, stale revisions, unknown ordinals or mismatched value kinds are rejected with a visible diagnostic. The host cannot construct an arbitrary action, and a binding ID is never an authorization token.

**Closure blocker [UI-CLOSURE-BLOCKER] ([#275](https://github.com/Nimblesite/osprey/issues/275)).** The natural binding form, a closure such as `edit : string -> A` stored in a node, is not usable on the Apple C-ABI target today: a closure projected out of the recursive `Node` union is rejected with `library export ... invokes a dynamic callable whose effect provenance cannot be proven`, and record field function types cannot declare a closed effect row. The data-action binding above is the supported design until Osprey can prove effect provenance through recursive aggregates or accept rows on field types.

**Field names ([#273](https://github.com/Nimblesite/osprey/issues/273)).** Codegen rejects a field access when two records share the field name and it loses track of the receiver; `--check` does not report it. Record field names are unique across this package, and an application keeps its own unique against them.

**Documented exports ([#289](https://github.com/Nimblesite/osprey/issues/289)).** Every export and type carries a `(** ... *)` documentation comment; the `undocumented-export` warning shipped in b3bdd6dd.

## Effects and capabilities [UI-EFFECTS]

Updates request platform work by returning `Request` values in their `Step`: a `Capability` with an application-chosen ticket. The runtime encodes them into the envelope; the host completes each with a `completed` event carrying the ticket, and the application ignores completions whose ticket no longer matches, so stale, cancelled and superseded work never overwrites newer state. Bulk results land in host-held slices named by the request, so large outputs never travel back through an event.

`Capability` is a typed union whose variants are mechanical primitives with explicit bounds chosen by Osprey: `Run` (shell-free executable, arguments, deadline, output limit, optional property-list conversion), `ListDirectories`, `ReadPropertyLists`, `ReadTexts`, `Fingerprint`, `StoreRead`, `StoreWrite`, `StoreList`, `StoreDelete` (app-private files), `Http` (method, URL, headers, body, response limit, deadline), `RunningApplications`, `ChooseFile`, `ChooseFolder`, `SaveFile`, `Reveal`, `OpenSettings` and `Cancel`. Adapters return typed results or typed errors; they make no application decision, choose no paths beyond what Osprey named and never retry automatically. Rendering never starts work: only an explicit action, lifecycle message or completion produces requests.

## Theme and motion [UI-THEME]

Semantic colour tokens, spacing, shapes and motion durations are Osprey values. The application's theme is sent once with the first envelope; the host resolves roles against the current appearance and never chooses raw colours itself. Contrast policy is computed and tested in Osprey (`Contrast::ratio`, WCAG 2 relative luminance). The system Reduce Motion preference always wins in the host.

## Protocol [UI-PROTOCOL]

An application exports `Runtime::start (context, init, render, save, theme)` and `Runtime::dispatch (state, event, update, render, save, restore)` under names of its choosing (Vigil: `osprey_app_start` and `osprey_app_dispatch`, importing `vigil_slice` for `Slices.fetch`). The start context is `{"now","offset","platform","home","arguments"}`. Every result is an envelope:

| Field | Meaning |
| --- | --- |
| `version` | Protocol version; the host rejects others |
| `state` | Opaque small slices plus each region's `{ key, dependencies, revision }`; returned unchanged with the next event |
| `slices` | New bytes for host-held slices written by this update |
| `patches` | Keyed regions to replace, each a typed node tree |
| `removed` | Region keys to dispose |
| `requests` | Ordered capability requests with tickets |
| `diagnostics` | Rejected events, arithmetic faults and protocol errors, to show and log rather than hide |
| `theme` | First envelope only |

Events are `{"kind":"ui","region","revision","ordinal","value","now"}`, `{"kind":"completed","ticket","ok","result","error","now"}` and `{"kind":"lifecycle","phase","now"}`. A host bounds string length, tree depth and node count before decoding.

## Hosts

A host initializes the library once, calls it only from its UI thread, copies every returned string immediately, keeps the slices and the region store, renders nodes with native controls, maps tokens to colours and runs capability adapters off the UI thread before delivering completions back through the dispatcher. It must not filter, sort, label, validate, decide visibility or compose application screens. Vigil's `Sources/OspreyNative` is the SwiftUI, AppKit and UIKit host; Android can supply a Kotlin host over the same protocol.

## Memory boundary

The Apple targets support only `--memory=default`, whose general allocations live for the process lifetime, and Osprey has no host-facing string release API. Every dispatch therefore leaks the strings it allocates, and a string returned for `Slices.fetch` must remain valid for the process lifetime. Signals bound the leak to what changed: keystrokes refresh small regions and never decode unrelated host-held slices, and large slices are re-encoded only when written. This is a measured mitigation, not bounded memory.

## Using the package

Osprey's package client is not released, so a consumer lists the package's source root today, first ([#277](https://github.com/Nimblesite/osprey/issues/277)):

```toml
[project]
name = "app"
source_roots = ["../.osprey/packages/@osprey/native/src", "src"]
```

Once the registry serves it, the same consumer declares `"@osprey/native" = { epoch = 1, min_release = 1 }` under `[dependencies]` and the client installs it under `.osprey/packages`. The package itself has no entry; `osprey packages/native/tests --check` analyses it, `osprey packages/native/tests --run` runs its suite and `osprey test packages/native/tests --coverage` measures it: every source file is fully covered, and the four lines the report leaves out in `tests/runtime.ospml` are the probe's functions handed to `Runtime::start`, which the coverage counter misses on their first line ([#296](https://github.com/Nimblesite/osprey/issues/296)).
