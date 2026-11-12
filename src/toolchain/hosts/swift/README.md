# SwiftUI host

A macOS host for [Osprey Native](../../../osprey/packages/native/README.md) applications ([NATIVE-HOSTS-FAMILY](../../../../docs/specs/applications/0047-OspreyNativeHosts.md)). It links an application's C ABI archive, keeps its regions, draws its nodes with SwiftUI and answers its capabilities. It holds no application logic.

```sh
OSPREY_SWIFT_APP=$PWD/src/osprey/examples/agent OSPREY_SWIFT_NAME="Osprey Agent" src/toolchain/hosts/swift/run.sh -- /path/to/project
```

`run.sh` compiles the application with `--target=macos`, builds the Swift package and assembles an ad-hoc signed development bundle. The next launch argument onwards reach the application as `Context.arguments`.

The C ABI loader, slice store, store and HTTP adapters come from [Vigil](https://github.com/Nimblesite/vigil)'s `OspreyNative` library. The renderer here reads each node as the JSON the framework encoded, so it follows the framework's current vocabulary, including frames, rich text and themes.

## What it draws

| Node kinds | Mapping |
| --- | --- |
| `text`, `rich-text`, `label`, `icon`, `badge`, `pair`, `evidence`, `link`, `spacer`, `divider`, `progress`, `meter`, `empty` | Selectable text and native views; symbols are SF Symbols |
| `button`, `field`, `toggle`, `picker`, `disclosure`, `alert`, `rows` | Native controls; rows in a sidebar are the system source list |
| `stack`, `framed`, `themed`, `slot`, `layer`, `section`, `screen`, `split`, `aside`, `tabs`, `dialog`, `context-menu` | SwiftUI layout; a split's first pane stands on the system's sidebar material |
| `math`, `image`, `video`, `editor`, `board`, `scene`, `segments`, `swatch`, `brand` | A visible refusal, never a fallback |

It answers `run`, `http`, `store-read`, `store-write`, `store-list`, `store-delete`, `choose-folder` and `cancel`. Any other capability completes with an error naming it.

A multiline field that commits on edit delivers Return as a newline at the end of its text, as a multiline field does on every platform. The application decides what that means; Osprey Agent sends.

## An equivalent Windows host

The application needs nothing changed. [`src/toolchain/hosts/winui`](../winui) already speaks the protocol, draws most kinds and answers `run` and the store. To draw Osprey Agent, it needs:

| Gap in `Coverage.cs` | What to add |
| --- | --- |
| `framed` refused | Draw a frame's insets, sizes, surface tokens, radius, edge line and inherited type and control metrics, following [`frame.ts`](../../vscode-extension/client/src/osprey-ui/frame.ts). A scrolling frame with `align` `end` follows new content |
| `rich-text` refused | Inline runs in one `RichTextBlock`, honouring `ink`, `italic` and `emphasis` (`strong`, `code`) |
| `http` unanswered | An adapter over `HttpClient`, with the limits and receipt of [`NetworkAdapter.swift`](Sources/OspreyNative/NetworkAdapter.swift) |
| `choose-folder` | A `FolderPicker`, if it is not already answered |
| A multiline field | Return appends a newline to the delivered text |
| A split's first pane | Mica or Acrylic in place of the sidebar material, tinted by the theme's `sidebar` token |

The new symbols `send`, `terminal`, `edit`, `panel` and `sparkle` are already in `Glyphs.cs`. `Engine.tool_command` already has PowerShell forms of every tool, but none of this has run on Windows yet.

## The shared-memory view host

`view.sh` compiles a native-view application ([NATIVE-SHM-VIEW](../../../../docs/specs/applications/0047-OspreyNativeHosts.md)) into `build/view/libOspreyViewApp.a` and runs `OspreyViewHost`, which draws the rows the application writes into shared memory and reads them in place: no JSON, no parse, no copy, and no colour, font or padding of its own.

```sh
src/toolchain/hosts/swift/view.sh              # Tally, in a window
src/toolchain/hosts/swift/view.sh --self-test  # the same journey, headless
```

`--self-test` taps "Add one" three times, renames the tally to "Zoë" and restyles it, checking each view's texts, colours and counts by reading the rows; it prints `PASSED`, or `FAILED: <why>` and exits 1. `OSPREY_SWIFT_APP` names another application folder.
