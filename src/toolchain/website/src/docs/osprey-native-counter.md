---
mlTwins: manual
layout: page
title: "Build a counter with Osprey Native"
description: A first Osprey Native app, step by step. Learn the basic controls, see how each one looks on macOS, iOS and Android, and build a counter that runs on all three.
permalink: /docs/osprey-native/first-app/
tags: [native, applications, ui, getting-started, tutorials]
---

{% from "native-capture.njk" import capture %}

This tutorial builds **Tally**, a counter with three buttons, a name field and a day/night switch. You write it once in Osprey, and it opens as a real native app on macOS, iOS, Android and Windows. Each platform draws it with its own buttons, text and fields.

{{ capture("/assets/images/native/tally-macos-counted.webp", "Tally on macOS after three taps: the title Tally, the count 3 in large blue digits, the buttons Take one, Add one and Clear, and an empty name field", "Tally · macOS · after three taps of Add one.", 900, 239) }}

You need a checkout of the [Osprey repository](https://github.com/Nimblesite/osprey) and a working build ([Install](/docs/installation/)). The finished app is in [`src/osprey/examples/tally`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/examples/tally). Every snippet below is copied from it.

## How an Osprey Native app works

You only write three things:

1. **The data** your app remembers. For Tally, that is the count, a name and whether night colours are on.
2. **A view function** that turns that data into controls such as text, buttons and fields.
3. **An update function** that takes the data and the button a person tapped, and returns the new data.

The platform does the rest. When someone taps **Add one**, the host tells your app which button was tapped. Your app runs `update`, then `view`, and the host redraws the screen with its own native controls. You never write Swift, Kotlin or C# for this.

## The basic controls

Every screen is built from a few controls. Each one becomes the platform's own control. macOS and iOS use SwiftUI. Android uses Android's standard views. Windows uses WinUI.

| Osprey control | What it is | macOS and iOS | Android |
| --- | --- | --- | --- |
| `Column` | Puts its items one below the other | `VStack` | vertical `LinearLayout` |
| `Across` | Puts its items side by side | `HStack` | horizontal `LinearLayout` |
| `Wording` | A line of text | `Text` | `TextView` |
| `Press` | A button that sends an action when tapped | tappable text with your colours | `TextView` that takes taps |
| `Typing` | A one-line text field | `TextField` | `EditText` |
| `Writing` | A text box for several lines | a multi-line editor | `EditText` |
| `Switch` | An on/off switch | `Toggle` | `Switch` |
| `Choosing` and `Option` | Pick one from a few choices | a row of tappable options | a row of tappable options |
| `Gap` | Empty space that pushes things apart | `Spacer` | `Space` |
| `Scrolling` | A column that scrolls | `ScrollView` | `ScrollView` |
| `Layered` | Items stacked on top of each other | `ZStack` | `FrameLayout` |
| `Rule` | A thin dividing line | a hairline | a thin `View` |
| `Picture` | An image | an image view | `ImageView` |

There are also `Menu`, `Sheet`, `Phrase` for rich paragraphs and `Canvas` and `Drawing` for custom graphics. Tally doesn't need any of them.

### Looks: colours, sizes and spacing

Every control takes a **look** (`ui::Look`) that sets how it appears:

| Field | Meaning |
| --- | --- |
| `ink` | Text colour |
| `fill` | Background colour; `0` means none |
| `points` | Text size |
| `inset` | Padding inside the control |
| `gap` | Space between the items in a `Column` or `Across` |
| `corner` | Corner rounding |
| `heft` | Font weight, from 100 (thin) to 900 (black) |

`Views::colour "#0B6E8A"` turns a web-style colour into the number `ink` and `fill` expect. The host has no colours or fonts of its own, so every platform draws exactly the look you wrote.

## Step 1: describe the data

Tally can do five things. List them as one type. Each name is an **action** that a button or field can send:

```ospml
type Act = Add | Take | Clear | Named | Restyled
```

Then describe everything Tally remembers:

```ospml
type Counted =
    count : int
    owner : string
    night : bool
```

Inside `module Screen`, the app starts from this value:

```ospml
    export first = Counted(count = 0, owner = "", night = false)
```

## Step 2: write the update

`update` takes the current data, the action and any typed text. It returns the new data. `store(count = ...)` means "the same store, with this one field changed".

```ospml
    lowered count = match count > 0
        true => satSub count 1
        false => 0

    export update (store, action, words) = match action
        Add => store(count = satAdd store.count 1)
        Take => store(count = lowered store.count)
        Clear => store(count = 0)
        Named => store(owner = words)
        Restyled => store(night = store.night == false)
```

`match` must cover every action, so the compiler tells you if you add an action and forget to handle it. `lowered` keeps the count from going below zero. `satAdd` and `satSub` stop at the largest and smallest integers instead of wrapping around.

## Step 3: draw the screen

`view` turns the data into controls. It is a plain function: give it the same data and it always returns the same screen.

```ospml
    export view store =
        colours = palette store.night
        ui::Column(columnLook = page colours, columnItems = [
            ui::Wording(wordsLook = title colours, wordsText = heading store),
            ui::Wording(wordsLook = figure colours, wordsText = toString store.count),
            ui::Across(acrossLook = spaced, acrossItems = [
                ui::Press(pressLook = soft colours, pressWords = "Take one", pressAction = Take),
                ui::Press(pressLook = strong colours, pressWords = "Add one", pressAction = Add),
                ui::Press(pressLook = soft colours, pressWords = "Clear", pressAction = Clear),
            ]),
            ui::Typing(typingLook = soft colours, typingText = store.owner, typingAction = Named),
            ui::Gap,
            ui::Press(pressLook = soft colours, pressWords = restyle store, pressAction = Restyled),
        ])
```

From top to bottom: a title, the count, a row of three buttons, a name field, a gap and the colour button. Each `Press` names the action it sends. The `Typing` field sends `Named` with whatever the person typed.

The title and the colour button change with the data:

```ospml
    heading store = match store.owner
        "" => "Tally"
        owner => "${owner}'s tally"

    restyle store = match store.night
        true => "Use day colours"
        false => "Use night colours"
```

## Step 4: pick the colours

`palette` picks a set of colours for day or night. Small functions build each look from it:

```ospml
    palette night = match night
        true => Palette(paper = "#10141F", words = "#F2F4FA", accent = "#7AD7F0", onAccent = "#06222B", quiet = "#232A3B")
        false => Palette(paper = "#FBFAF7", words = "#1B2230", accent = "#0B6E8A", onAccent = "#FFFFFF", quiet = "#E8E6E0")

    figure colours = ui::Look(ink = Views::colour colours.accent, fill = 0, points = 72.0, inset = 0.0, gap = 0.0, corner = 0.0, heft = 800)

    strong colours = ui::Look(ink = Views::colour colours.onAccent, fill = Views::colour colours.accent, points = 17.0, inset = 12.0, gap = 0.0, corner = 10.0, heft = 600)
```

`figure` makes the count big and bold in the accent colour. `strong` makes **Add one** a filled, rounded button. The other looks (`page`, `title`, `soft` and `spaced`) work the same way; see [`src/app.ospml`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/tally/src/app.ospml). Tapping **Use night colours** flips `night`, and the whole screen redraws in the night palette:

{{ capture("/assets/images/native/tally-macos-night.webp", "Tally on macOS in night colours: a dark page, the count 3 in light blue and a light blue Add one button", "Tally · macOS · after Use night colours. Only the data changed.", 900, 239) }}

## Step 5: connect it to the platform

The last file, [`main.ospml`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/tally/main.ospml), connects your screen to the platform. It is about 50 lines, and you can copy it unchanged into a new app: only the `tally::` names change. It gives the host three entry points:

- **argument**: receives any launch arguments. Tally ignores them.
- **open**: draws the first screen from `Screen::first`.
- **event**: receives a tap or an edit, runs `Screen::update`, then draws the next screen.

The project file lists your source folders and the Osprey Native package:

```toml
[project]
name = "tally"
source_roots = ["main.ospml", "src", "../../packages/native/src", "../../packages/graphics/src", "../../packages/json/src"]
default_namespace = "app"
entry = "main.ospml"
```

To see how that file hands the screen to each platform, read [The Osprey Native bridge](/docs/osprey-native/bridge/).

## Step 6: test it without a screen

`view` and `update` are plain functions, so you can test them without opening a window. [`test/main.ospml`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/examples/tally/test/main.ospml) taps buttons in code and checks every control on the screen. Its first test starts like this:

```ospml
test ("the first view is one column of six controls with five bindings in drawing order", \() =>
    bound = drawn Screen::first
    expect (length bound, 5)
    expect ((at 0).placedLook.inset, 32.0)
    expect ((at 2).placedLook.points, 72.0)
```

Run the app's tests:

```sh
make -C src/toolchain/targets/tests tally
```

## Step 7: run it on each platform

Each command builds Tally and opens it:

```sh
make -C src/toolchain/targets/macos tally     # a macOS window
make -C src/toolchain/targets/ios tally       # an iPhone simulator
make -C src/toolchain/targets/android tally   # the Android device named by OSPREY_ANDROID_SERIAL
make -C src/toolchain/targets/windows tally   # a WinUI window, on Windows
```

The same Osprey code opens on each platform, drawn with that platform's own controls:

{{ capture("/assets/images/native/tally-ios.webp", "Tally on iOS: the title Tally, the count 0, Take one, Add one and Clear buttons and a name field", "Tally · iOS · drawn with SwiftUI.", 600, 498) }}

{{ capture("/assets/images/native/tally-android.webp", "Tally on Android: the same title, count, three buttons and name field", "Tally · Android · drawn with Android's own views.", 600, 500) }}

The macOS check opens Tally in a hidden window, taps **Add one** three times and confirms that the window shows the same thing as a window opened at three:

```sh
make -C src/toolchain/targets/macos/tests tally   # prints PASSED
```

## Make it yours

Try one change at a time, then run the app again:

- Change `"Add one"` to `"+1"` in `view`.
- Add a `Twice` action that doubles the count. Add it to `Act`, give it a line in `update` and add a `Press` for it. If you forget the line in `update`, the compiler names the missing case.
- Change `accent` in the day palette and watch every platform pick up the new colour.

Next: [Osprey Native](/docs/osprey-native/) shows larger apps, and [The Osprey Native bridge](/docs/osprey-native/bridge/) explains how the screen reaches each platform.
