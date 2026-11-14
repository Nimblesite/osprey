# SwiftUI host

A macOS host for [Osprey Native](../../../osprey/packages/native/README.md) applications ([NATIVE-HOSTS-FAMILY](../../../../docs/specs/applications/0047-OspreyNativeHosts.md)). It links an application's C ABI archive, leases the frame it writes into shared memory, checks every row and draws it in place — no JSON, no parse, no copy, and no colour, font or padding of its own. [Shared-memory views](../../../../docs/specs/applications/0053-NativeSharedViews.md) explains the frame end to end. It holds no application logic.

```sh
src/toolchain/hosts/swift/view.sh                                   # Tally, in a window
src/toolchain/hosts/swift/view.sh --view-check                       # the same journey, headless
OSPREY_SWIFT_APP=$PWD/src/osprey/examples/agent OSPREY_SWIFT_NAME="Osprey Agent" \
OSPREY_SWIFT_BUNDLE_ID=org.ospreylang.agent src/toolchain/hosts/swift/run.sh -- /path/to/project
```

`view.sh` compiles the application with `--target=macos --memory=arc`, builds the Swift package and runs `OspreyViewHost` directly. `run.sh` compiles the same archive, links the same host and assembles an ad-hoc signed development bundle, which it opens with the arguments after `--`; those arguments reach the application, in order, through `osprey_app_argument` before it opens. An application's `icon.png`, 1024 pixels square, becomes the bundle's icon. `--build` stops after assembling the bundle and prints its path.

## iOS

The same one library hosts iOS — no second package, no copied files:

```sh
src/toolchain/hosts/swift/ios.sh                                                       # Tally, built for the simulator
src/toolchain/hosts/swift/ios.sh ios-sim --view-check                                 # the same journey, headless
OSPREY_SWIFT_APP=$PWD/src/osprey/examples/storage OSPREY_IOS_NAME=Storage \
  OSPREY_IOS_ID=org.ospreylang.storage src/toolchain/hosts/swift/ios.sh ios-sim        # another application
```

`ios.sh` compiles the application for `ios-sim` (or `ios`, unsigned) against the ARC runtime, links the same `OspreyShared` library through the `OspreyViewHostIOS` entry and assembles `<Name>.app`. What differs on iOS: the launch context carries platform `ios` and the app's Documents folder as home; process execution, folder choosing, saving, property-list reads, fingerprints and text recognition are compiled out — the frame's asks for them complete with the same typed error an unknown capability gets; `choose-file` opens the system document picker instead of a panel; and `--view-check` boots the simulator, runs the journey with no person present and prints `OSPREY_IOS_VIEW_OK`, writing the verdict to the app's `Documents/view-check.txt` too.

## The boundary the application exports

Every application exports exactly three calls beside `osprey_main` ([NATIVE-SHM-EVENT]):

```c
int64 osprey_app_argument(const char *text);                       // one launch argument, in order, before open
int64 osprey_app_open(int64 now, int64 offset, const char *platform, const char *home, bool dark);
int64 osprey_app_event(int64 now, int64 kind, int64 ordinal, const char *words, const char *detail,
                       double x, double y, bool down, bool shift);
```

Every call — including a refused event, which answers 1 — writes a whole frame, and the host always commits it (`osp_native_frame_commit`) and shows it. A frame is one branch chunk with four sealed leaf runs, which `SharedBridge` leases and `SharedRows` checks row by row exactly as the [GPUI host's `rows.rs`](../gpui/src/rows.rs) does: every kind, code, parent, count, colour, extent and text range must be known and in bounds, every float finite, every text UTF-8. A frame that fails any check is refused whole and the last good view stays, with the reason on the host's diagnostic strip.

| Run | Rows | Drawn by |
| --- | --- | --- |
| Scene rows (`OspNativeShape`, 88 bytes) | A canvas's scene: rects, discs, strips, sprites, glyphs, lettering, paths, atlas entries, accessible elements | `SharedScene`/`SharedScenePaint`, ported from the iOS host's painting |
| View rows (`OspNativeView`, 152 bytes) | 17 control kinds, from `column` to `sheet`, each with its look, fit, flags, words and hint | `SharedView`/`SharedLeaf`, through `SharedLook` |
| Text | The UTF-8 bytes every row's offsets name | Read in place |
| Ask rows (`OspNativeAsk`, 32 bytes) | Capability requests (code 1–25), slice writes (100) and diagnostics (101) | `SharedStore` |

A control's hint names its icon (a label's or press's), its prompt (a field's) or its key (an option's). The host sends typed events back: pointer, key and view measurements over a canvas, `control` values from controls, `appearance` when dark mode changes, `frame` from a display link an `animate` ask turns on, and `phase "active"` after open.

## Asks

A field-0 ask row starts one: its number is the capability's code (its position in `Capability` in [`capability.ospml`](../../../osprey/packages/native/src/capability.ospml), 1–25) and its text the ticket; the rows after it are that capability's arguments by position. Code 100 is a slice write (its text the key), which the host stores for the application's `native_slice` import; code 101 is a diagnostic — why the application refused an event — shown on the host's diagnostic strip.

Requests go to the capability adapters in `OspreyShared`, which take the typed `Ask`:

| Answered | Adapter |
| --- | --- |
| `run`, `process-start`, `process-write`, `process-read`, `process-stop` | `ProcessAdapter.swift` — one-shot and long-lived child processes, bounded and cancellable |
| `http` | `NetworkAdapter.swift` — HTTPS or loopback HTTP, no redirects, streamed limit |
| `store-read`, `store-write`, `store-list`, `store-delete` | `StoreAdapter.swift` — the app-private store, pinned descriptor traversal |
| `choose-file`, `choose-folder`, `save-file` | `DocumentAdapter.swift` — the system panels, completing with the bare path (`""` when cancelled) |
| `measure-folders` | `MeasureAdapter.swift` — folders walked to the bottom on their own volume: allocated bytes, hidden items, links never followed, volumes [UI-EFFECTS-MEASURE] |
| `animate` | `SharedStore` itself — the display-link frame clock |
| `cancel` | `CapabilityRunner` — cancels the ticket's in-flight request |

Every other completion's text is the same result document the adapter produced before, as text. Any other capability completes with an error naming it, on the diagnostic strip.

## Checking it

`view.sh --view-check` drives Tally's journey with no window: three taps, a rename, a restyle and a refused event, with every fact read back out of the checked frame — controls, words, colours, ask rows, scene rows and the refused event's committed frame. It prints `PASSED`, or `FAILED: <why>` and exits 1. `OSPREY_SWIFT_APP` names another application folder. `swift test` runs the adapter suite (`Tests/OspreySharedTests`), which needs the archive `view.sh` builds.
