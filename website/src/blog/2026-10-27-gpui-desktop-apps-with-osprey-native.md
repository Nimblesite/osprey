---
layout: page.njk
title: "Desktop Apps on GPUI, Written in Osprey"
excerpt: "Osprey Native now has a GPUI host: the application is Osprey, Zed's Rust UI framework draws it with the GPU. A boids game holds 120 fps on an M4 Max, and a port of GPUI's painting example matches the Rust original pixel for pixel."
description: "How an Osprey application runs in a GPUI window through Osprey Native: the two-function C boundary, scenes and paths as data, the frame clock, measured frame rates, and how this differs from Osprey's GPU compute surface."
date: 2026-10-27
tags: ["blog", "gpui", "osprey-native", "desktop", "rust", "gpu"]
author: "Christian Findlay"
readingTime: 6
image: /assets/images/blog/osprey-native-gpui.png
imageWebp: /assets/images/blog/osprey-native-gpui.webp
imageAlt: "The Flock example in a GPUI window: a thousand pale blue birds wheeling away from a red hawk, a green dog with its herding ring, and a pen on the right"
---

[GPUI](https://gpui.rs) is the UI framework [Zed](https://zed.dev) is built with. It is written in Rust, it draws everything with the GPU, and it is fast enough to hold a code editor at the display's refresh rate. Osprey can now use it. An Osprey application runs in a GPUI window through [Osprey Native](/docs/osprey-native/), the framework that already drives a SwiftUI app and the package manager inside VS Code.

The application is all Osprey: its state, its rules, its screens, every number it draws. A small Rust crate, [`hosts/gpui`](https://github.com/Nimblesite/osprey/tree/main/hosts/gpui), opens the window and turns what Osprey returns into GPUI elements. It makes no decisions of its own.

Two examples run on it today, on macOS:

- **[Flock](https://github.com/Nimblesite/osprey/tree/main/examples/gpui/flock)**: a herding game with a thousand boids, hawks that hunt them, and a dog you steer to drive them into a pen. It runs at **119–120 fps on a 120 Hz M4 Max**, with Osprey's share of each frame between 3.4 and 4.4 ms.
- **[Painting](https://github.com/Nimblesite/osprey/tree/main/examples/gpui/painting)**: a port of GPUI's own `examples/painting.rs`. Captured side by side at the same window size, the Osprey port and the Rust original differ in **zero pixels**, both on first paint and after the same freehand strokes are drawn into each.

## Two functions across the boundary

The Osprey compiler builds the application as a static library. The Rust host links it and calls two exported functions:

```c
const char *start(const char *context);
const char *dispatch(const char *saved, const char *event);
```

Here is the painting port's whole boundary, from [`main.ospml`](https://github.com/Nimblesite/osprey/blob/main/examples/gpui/painting/main.ospml):

```osprey-ml
extern gpui_slice (key : string) -> string

start source =
    handle ui::Slices
        fetch key => gpui_slice key
    Runtime::start (source, \context => App::initial context, \(before, after, frames, writes) => App::regions (before, after, frames, writes), \store => App::save store, App::theme ())

dispatch (saved, event) =
    handle ui::Slices
        fetch key => gpui_slice key
    Runtime::dispatch (saved, event, \(context, store, message) => App::update (context, store, message), \(before, after, frames, writes) => App::regions (before, after, frames, writes), \store => App::save store, \doc => App::restore doc)
```

Each call returns an envelope: an opaque session string for the host to pass back, the regions of the screen whose signals changed, any capability requests (read a file, start the frame clock) and diagnostics. The host draws the regions and sends back events by the ids Osprey issued. A button press reaches `update` as the typed action the button was built with. The host never sees an action, a score or a rule.

Because `update` is pure, the tests drive the real `start`/`dispatch` path with fixture events and check the envelopes. Flock has 13 tests and an [`osprey hunt`](/blog/2026-10-25-symbolic-execution-osprey-hunt/) harness over its rules. None of them needs a window.

## Scenes are data

Buttons, text and pickers map to GPUI's controls. Drawing goes through a `Scene` node: a list of typed primitives (rectangles, discs, line strips, sprites, glyph runs and paths) that the host paints with GPUI's own painter. A path is the same sequence of builder calls GPUI's `PathBuilder` takes. Here is the painting example's star, with its Oklab gradient:

```osprey-ml
star () = fill [ui::MoveTo(toX = 350.0, toY = 200.0), ui::LineTo(toX = 370.0, toY = 260.0), ...] (ui::Linear(angle = 180.0, from = Scenes::hex (16436245, 1.0), fromAt = 0.7, to = Scenes::hex (13987084, 1.0), toAt = 1.0, oklab = true))
```

The encoder sends each primitive as one small array with a type code first, so the Rust painter reads it without a schema. For paths the host replays the steps on GPUI's builder in the same order and adds no call of its own. That is why the port is pixel-identical: GPUI tessellates the same commands it would have received from Rust. Getting there meant matching more than the shapes. Colours travel as float HSL or sRGB so no conversion rounds them, and the header layout has to match too, because GPUI batches paths by draw order and a different header changes how the pie's seams blend.

## A frame clock, not a game loop

Flock animates by asking for the host's frame clock with an `Animate` capability. While it runs, every display refresh reaches `update` as a `Frame` message with the seconds since the last one. `update` moves the birds, and the field region refreshes like any other region. There is no simulation step in Rust.

Getting to the display rate took profiling. The first version spent 7.3 ms of Osprey time per frame at a thousand birds, and over half of a frame went to copying neighbour lists. `osprey --profile` pointed at the hot spots. A sorted-index spatial grid, cheaper change detection and integer-scaled bird state brought `dispatch` under 4.5 ms. The thing still costing the most is the session round trip: the opaque state is parsed and re-encoded on every call, about a third of each frame. At 5,000 birds the frame rate falls well short of the plan's gate, and moving that state into a store the framework keeps in memory is the next lever.

## The framework names no host

Osprey Native was shaped by its first host, a SwiftUI app. Its icons were SF Symbols names, it had capabilities only that product used, and the platform was a string that could be `macos` or `ios`. Before a second native host inherited that, it all came out:

- Icons are a typed `Symbol` union (`Add`, `Delete`, `Warning`, `Package` and so on). GPUI maps them to Lucide icons, VS Code to Codicons, a SwiftUI host to SF Symbols. Each host has a test that fails if any symbol has no icon.
- The platform is a union: `Macos`, `Linux`, `Windows`, `Ios`, `Android`, `Web`. The GPUI host reports the operating system it actually runs on.
- Work only one product can do is a `HostCall` in that host's namespace, not a framework capability.

## GPUI is not GPU compute

Osprey already has a [GPU compute surface](/spec/0034-gpucomputation/): `gpuMap`, `gpuFold` and `gpuMatMul` compile your own Osprey kernels and run them on Metal or OpenCL. It is what runs [GPT-2 on the GPU](/blog/2026-10-12-gpt2-on-the-gpu-surface/). That is raw access: your arithmetic runs on the device, the compiler proves each kernel pure, and the numbers come back to your program.

The GPUI host is something else. Your application is ordinary CPU code that returns a description of what to draw. GPUI's Rust renderer batches that description and rasterizes it with its own shaders. You write no shader and touch no device buffer, and nothing comes back except input events. Use compute for tensors and simulations, and GPUI for windows and 2D drawing. One program can use both: compute a field with `gpuMap`, read it back, draw it as a scene.

## Try it

From a checkout of the repository, on an Apple silicon Mac with Rust installed:

```bash
make gpui-flock      # the herding game
make gpui-painting   # the painting port
make gpui-compare    # captures the port and the Rust original and diffs them
make gpui-test       # the Osprey suites, the host's tests and clippy, and both smoke runs
```

Linux and Windows are next. GPUI already runs on both, so the work is Osprey library targets for them and CI that runs the same smoke tests there. The plan, with its measured numbers, is [plan 0036](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0036-gpui-desktop-host.md). The host's contract is [spec 0046](/spec/0046-gpuidesktophost/).
