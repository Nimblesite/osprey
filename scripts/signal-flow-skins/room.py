"""Lets a skin grow with its window ([UI-MACHINE-CANVAS], [UI-MACHINE-FLOW-SKINS]).

The canvas stays at its fixed zoom, so nothing on it is ever resized; a larger window gives it room.
The host reports the region's size and the machine keeps how far it outgrows the design as
`spareWidth` and `spareHeight`. Everything right of the graph (the inspector and the header's undo
and redo) moves right by the spare width, everything under it (the legend and the keyboard deck)
moves down by the spare height, and the graph between them is one large grid image the panels cover,
so the extra room is more grid to place nodes on. The backdrop is cut into the pieces that move, and
the gap each leaves is filled with solid runs of the colours along its edge.
"""
from PIL import Image, ImageDraw

WIDE = "{{spareWidth}}"
TALL = "{{spareHeight}}"
# The grid image, in canvas units: room for a 4K-wide window at the 0.75 zoom.
GRID = (5120, 2880)
SUPER = 2
# Where the header's left part ends; undo and redo sit right of it and move with the inspector.
SPLIT = 1080


def inputs():
    """The region's size the host reports and the room it spares the design, all in canvas units."""
    return {name: {"type": "number", "value": 0} for name in ("viewWidth", "viewHeight", "spareWidth", "spareHeight")}


def bounds(skin):
    """The graph's right and bottom edges and the top of the inspector, from the skin's graph box."""
    _, top, right, bottom = skin["graph"]
    return right + 1, top, bottom


NAMES = ["header", "tools", "inspector", "legend"]


def pieces(skin):
    """Each moving piece of the backdrop as (name, box cut from the mock), in atlas order after the grid."""
    right, top, bottom = bounds(skin)
    boxes = [(0, 0, SPLIT, top), (SPLIT, 0, 1536, top), (right, top, 1536, bottom), (0, bottom, right, skin["height"])]
    return list(zip(NAMES, boxes))


def cut(skin, px, folder):
    """Saves the backdrop's moving pieces and the grid behind them, and keeps the gaps' fills on the skin."""
    image = Image.fromarray(px.clip(0, 255).astype("uint8"))
    for name, box in pieces(skin):
        image.crop(box).save(folder / f"{name}.png", optimize=True)
    grid(skin, folder)
    skin["fills"] = fills(skin, px)


def runs(colours):
    """Consecutive near-equal colours as (start, length, #RRGGBB)."""
    out, start = [], 0
    for i in range(1, len(colours) + 1):
        if i == len(colours) or abs(colours[i] - colours[start]).max() > 1.5:
            r, g, b = colours[start:i].mean(axis=0).round().astype(int)
            out.append((start, i - start, f"#{r:02X}{g:02X}{b:02X}"))
            start = i
    return out


def fill(x, y, w, h, colour):
    """A solid run one depth over the backdrop's sprites, since a path at their depth draws under them."""
    return {"shape": "rounded", "x": x, "y": y, "w": w, "h": h, "radius": 0, "colour": colour, "depth": 1}


def fills(skin, px):
    """The header's gap under undo and redo, the panel under the inspector and the legend's gap, as runs of their edge's colours."""
    right, top, bottom = bounds(skin)
    height = skin["height"]
    header = [fill(SPLIT, y, WIDE, n, c) for y, n, c in runs(px[0:top, SPLIT - 6])]
    panel = [fill(f"{WIDE} {right + x}", bottom, n, f"{TALL} {height - bottom}", c) for x, n, c in runs(px[bottom - 4, right:1536])]
    legend = [fill(right, f"{TALL} {bottom + y}", WIDE, n, c) for y, n, c in runs(px[bottom:height, right - 10])]
    return header + panel + legend


def grid(skin, folder):
    """The graph's background: the canvas colour ruled at the skin's grid period, drawn large and filtered down."""
    w, h = GRID
    pw, ph = skin["grid"]
    canvas = tuple(int(skin["tokens"]["canvas"][i:i + 2], 16) for i in (1, 3, 5))
    big = Image.new("RGB", (w * SUPER, h * SUPER), canvas)
    draw = ImageDraw.Draw(big)
    line = (22, 19, 42)
    for k in range(int(w / pw) + 1):
        x = round(k * pw * SUPER)
        draw.rectangle([x, 0, x + SUPER, h * SUPER], fill=line)
    for k in range(int(h / ph) + 1):
        y = round(k * ph * SUPER)
        draw.rectangle([0, y, w * SUPER, y + SUPER], fill=line)
    big.resize((w, h), Image.LANCZOS).save(folder / "grid.png", optimize=True)


def atlas(key):
    """The pieces' images; the grid takes the backdrop's place as atlas entry 0."""
    return [f"{{{{assets}}}}/skins/{key}/{name}.png" for name in NAMES]


def half(size):
    """Half a size, which may be a measure: each `{{input}}` scaled by a half, each number halved."""
    if not isinstance(size, str):
        return size / 2
    return " ".join(part.replace("}}", "|scale:0.5:0}}") if part.startswith("{{") else str(float(part) / 2) for part in size.split())


def centre(at, size):
    both = isinstance(at, (int, float)) and isinstance(size, (int, float))
    return at + size / 2 if both else f"{at} {half(size)}"


def sprite(index, x, y, w, h):
    """A backdrop piece placed by its top-left corner, as sprites are drawn about their centre; any of the four may be a measure."""
    return {"shape": "sprite", "index": index, "x": centre(x, w), "y": centre(y, h), "w": w, "h": h, "depth": 0}


def backdrop(skin, first):
    """The grid (atlas entry 0), then each piece where it sits once moved and the fills of the gaps; the pieces are atlas entries from `first` on."""
    right, top, bottom = bounds(skin)
    grid_w, grid_h = GRID
    return [sprite(0, 0, top, grid_w, grid_h),
            sprite(first, 0, 0, SPLIT, top),
            sprite(first + 1, f"{WIDE} {SPLIT}", 0, 1536 - SPLIT, top),
            sprite(first + 2, f"{WIDE} {right}", top, 1536 - right, bottom - top),
            sprite(first + 3, 0, f"{TALL} {bottom}", right, skin["height"] - bottom)] + skin["fills"]


def mock(value):
    """Where a coordinate sits in the mock: a number, the number a node's drag offset is added to, or the offset a `{{input|scale:k:offset}}` starts from."""
    if not isinstance(value, str):
        return value
    last = value.split()[-1]
    try:
        return float(last.rstrip("}").split(":")[-1] if last.endswith("}}") else last)
    except ValueError:
        return None


def pinned(skin, item):
    """The item moved right if it sits right of the graph, and down if it sits under it."""
    right, top, bottom = bounds(skin)
    points = item.get("points")
    x, y = (mock(points[0][0]), mock(points[0][1])) if points else (mock(item.get("x")), mock(item.get("y")))
    if x is None or y is None:
        return item
    dx = x >= right or (x >= SPLIT and y < top)
    dy = y >= bottom
    move = lambda v, by, spare: f"{spare} {v}" if by else v
    if points:
        return {**item, "points": [[move(px, dx, WIDE), move(py, dy, TALL)] for px, py in points]}
    return {**item, "x": move(item["x"], dx, WIDE), "y": move(item["y"], dy, TALL)}
