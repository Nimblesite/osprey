# SwiftUI casting backup — posterity only

**This backup is only for posterity. It is not a real example, a supported implementation, or a starting point for new work.**

The separate SwiftUI casting renderer was an architectural mistake. The active casting app must use the same `hosts/gpui` rendering code on desktop, iOS and Android. Do not restore this implementation into an active app or add it to build targets, tests or example discovery.

`source.tar.gz` preserves the former iOS casting app, its SwiftUI host sources, bundled skins, machine document and iOS screenshot references. It excludes generated build products. The archive is retained as historical source data, outside the active examples; scripts inside it are historical records and are not maintained commands.

Snapshot commit: `1ac118af3c8e4c8614d3221b913baf56b663522e`.

SHA-256 of `source.tar.gz`: `8450898a3206dcdcfd437d1adf81f1b01e3a235dfa9a123700288f743a187c95`.

The shared Swift host under `hosts/swift` remains available for applications that intentionally use SwiftUI, including Osprey Agent. It is not the casting renderer going forward. Track the replacement in [the shared GPUI mobile plan](../../docs/plans/0053-casting-gpui-mobile.md).
