# Painting

![The Osprey port of GPUI's painting example running in the GPUI host, with dashed strokes selected](Screenshot.png)

GPUI's `examples/painting.rs` (gpui-pre 0.3.10) ported to Osprey and run in the GPUI host. Everything the window shows comes from the Osprey application: the five black quads, the sixteen paths (translucent squares and circles, the Rust logo, a lightning bolt, an Oklab gradient star, a gradient banner, a pie chart, a bevel-joined wave) and the freehand lines you draw. Rust is only the host.

## Run

```sh
examples/gpui/painting/run.sh
make -C apps/windows painting          # Windows: sandboxed on the WinUI host
```

Drag to draw. Hold Shift to draw a straight line from where the drag began. **Dashed** switches every line to a four-on, two-off dash, and **Solid** switches back. **Clear** removes your lines.

## Test

```sh
./target/release/osprey test examples/gpui/painting/test
```

Seven tests drive the real `start`/`dispatch` boundary: the original's shape count, drawing, a single press, Shift straightening both ways, dashes and clearing.

## Pixel comparison

```sh
examples/gpui/painting/reference/compare.sh [--draw] [out-dir]
```

`reference/` builds the original example, unchanged except for its imports, against the same GPUI as the host. The script captures it, opens the port at the original's exact window size, captures that, and diffs everything below the title bar. With `--draw`, both windows first receive the same synthetic strokes (a freehand zigzag and a Shift-held drag). Both runs report **0 differing pixels** out of 2906 by 1818 on an Apple silicon Mac at 2x.

Three things make that possible:

- **The Rust logo.** `reference/logo.py` generates `src/logo.ospml` from lyon_extra's `rust_logo.rs`. It resolves the relative and smooth SVG commands in `f32` exactly as lyon's builder does, so the host receives the same points.
- **The colours.** Tints travel as floats in the space the original used, sRGB or HSL, so `rgb(0xFF0000).alpha(0.5)` and `gpui::green()` arrive unrounded.
- **The header.** A theme with `button` and `buttonText` tokens makes the host draw plain buttons as flat divs. The header then paints the same primitives as the original. That matters because GPUI orders and batches paths by what was painted before them.

One known difference is below a pixel. The original computes its wave in `f32` with Apple's `sinf`, Osprey in `f64`, and one of the 49 wave points differs by one `f32` ulp. No rendered pixel changes.

The window size varies by a point or two between launches of GPUI's default bounds, so the script sizes the port with `OSPREY_GPUI_WINDOW=<width>x<height>`. It needs Screen Recording permission for the terminal. With `--draw` it also needs Accessibility permission, and the windows must be in front: the strokes are real pointer events.
