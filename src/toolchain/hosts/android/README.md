# The Android view host

This is the one Android host every Osprey Native application runs on: the Android twin of [`src/toolchain/hosts/swift`](../../swift). An application is compiled to a static library that exports `osprey_main`, `osprey_host_abi_version` and the three boundary calls (`osprey_app_argument`, `osprey_app_open`, `osprey_app_event`), linked beside `host/src/main/cpp/bridge.c` into one `libosprey_host.so`, and drawn with the platform's own Android views. There is no Compose and no third-party dependency.

Nothing here names an application: the same host packages any of them. `build.sh` chooses the application, and the package id and label come from the same call.

## The two commands

```
OSPREY_ANDROID_APP=<application folder> [OSPREY_ANDROID_ID=<package id>] \
    [OSPREY_ANDROID_NAME=<label>] bash src/toolchain/hosts/android/build.sh
bash src/toolchain/hosts/android/run.sh [--view-check]     # installs on OSPREY_ANDROID_SERIAL
```

With nothing set, the host builds and opens Tally. `run.sh --view-check` runs the headless check instead: it opens the application with no person present, requires a first frame with at least one row and nothing refused, presses the first actionable control — a plain row's action, or for a component frame the first told control with an `event` ordinal, a button before any other — and requires a second frame, and writes `OSPREY_ANDROID_VIEW_OK rows=<n>` or the failure to the app's `files/view-check.txt` and to logcat under the tag `OspreyHost`. `make -C src/toolchain/targets/android tally` and `make -C src/toolchain/targets/android/tests tally` wrap the two commands.

The host leases each call's shared frame, checks every row before drawing it ([NATIVE-SHM-SAFETY]), keeps the last good frame when one is refused, and shows its own diagnostics under the application. A row keeps its view while its kind stays the same, and a told component node keeps its view across frames the same way (key: region key, position path, kind), so text fields hold their text, caret and focus across redraws. Launch arguments ride the intent extra `osprey_arguments` (a string array), and an application folder's `assets/` folder is copied into the APK as assets.

## Component kinds drawn

Plain view rows ([NATIVE-SHM-VIEW]): column, line, label, press, entry, gap, scroll, layer, toggle, picker with its options, rule, image, canvas (its scene), editor, menu and sheet. Scene rows ([NATIVE-SHM-SCENE]): rects, discs, strips, sprites, glyphs, lettering and paths with fills, strokes, dashes, gradients and SVG arcs.

Told component kinds ([NATIVE-SHM-NODES]) are read, checked and drawn as the SwiftUI host draws them, resolving the frame's `theme` record: the page is the theme's `background` token for the system appearance, text and tones come from the theme's tokens, buttons hug their labels, stacks lead their columns at the frame's gap or the theme's spacing step, symbols are this host's own monochrome vector drawables tinted with the words they sit beside, and the root region is centred as SwiftUI centres one shorter than its window. The kinds: text, rich-text, label, icon, badge, pair, link, spacer, divider, progress, meter, segments, swatch, empty, image, scene, button, field, toggle, picker, disclosure, alert, editor, stack, framed, themed, slot, layer, section, screen, split, aside (with its splitter), tabs, dialog, context-menu and rows.

### Kinds this host does not draw

Math nodes need a mathematics typesetter the platform's own views do not carry, video needs a device-verified media pipeline, and board nodes need the stroke and lasso capture this host has not verified on a screen; each is refused visibly where it would have been, per [NATIVE-HOSTS-CONTRACT]. An option row means nothing outside its picker and draws nothing, as on every host.

## Capabilities

Answered ([NATIVE-SHM-ASK]): store-read, store-write, store-list and store-delete over the app's private store; http over `HttpURLConnection`, off the main thread; read-texts for files this host itself imported; measure-folders ([`MeasureAdapter.kt`](host/src/main/java/org/ospreylang/host/MeasureAdapter.kt)), which walks each named folder's children to the bottom — allocated bytes (`st_blocks * 512`), a hard-linked file once per walk, links listed and never followed, the walk staying on its start volume — within an entry limit and a deadline, writing the result to the named slice [UI-EFFECTS-MEASURE]; choose-file through the Storage Access Framework, completing with the bare path of the imported copy; cancel; animate, which turns the choreographer frame clock on and off; and host-call's `android` namespace — `open-url` hands an https page to the browser, `clipboard` reads or writes the person's clipboard.

Refused with the typed refusal the SwiftUI host gives: every other capability, including run and the process capabilities (no child processes on Android), list-directories, read-plists, fingerprint, choose-folder and save-file (a storage-access tree is not a folder path an application could walk), reveal, open-settings, recognise and host-calls outside the `android` namespace. Each refusal completes its ticket with the reason.
