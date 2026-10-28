# Osprey Storage

Osprey Storage shows what takes the space in a folder: the volume it sits on, then every child ranked by the bytes it and everything below it allocate. It is written entirely in Osprey with `@osprey/native` ([NATIVE-APPS-ALL](../../docs/plans/0039-examples-on-osprey-native.md)). The store, the rules, the words and the screens are Osprey. A host only walks folders through the generic `MeasureFolders` capability ([UI-EFFECTS-MEASURE](../../packages/native/README.md)) and draws the nodes.

## Run

From the repository root:

```sh
examples/storage/run.sh --host=winui   # Windows: builds storage.dll and opens it in the WinUI host
examples/storage/run.sh                # the GPUI host
examples/storage/run.sh --test         # the suite: no disk, no window, every operating system
```

| Host | State |
| --- | --- |
| WinUI ([hosts/winui](../../hosts/winui)) | Draws every node Storage uses and answers `measure-folders`, `choose-folder`, `reveal` and `cancel`. On CI's Windows runner it passes `self-test.json` on a fixture tree, and `self-test-drive.json` on the whole C drive, in a real window |
| GPUI ([hosts/gpui](../../hosts/gpui)) | Lacks `split`, `meter`, `context-menu` and `measure-folders` until [plan 0039](../../docs/plans/0039-examples-on-osprey-native.md) phase 1 lands, and reports each as a diagnostic |

## How it works

- **Store.** Three small slices travel in the session: `Place` (platform, home, open folder, chosen folders), `Options` (whether hidden items count) and `Walk` (the open folder's ticket and stage). The walk's result is host-held under `measured`, so a folder of 20,000 children never enters the session ([UI-STORE](../../packages/native/README.md)).
- **Walks.** Opening a folder asks the host to measure it under a new ticket and cancels a walk still running. Only the current ticket's completion is accepted, so a stale walk never overwrites a newer one. A walk stops at two minutes or 20,000 children and says its totals are partial.
- **Regions.** `places` (the sidebar), `folder` (the page, its toolbar and the path to it) and `listing` (the volume and the ranked children). A finished walk redraws only the listing.
- **Rules.** [`disk.ospml`](src/disk.ospml) ranks children in one loop and keeps the 120 largest. It sums the rest into one row, lists unreadable folders last and leaves hidden items out of every total when they are switched off. Paths climb to their drive or share on Windows and to `/` elsewhere. Sizes count in powers of 1,024 on Windows, as File Explorer does, and in powers of 1,000 elsewhere. The words follow the platform: Show in File Explorer, Show in Finder or Show in Files.

## Test

```sh
./target/release/osprey test examples/storage/test
```

The suite drives the real `start` and `dispatch` path with fixture walks. It covers the first walk, ranking, the hidden-items switch, links, unreadable folders, a stale walk, Up at a drive's top, a chosen folder and a cancelled choice. It also covers a failed walk, a vanished folder, per-platform words and units, and Windows and Unix paths.
