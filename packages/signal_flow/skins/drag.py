"""Makes a skin's nodes draggable ([UI-MACHINE-DRAG], [UI-MACHINE-FLOW-SKINS]).

Each node is cut out of the mock as its own sprite (card, ports and the port labels outside it), and
the graph behind is rebuilt from whole grid periods of empty canvas, so a moved node leaves grid,
not a hole. Every shape and listener on a node then sits at `{{<role>X}} x`, `{{<role>Y}} y`: the
node's drag offset added to where the mock drew it. The cables are canvas `cable` shapes between
the offset ports, so they follow both ends, and pressing a node's body grabs its offsets.
"""
import numpy as np
from PIL import Image, ImageDraw

from spec import ROLES

MARGIN = 16


def offsets():
    """The two number inputs per node that hold how far it has been dragged."""
    return {f"{role}{axis}": {"type": "number", "value": 0} for role in ROLES for axis in "XY"}


def box(skin, i):
    """The node's sprite bounds: its card and ports with a margin, and the port labels outside it."""
    x0, y0, x1, y1 = skin["nodes"][i]["rect"]
    rects = [(x0 - MARGIN, y0 - MARGIN, x1 + MARGIN, y1 + MARGIN)] + skin["labels"].get(ROLES[i], [])
    return min(r[0] for r in rects), min(r[1] for r in rects), max(r[2] for r in rects), max(r[3] for r in rects)


def card_mask(skin, i, size, origin):
    """The node's opaque parts in sprite space: its rounded card and its port rings."""
    mask = Image.new("L", size, 0)
    draw = ImageDraw.Draw(mask)
    x0, y0, x1, y1 = skin["nodes"][i]["rect"]
    ox, oy = origin
    draw.rounded_rectangle([x0 - ox - 2, y0 - oy - 2, x1 - ox + 2, y1 - oy + 2], skin["radius"] + 2, fill=255)
    r = skin["port"]["r"] + 3
    for cx, cy, _ in skin["nodes"][i]["ports"]:
        draw.ellipse([cx - ox - r, cy - oy - r, cx - ox + r, cy - oy + r], fill=255)
    return np.array(mask).astype(np.float64)


def lettering(rgb, rect, origin):
    """A port label's opacity: how far each pixel stands out from the label box's own background."""
    ox, oy = origin
    x0, y0, x1, y1 = (rect[0] - ox, rect[1] - oy, rect[2] - ox, rect[3] - oy)
    patch = rgb[y0:y1, x0:x1]
    ground = np.median(patch.reshape(-1, 3), axis=0)
    return (x0, y0, x1, y1), np.clip(np.abs(patch - ground).max(axis=2) * 3.5, 0, 255)


def sprite(skin, px, i):
    """One node cut out of the painted-out mock as RGBA."""
    left, top, right, bottom = box(skin, i)
    rgb = px[top:bottom, left:right]
    alpha = card_mask(skin, i, (right - left, bottom - top), (left, top))
    for rect in skin["labels"].get(ROLES[i], []):
        (x0, y0, x1, y1), ink = lettering(rgb, rect, (left, top))
        alpha[y0:y1, x0:x1] = np.maximum(alpha[y0:y1, x0:x1], ink)
    return Image.fromarray(np.dstack([rgb, alpha]).clip(0, 255).astype(np.uint8), "RGBA")


def covered(skin, shape):
    """Everything the nodes, labels, cables and badge draw, which the backdrop must not keep."""
    mask = Image.new("L", shape, 0)
    draw = ImageDraw.Draw(mask)
    for n in skin["nodes"]:
        x0, y0, x1, y1 = n["rect"]
        draw.rectangle([x0 - MARGIN, y0 - MARGIN, x1 + MARGIN, y1 + MARGIN], fill=255)
    for rect in [r for rects in skin["labels"].values() for r in rects] + skin["wires"]:
        draw.rectangle(list(rect), fill=255)
    return np.array(mask) > 0


def regrid(skin, px):
    """Rebuilds the graph under every node and cable from empty canvas a whole number of grid periods away."""
    mask = covered(skin, (px.shape[1], px.shape[0]))
    gx0, gy0, gx1, gy1 = skin["graph"]
    pw, ph = skin["grid"]
    todo = mask.copy()
    todo[:gy0], todo[gy1:], todo[:, :gx0], todo[:, gx1:] = False, False, False, False
    shifts = sorted(((round(i * pw), round(j * ph)) for i in range(-24, 25) for j in range(-24, 25) if i or j),
                    key=lambda s: abs(s[0]) + abs(s[1]))
    for dx, dy in shifts:
        ys, xs = np.nonzero(todo)
        if not len(ys):
            break
        sx, sy = xs + dx, ys + dy
        ok = (sx >= gx0) & (sx < gx1) & (sy >= gy0) & (sy < gy1)
        ok[ok] = ~mask[sy[ok], sx[ok]]
        px[ys[ok], xs[ok]] = px[sy[ok], sx[ok]]
        todo[ys[ok], xs[ok]] = False


def cut(key, skin, px, folder):
    """Saves each node's sprite from the painted-out mock, then clears the nodes and cables out of it."""
    for i, role in enumerate(ROLES):
        sprite(skin, px, i).save(folder / f"node-{role}.png", optimize=True)
    regrid(skin, px)


# ---- shapes that follow their node ----

def at(role, axis, value):
    """A coordinate moved by its node's drag offset."""
    return f"{{{{{role}{axis}}}}} {value}"


def anchor(value):
    """Where a coordinate sits in the mock: a number, or the offset a `{{input|scale:k:offset}}` adds."""
    return value if not isinstance(value, str) else float(value.rstrip("}").split(":")[-1])


def owner(skin, x, y):
    """The node whose sprite holds the point, or None."""
    for i, role in enumerate(ROLES):
        left, top, right, bottom = box(skin, i)
        if left <= x <= right and top <= y <= bottom:
            return role
    return None


def followed(skin, item):
    """The item moved with the node it sits on; anything elsewhere stays put."""
    points = item.get("points")
    x, y = points[0] if points else (item.get("x"), item.get("y"))
    if x is None or item.get("shape") == "cable":
        return item
    role = owner(skin, anchor(x), anchor(y))
    if role is None:
        return item
    if points:
        return {**item, "points": [[at(role, "X", px), at(role, "Y", py)] for px, py in points]}
    return {**item, "x": at(role, "X", x), "y": at(role, "Y", y)}


def port(skin, role, index):
    cx, cy, _ = skin["nodes"][ROLES.index(role)]["ports"][index]
    return [at(role, "X", cx), at(role, "Y", cy)]


def cables(skin):
    """Each connection as a cable between its offset ports, dashed for modulation, arrowed at its middle."""
    return [{"shape": "cable", "points": [port(skin, a, i), port(skin, b, j)], "stroke": 2.5, "colour": f"@{kind}",
             "size": 10, "depth": 1, **({"dash": 5} if kind == "mod" else {})} for a, i, b, j, kind in skin["cables"]]


def badge(skin):
    """The modulation depth on its cable, held beside the cable's middle wherever its ends go."""
    (a, i, b, j, kind), (dx, w, h), label = skin["cables"][skin["badge"][0]], skin["badge"][1], skin["badge"][2]
    (ax, ay, _), (bx, by, _) = skin["nodes"][ROLES.index(a)]["ports"][i], skin["nodes"][ROLES.index(b)]["ports"][j]
    mx, my = (ax + bx) / 2 + dx, (ay + by) / 2
    x = f"{{{{{a}X|scale:0.5:0}}}} {{{{{b}X|scale:0.5:0}}}} {mx}"
    y = f"{{{{{a}Y|scale:0.5:0}}}} {{{{{b}Y|scale:0.5:0}}}} {my}"
    left = f"{x} {-w / 2}"
    top = f"{y} {-h / 2}"
    return [{"shape": "rounded", "x": left, "y": top, "w": w, "h": h, "radius": h / 2, "colour": "@canvas", "depth": 1},
            {"shape": "rounded", "x": left, "y": top, "w": w, "h": h, "radius": h / 2, "stroke": 1.5, "colour": f"@{kind}", "depth": 1},
            {"shape": "text", "text": label, "x": x, "y": f"{y} -8.4", "size": 14, "colour": "@modText", "anchor": 1, "depth": 1}]


def nodes(skin, first):
    """The node sprites, atlas entries from `first` on, each centred where the mock drew it plus its offset."""
    out = []
    for i, role in enumerate(ROLES):
        left, top, right, bottom = box(skin, i)
        out.append({"shape": "sprite", "index": first + i, "x": at(role, "X", (left + right) / 2), "y": at(role, "Y", (top + bottom) / 2),
                    "w": right - left, "h": bottom - top, "depth": 2})
    return out


def grabbed(listener, role):
    """A node's select listener that also drags it."""
    return {**listener, "drag": {"x": f"{role}X", "y": f"{role}Y"}}
