# Osprey Storage: where your disk space goes, on the Mac and iPhone

One SwiftUI app that shows what uses your disk, folder by folder. On the Mac it opens on the whole disk with hidden files shown, so apps, the system, your home folder and Documents all count; it ranks everything inside by size and lets you click into any folder. The eye button in the toolbar (⇧⌘.) leaves hidden files out of every total; ⌘↑ goes to the enclosing folder, ⌘R measures again and ⌘O measures another folder or drive. On the Mac, right-click a folder and choose Open in Finder to see it in a Finder window. On iPhone it shows what uses the phone as far as iOS lets an app see: the photo library, once you allow it, this app, and everything else.

Osprey owns the app. [`storage/src`](storage/src) holds the browser's state and events, the ranking, the hidden-file rule, the symbols, the formatting and every screen, which Osprey returns as a tree of native nodes. Swift does two things: it draws that tree with the platform's own grouped list, toolbar, toggle and capacity bar, and it measures the disk.

## Build and run

Use an Apple silicon Mac with Xcode selected by `xcode-select`. From the repository root:

```sh
make macos                          # Mac runtime, compiler and every macOS example
examples/macos/run.sh               # build and open the Mac app
examples/macos/run.sh ios-sim       # the same app in the iPhone simulator
make macos-test                     # C ABI, corpus, domain suite and smoke
```

`run.sh --smoke` builds a folder with known contents, including a hidden `.git` inside a visible folder, a hidden `.cache` and an unreadable folder, then drives Osprey through the events the app sends: it requires the walk to count them, the screen to show and explain them, the toggle to leave the hidden ones out, and a folder to open. `run.sh ios-sim-smoke` runs the same check in the simulator. `OSPREY_MACOS_SKIP_BUILD=1` reuses the existing Mac build, and `OSPREY_BIN` selects another compiler.

## One app, two platforms

The iPhone apps and this one build the same way: [`examples/ios/build.sh`](../ios/build.sh) compiles the Osprey project into `libOspreyApp.a` and its generated `libOspreyApp.h`, and `xcodebuild` links them into the app. [`OspreyStorage.xcconfig`](OspreyStorage.xcconfig) declares one target for `macosx`, `iphoneos` and `iphonesimulator` and picks the matching archive (`--target=macos`, `ios` or `ios-sim`). The Swift sources, the renderer and the Osprey code are the same on both. See [the Mac slice](../../docs/specs/compiler/iOSCodegen.md#the-mac-slice-ios-target-macos) for the boundary.

The renderer is the one [Issue Inbox](../mobile/ios/IssueInbox/NativeRenderer.swift) uses. This app adds node kinds that map straight to stock controls: `navigation` (a titled screen of grouped sections with a toolbar), `entry`, `gauge`, `stack`, `pathbar`, `toggle`, `progress` and `empty`. Any node can carry a `menu` of buttons, which the renderer shows as its context menu; Osprey gives each folder an Open in Finder item on the Mac, and its `reveal` command opens the folder in Finder. Osprey chooses every word, symbol, colour, share and key equivalent.

On iPhone, home is the app's own container, and iOS lets no app read another app's files. The first screen separates Photos & Videos, Osprey Storage itself, and "Other apps & iOS", the used space less those two. PhotoKit reports the bytes of photo and video resources stored on the device, including hidden assets; originals held only in iCloud appear separately. Limited Photos permission counts only the assets you allow. Settings › General › iPhone Storage splits the remainder by app. Choose Folder opens the Files picker for iCloud Drive and On My iPhone folders; the app holds the chosen folder's security scope until the next pick.

## The boundary

The [store](OspreyStorage/StorageStore.swift) calls `osprey_main()` to initialize the runtime, then uses two browser entry points from [`storage/src/main.ospml`](storage/src/main.ospml):

```ospml
start (home, platform, full) = envelope (Browse::start home platform full)
dispatch (model, event) = envelope (Browse::dispatch (Browse::decode model) event)
```

`platform` is `"macos"` or `"ios"`; `full` reports Full Disk Access on the Mac and is `false` on iPhone. The entry source also exports `formatBytes`, `formatCount` and `percent` for host calls.

Each returns the browser as opaque text, the screen, and any host work: `measure` a folder (with the folders not to enter, and whether to add it to the walk already made), `pick` one, `reveal` one in Finder, measure the `photos` library, or open the privacy `settings`. Swift does that work and reports back as events (`progress`, `measured`, `failed`, `picked`, `photos`), and Osprey ignores a report about a folder it has since left.

The screen asks the platform through an effect, so [`View`](storage/src/view.ospml) never touches the file system and the [domain suite](storage/test/main.ospml) renders it from fixed data:

```ospml
export effect Facts
    volume : string => Volume
    listing : string => Listing
    photos : Unit => Photos
```

`main.ospml` answers it with C functions defined with `@_cdecl` in [`StorageServices.swift`](OspreyStorage/StorageServices.swift) and [`PhotoLibrary.swift`](OspreyStorage/PhotoLibrary.swift). All calls are synchronous on the main thread. Swift strings passed to Osprey are borrowed for the call. A string a Swift import returns must stay valid for the rest of the process, so each listing's paths are copied once per walk and never freed. Calling Swift crosses Osprey's memory-safety boundary.

Ranking loops over a folder's entries and keeps only the 120 largest, so a folder of any width keeps the stack flat. The iPhone main thread has a 1 MB stack, and Osprey does not turn tail calls into loops.

## Where the numbers come from

- **Volume capacity:** Foundation's `volumeTotalCapacity`, `volumeAvailableCapacity` and `volumeAvailableCapacityForImportantUsage`. The last includes space the system can purge on demand, such as caches and local snapshots. Settings counts that space as available, and so does the app: used is the total less it, so the gauge matches the Storage pane.
- **Folder sizes:** one walk of the whole disk on the Mac, or of the chosen folder, with Foundation's directory enumerator. Each folder records the allocated size of everything below it, and the size of everything below it outside hidden items, so the hidden-files toggle needs no second walk. Hidden files, caches and package contents count. A hard-linked file counts once, a mounted volume below the folder is not entered, and APFS clones count in full, as in Finder. An iCloud Drive or File Provider folder whose contents were evicted is not entered either: it holds nothing on this disk, and listing it would make the provider fetch it from the network. "Hidden" means what Finder hides: names starting with a dot and items flagged hidden, such as `~/Library`. On the Mac the walk starts at `/`: the system's sealed volume, and the data volume through its firmlinks (`/Applications`, `/Users`, `/Library`, `/private`), with `/System/Volumes/Data` itself skipped as a mount so nothing counts twice.

Folder totals update during the background walk, with live file counts and measured bytes.

## Permissions

macOS asks the person once before any app reads Desktop, Documents, Downloads or iCloud Drive. The walk enters them, so those questions appear during the first walk; it waits for each answer, and a folder you refuse is counted as unreadable. The Info.plist says why each is read.

macOS also asks before an app reads another app's data, such as the containers under `~/Library/Containers`, and Mail, Messages and Safari are closed to every app without Full Disk Access. Without it, Osprey sends the container folders with every walk and the walk does not enter them: they appear under "Other apps' data", where choosing one opens the system folder picker already on that folder, which grants it without a prompt, and its walk is added into the totals already measured. The first screen offers Allow Full Disk Access, which opens that list in System Settings; turn on Osprey Storage, quit and reopen it, and the walk enters everything the system lets any app read. The app checks for that access by opening the privacy database, which never prompts. What still cannot be read, such as another user's folders, is counted in the "could not be read" footer instead of silently shrinking a total.

On iPhone the app asks for the photo library once, at its first walk; Allow in Settings opens the app's own settings page if you refused.
