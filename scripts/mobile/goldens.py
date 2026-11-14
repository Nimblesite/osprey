#!/usr/bin/env python3
"""Phone screenshots against committed goldens ([NATIVE-HOSTS-MOBILE], AGENTS.md § Apps).

One comparison for every phone test: a picture is decoded to RGB by the standard library alone, so
every runner can compare it with no image tool installed; the system bars are
cropped away so the clock and signal icons never count, and a pixel differs when any channel moves
by more than 24. A picture passes when at most 0.5% of its pixels differ and no 64-pixel tile has
more than 2% changed, so one moved control fails even on a large screen.

    goldens.py ACTUAL GOLDEN_FOLDER [--top FRACTION] [--bottom FRACTION]

The golden is `<width>-ready.png` in GOLDEN_FOLDER, so a phone of another width fails as missing
rather than as different. OSPREY_UPDATE_GOLDENS=1 writes it from ACTUAL for review.
"""
import argparse
import os
from pathlib import Path
import struct
import sys
import zlib

CHANNEL_STEP = 24
WHOLE_LIMIT = 0.005
TILE_LIMIT = 0.02
TILE = 64


SIGNATURE = b"\x89PNG\r\n\x1a\n"
CHANNELS = {2: 3, 6: 4}  # 8-bit truecolour, without and with alpha: what phones and simulators write
# An 8K RGBA screen fits, and malformed dimensions cannot allocate arbitrary scanline storage.
MAX_SCANLINE_BYTES = 128 * 1024 * 1024


def chunks(data):
    """Each (type, body) of a PNG's chunks, in order."""
    at = len(SIGNATURE)
    while at < len(data):
        assert at + 12 <= len(data), "Truncated PNG chunk"
        length, kind = struct.unpack(">I4s", data[at:at + 8])
        assert length <= len(data) - at - 12, "Truncated PNG chunk body"
        body = data[at + 8:at + 8 + length]
        checksum, = struct.unpack(">I", data[at + 8 + length:at + 12 + length])
        assert checksum == zlib.crc32(kind + body), "PNG chunk checksum mismatch"
        yield kind, body
        at += 12 + length
        if kind == b"IEND":
            assert length == 0 and at == len(data), "Invalid PNG end or trailing data"
            return
    raise AssertionError("Missing PNG end chunk")


def header(path, data):
    """(width, height, channels) of an 8-bit, non-interlaced truecolour PNG."""
    assert data.startswith(SIGNATURE), f"{path} is not a PNG"
    assert len(data) >= 33 and data[8:16] == b"\0\0\0\rIHDR", f"{path}: invalid PNG header"
    width, height, depth, colour, compression, filtering, interlace = struct.unpack(">IIBBBBB", data[16:29])
    assert width > 0 and height > 0 and compression == filtering == 0, f"{path}: invalid PNG dimensions or methods"
    assert depth == 8 and colour in CHANNELS and interlace == 0, f"{path}: unsupported PNG (depth {depth}, colour {colour}, interlace {interlace})"
    return width, height, CHANNELS[colour]


def paeth(left, up, corner):
    estimate = left + up - corner
    near_left, near_up, near_corner = abs(estimate - left), abs(estimate - up), abs(estimate - corner)
    return left if near_left <= near_up and near_left <= near_corner else up if near_up <= near_corner else corner


def unfilter(kind, line, previous, step):
    """One scanline with its PNG filter undone in place."""
    assert 0 <= kind <= 4, f"Unknown PNG scanline filter {kind}"
    if kind == 2:
        line[:] = bytes((value + above) & 255 for value, above in zip(line, previous))
        return line
    for i in range(len(line)):
        left = line[i - step] if i >= step else 0
        corner = previous[i - step] if i >= step else 0
        line[i] = (line[i] + (left if kind == 1 else (left + previous[i]) // 2 if kind == 3 else paeth(left, previous[i], corner) if kind == 4 else 0)) & 255
    return line


def png_pixels(path):
    """The picture at `path` as (width, height, rgb24 bytes)."""
    data = Path(path).read_bytes()
    width, height, channels = header(path, data)
    expected = height * (width * channels + 1)
    assert expected <= MAX_SCANLINE_BYTES, f"{path}: PNG exceeds screenshot decoding capacity"
    decoder = zlib.decompressobj()
    try:
        raw = decoder.decompress(b"".join(body for kind, body in chunks(data) if kind == b"IDAT"), expected + 1)
    except zlib.error as failure:
        raise AssertionError(f"{path}: invalid PNG compressed data") from failure
    assert decoder.eof and not decoder.unused_data and len(raw) == expected, f"{path}: invalid PNG scanline size"
    stride, previous, rows = width * channels, bytearray(width * channels), []
    for row in range(height):
        start = row * (stride + 1)
        previous = unfilter(raw[start], bytearray(raw[start + 1:start + 1 + stride]), previous, channels)
        rows.append(bytes(previous) if channels == 3 else bytes(b for i, b in enumerate(previous) if i % 4 != 3))
    return width, height, b"".join(rows)


def png_encode(width, height, pixels, alpha=False):
    """8-bit truecolour PNG bytes for `pixels` (rgb24, or rgba with `alpha`), unfiltered."""
    stride = width * (4 if alpha else 3)
    assert width > 0 and height > 0 and len(pixels) == stride * height, "Invalid PNG dimensions or pixel count"
    rows = b"".join(b"\x00" + pixels[row * stride:(row + 1) * stride] for row in range(height))
    chunk = lambda kind, body: struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))
    head = struct.pack(">IIBBBBB", width, height, 8, 6 if alpha else 2, 0, 0, 0)
    return SIGNATURE + chunk(b"IHDR", head) + chunk(b"IDAT", zlib.compress(rows, 6)) + chunk(b"IEND", b"")


def png_write(path, width, height, pixels):
    """Writes rgb24 `pixels` as a PNG at `path`."""
    Path(path).write_bytes(png_encode(width, height, pixels))


def picture_delta(actual, expected, tile=0):
    """The share of pixels that differ, or with `tile` the worst share in any tile of that size."""
    assert actual[:2] == expected[:2], (actual[:2], expected[:2])
    width, height = actual[:2]
    one, two = actual[2], expected[2]
    assert width > 0 and height > 0 and len(one) == len(two) == width * height * 3, "Invalid screenshot pixel count"
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
