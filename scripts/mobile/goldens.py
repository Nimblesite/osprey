#!/usr/bin/env python3
"""Phone screenshots against committed goldens ([NATIVE-HOSTS-MOBILE], AGENTS.md § Apps).

One comparison for every phone test: a picture is decoded to RGB with ffmpeg, the system bars are
cropped away so the clock and signal icons never count, and a pixel differs when any channel moves
by more than 24. A picture passes when at most 0.5% of its pixels differ and no 64-pixel tile has
more than 2% changed, so one moved control fails even on a large screen.

    goldens.py ACTUAL GOLDEN_FOLDER [--top FRACTION] [--bottom FRACTION]

The golden is `<width>-ready.png` in GOLDEN_FOLDER, so a phone of another width fails as missing
rather than as different. OSPREY_UPDATE_GOLDENS=1 writes it from ACTUAL for review.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

CHANNEL_STEP = 24
WHOLE_LIMIT = 0.005
TILE_LIMIT = 0.02
TILE = 64


def png_pixels(path):
    """The picture at `path` as (width, height, rgb24 bytes)."""
    probe = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height",
                            "-of", "json", str(path)], check=True, stdout=subprocess.PIPE)
    size = json.loads(probe.stdout)["streams"][0]
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(path), "-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"],
                         check=True, stdout=subprocess.PIPE).stdout
    width, height = size["width"], size["height"]
    assert len(raw) == width * height * 3, (path, width, height, len(raw))
    return width, height, raw


def png_write(path, width, height, pixels):
    """Writes rgb24 `pixels` as a PNG at `path`."""
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "rawvideo", "-pixel_format", "rgb24", "-video_size",
                    f"{width}x{height}", "-i", "pipe:0", "-frames:v", "1", str(path)], check=True, input=pixels)


def picture_delta(actual, expected, tile=0):
    """The share of pixels that differ, or with `tile` the worst share in any tile of that size."""
    assert actual[:2] == expected[:2], (actual[:2], expected[:2])
    width, height = actual[:2]
    one, two = actual[2], expected[2]
    changed = [any(abs(one[i + channel] - two[i + channel]) > CHANNEL_STEP for channel in range(3))
               for i in range(0, len(one), 3)]
    if not tile: return sum(changed) / (width * height)
    def region(x, y):
        right, bottom = min(width, x + tile), min(height, y + tile)
        count = sum(sum(changed[row * width + x:row * width + right]) for row in range(y, bottom))
        return count / ((right - x) * (bottom - y))
    return max(region(x, y) for y in range(0, height, tile) for x in range(0, width, tile))


def cropped(picture, top, bottom):
    """`picture` without the given fractions of its height at the top and the bottom."""
    width, height, raw = picture
    first, last = int(height * top), height - int(height * bottom)
    return width, last - first, raw[first * width * 3:last * width * 3]


def verify(actual_path, folder, top, bottom):
    """Compares the cropped picture with its golden, writing the crop beside the picture for review."""
    actual = cropped(png_pixels(actual_path), top, bottom)
    crop = actual_path.with_name(actual_path.stem + "-content.png")
    png_write(crop, *actual)
    golden = folder / f"{actual[0]}-ready.png"
    if os.environ.get("OSPREY_UPDATE_GOLDENS") == "1":
        folder.mkdir(parents=True, exist_ok=True)
        golden.write_bytes(crop.read_bytes())
    assert golden.exists(), f"Missing {golden}; rerun with OSPREY_UPDATE_GOLDENS=1 and review the image"
    expected = png_pixels(golden)
    whole, worst = picture_delta(actual, expected), picture_delta(actual, expected, tile=TILE)
    assert whole <= WHOLE_LIMIT and worst <= TILE_LIMIT, \
        f"{whole:.2%} of pixels differ, worst tile {worst:.2%}; inspect {crop} against {golden}"
    print(f"PASS: {golden.relative_to(Path.cwd()) if golden.is_relative_to(Path.cwd()) else golden} ({whole:.3%} differing pixels)")


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("actual", type=Path)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--top", type=float, default=0.08, help="share of the height the status bar takes")
    parser.add_argument("--bottom", type=float, default=0.05, help="share of the height the home indicator takes")
    arguments = parser.parse_args()
    try:
        verify(arguments.actual, arguments.folder, arguments.top, arguments.bottom)
    except AssertionError as failure:
        print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
