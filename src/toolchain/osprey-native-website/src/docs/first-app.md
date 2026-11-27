---
layout: layouts/docs.njk
title: "Build Your First App"
lead: "Learn the core Osprey Native loop — typed actions, pure update, declarative view, and headless testing — and run the same app across macOS, Windows, iOS, and Android."
section: "Tutorial"
prevUrl: /docs/
prevTitle: "Overview & Get Started"
nextUrl: /docs/bridge/
nextTitle: "Shared-Memory Bridge"
permalink: /docs/first-app/
---

Every Osprey Native application consists of three clean pieces:

1. **The state** your app remembers (a typed record of slices).
2. **A `view` function** that turns state into semantic controls (`Column`, `Across`, `Wording`, `Press`, `Typing`, `Switch`, `Rows`, `Split`, `Screen`).
3. **A pure `update` function** that takes the current state and a typed action, returning the next state and any capability requests.

## 1. Declare actions and state

```ospml
type Act = Add | Take | Clear | Named | Restyled

type Counted =
    count : int
    owner : string
    night : bool
```

## 2. Write the pure update

```ospml
export update (store, action, words) = match action
    Add => store(count = satAdd store.count 1)
    Take => store(count = max (0, satSub store.count 1))
    Clear => store(count = 0)
    Named => store(owner = words)
    Restyled => store(night = store.night == false)
```

`match` is exhaustive: adding a new action without handling it is a compile-time error. `satAdd` and `satSub` saturate cleanly without panicking.

## 3. Describe the view

```ospml
export view store =
    ui::Column(columnLook = page store.night, columnItems = [
        ui::Wording(wordsLook = title store.night, wordsText = heading store),
        ui::Across(acrossLook = spaced, acrossItems = [
            ui::Press(pressLook = soft store.night, pressWords = "Take one", pressAction = Take),
            ui::Press(pressLook = strong store.night, pressWords = "Add one", pressAction = Add),
            ui::Press(pressLook = soft store.night, pressWords = "Clear", pressAction = Clear),
        ]),
        ui::Typing(typingLook = soft store.night, typingText = store.owner, typingAction = Named),
    ])
```

## 4. Test headlessly and run on every platform

```sh
make -C src/toolchain/targets/tests tally     # headless tests (no window)
make -C src/toolchain/targets/macos tally     # macOS SwiftUI window
make -C src/toolchain/targets/windows tally   # Windows WinUI window
make -C src/toolchain/targets/ios tally       # iOS simulator
make -C src/toolchain/targets/android tally   # Android device/emulator
```

Next: [How the Shared-Memory Bridge Works →](/docs/bridge/)
