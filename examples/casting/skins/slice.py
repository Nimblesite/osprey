"""Slices the Sherbet Signal mock (designs/sherbet-signal.png) into the canvas skin's artwork.

The responsive canvas draws its background and controls as vectors. `disc.png` is the
disc alone, masked to its rim, which the host turns while playing. `transport.png` is
the round transport button without its glyph. Run with Pillow and NumPy:

    python3 examples/casting/skins/slice.py
"""
from pathlib import Path

import numpy as np
from PIL import Image

HERE = Path(__file__).resolve().parent
MOCK = HERE.parent / "designs" / "sherbet-signal.png"
OUT = HERE / "sherbet"

DISC_X, DISC_Y, DISC_R = 624, 377, 246
BUTTON_X, BUTTON_Y, BUTTON_R = 626, 1000, 78


def mock():
    return np.array(Image.open(MOCK).convert("RGBA")).astype(np.float64)


def fill_rows(px, x0, y0, x1, y1):
    """Paints a rectangle out row by row, blending the colours at its left and right edges."""
    for y in range(y0, y1):
        a, b = px[y, x0 - 2], px[y, x1 + 2]
        for x in range(x0, x1):
            t = (x - x0) / max(1, x1 - x0)
            px[y, x] = a * (1 - t) + b * t


def circle(px, cx, cy, r, edge=1.5):
    """A square crop around a circle, transparent outside it with an antialiased rim."""
    crop = px[cy - r:cy + r, cx - r:cx + r].copy()
    ys, xs = np.mgrid[0:2 * r, 0:2 * r]
    d = np.hypot(xs + 0.5 - r, ys + 0.5 - r)
    crop[..., 3] = np.clip((r - d) / edge, 0, 1) * 255
    return crop


def transport(px):
    """The round button with its two bars painted out from the button's own colours."""
    out = px.copy()
    fill_rows(out, 606, 978, 646, 1024)
    return circle(out, BUTTON_X, BUTTON_Y, BUTTON_R, edge=10)


def save(array, name):
    Image.fromarray(np.clip(array, 0, 255).astype(np.uint8), "RGBA").save(OUT / name, optimize=True)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    px = mock()
    save(circle(px, DISC_X, DISC_Y, DISC_R), "disc.png")
    save(transport(px), "transport.png")


if __name__ == "__main__":
    main()
