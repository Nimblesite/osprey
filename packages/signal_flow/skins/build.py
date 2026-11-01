"""Builds the four canvas skins of signal_flow.machine.json from the designs ([UI-MACHINE-FLOW-SKINS]).

Each design is sliced the way the casting player's Sherbet skin is: `backdrop.png` is the mock with
every live part (toggles, slider fills and knobs, values, health rows and the inspector's contents)
painted out, and the process node's selection ring redrawn as an ordinary border, so selection can
move. `icon-<role>.png` is each node's icon for the inspector. The live parts are then written as
canvas shapes and listeners bound to the machine's inputs, replacing the document's `skins`.
Run with Pillow and NumPy:

    python3 packages/signal_flow/skins/build.py
"""
import json
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

import drag
import keys
from spec import RATES, ROLES, SKINS, WAVES
from traces import meters, shape, traces

HERE = Path(__file__).resolve().parent
DESIGNS = HERE.parent / "design"
DOCUMENT = HERE.parent / "signal_flow.machine.json"
SUPER = 4
ZOOM = 0.75


def rgb(hex_colour):
    return tuple(int(hex_colour[i:i + 2], 16) for i in (1, 3, 5))


def fill_rows(px, x0, y0, x1, y1):
    """Paints a rectangle out row by row, blending the colours just outside its left and right edges."""
    for y in range(y0, y1):
        a, b = px[y, x0 - 2].copy(), px[y, x1 + 2].copy()
        t = (np.arange(x0, x1) - x0)[:, None] / max(1, x1 - x0)
        px[y, x0:x1] = a * (1 - t) + b * t


def unring(image, skin):
    """Redraws the process node's selection ring as a plain border, keeping its ports."""
    tokens = skin["tokens"]
    x0, y0, x1, y1 = skin["nodes"][2]["rect"]
    pad = 8
    box = (x0 - pad, y0 - pad, x1 + pad, y1 + pad)
    crop = image.crop(box).resize(((box[2] - box[0]) * SUPER, (box[3] - box[1]) * SUPER), Image.LANCZOS)
    draw = ImageDraw.Draw(crop)
    s, r = SUPER, skin["radius"]
    rect = lambda d: [(pad - d) * s, (pad - d) * s, (pad + x1 - x0 + d) * s, (pad + y1 - y0 + d) * s]
    draw.rounded_rectangle(rect(3), (r + 3) * s, outline=rgb(tokens["canvas"]), width=4 * s)
    draw.rounded_rectangle(rect(-2.5), (r - 2) * s, outline=rgb(tokens["fill"]), width=3 * s)
    draw.rounded_rectangle(rect(0), r * s, outline=rgb(tokens["border"]), width=int(1.5 * s))
    out = image.copy()
    out.paste(crop.resize((box[2] - box[0], box[3] - box[1]), Image.LANCZOS), box[:2])
    for cx, cy, _ in skin["nodes"][2]["ports"]:
        pr = skin["port"]["r"] + 2
        mask = Image.new("L", (pr * 2, pr * 2), 0)
        ImageDraw.Draw(mask).ellipse([0, 0, pr * 2 - 1, pr * 2 - 1], fill=255)
        out.paste(image.crop((cx - pr, cy - pr, cx + pr, cy + pr)), (cx - pr, cy - pr), mask)
    return out


def slice_skin(key, skin):
    folder = HERE / key
    folder.mkdir(parents=True, exist_ok=True)
    mock = Image.open(DESIGNS / skin["design"]).convert("RGB")
    for i, n in enumerate(skin["nodes"]):
        mock.crop(n["icon"]).save(folder / f"icon-{ROLES[i]}.png", optimize=True)
    px = np.array(unring(mock, skin)).astype(np.float64)
    for n in skin["nodes"]:
        cx, cy, w, h = n["toggle"]
        fill_rows(px, int(cx - w / 2 - 4), int(cy - h / 2 - 4), int(cx + w / 2 + 4), int(cy + h / 2 + 4))
    for x0, y0, x1, y1 in skin["erase"]:
        fill_rows(px, x0, y0, x1, y1)
    if "shift" in skin:
        (x0, y0, x1, y1), dx = skin["shift"]
        px[y0:y1, x0 + dx:x1 + dx] = px[y0:y1, x0:x1].copy()
        px[y0:y1, x0:x0 + dx] = px[y0:y1, x0 + dx + 4:x0 + dx + 5]
    if "cables" in skin:
        drag.cut(key, skin, px, folder)
    Image.fromarray(np.clip(px, 0, 255).astype(np.uint8)[:skin["height"]]).save(folder / "backdrop.png", optimize=True)


# ---- the Prism logo ----

LOGO = [(0.0, "#F59AC5"), (0.12, "#E256C9"), (0.3, "#B36AF7"), (0.45, "#559FFB"), (0.62, "#5AE2D7"), (0.75, "#8EE07A"),
        (0.88, "#F69369"), (1.0, "#F59AC5")]


def conic(size, cx, cy):
    """LOGO's colours swept clockwise from the top around (cx, cy)."""
    ys, xs = np.mgrid[0:size, 0:size]
    turn = (np.arctan2(xs - cx, cy - ys) / (2 * np.pi)) % 1.0
    stops = np.array([t for t, _ in LOGO])
    colours = np.array([rgb(c) for _, c in LOGO], dtype=np.float64)
    return np.stack([np.interp(turn, stops, colours[:, c]) for c in range(3)], axis=-1)


def logo(size=1024):
    """The Prism mark as a macOS app icon: the header's rainbow triangle on its dark tile, drawn large rather than scaled up."""
    s = size * SUPER
    tile = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    draw = ImageDraw.Draw(tile)
    inset, radius = s * 0.098, s * 0.18
    draw.rounded_rectangle([inset, inset, s - inset, s - inset], radius, fill=rgb("#211F3E"), outline=rgb("#4A4790"), width=int(s * 0.012))
    corners = [(s * 0.5, s * 0.27), (s * 0.73, s * 0.69), (s * 0.27, s * 0.69)]
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).line(corners + corners[:2], fill=255, width=int(s * 0.085), joint="curve")
    cx, cy = s * 0.5, sum(y for _, y in corners) / 3
    tile.paste(Image.fromarray(conic(s, cx, cy).astype(np.uint8)), (0, 0), mask)
    tile.resize((size, size), Image.LANCZOS).save(HERE / "prism" / "logo.png", optimize=True)


# ---- shapes ----

def on(role, flag=True):
    return {"input": f"{role}On", "eq": flag}


def chosen(role):
    return {"input": "selected", "eq": role}


def both(*conditions):
    live = [c for c in conditions if c]
    return live[0] if len(live) == 1 else {"all": live}


def text(words, x, cy, size, colour, anchor=0, weight=400, when=None):
    return shape("text", when, text=words, x=x, y=round(cy - size * 0.6, 1), size=size, colour=f"@{colour}", anchor=anchor, weight=weight, depth=6)


def disc(x, y, r, colour, depth=5, when=None):
    return shape("disc", when, x=x, y=y, radius=r, colour=colour if colour.startswith("@") else f"@{colour}", depth=depth)


def toggle(skin, flag, cx, cy, w, h, when=None, inverse=False):
    """A switch drawn on when `flag` is true (or false when `inverse`)."""
    tokens = skin["tokens"]
    lit, dark = {"input": flag, "eq": not inverse}, {"input": flag, "eq": inverse}
    r = h / 2 - 2.5
    travel = w / 2 - h / 2
    track = lambda colour, cond: shape("rounded", both(when, cond), x=cx - w / 2, y=cy - h / 2, w=w, h=h, radius=h / 2, colour=colour, depth=5)
    return [
        track("@switch" if "switch" in tokens else "@accent", lit),
        track("@switchOff" if "switchOff" in tokens else "@track", dark),
        disc(cx + travel, cy, r, "knob", 6, both(when, lit)),
        disc(cx - travel, cy, r, "knobOff" if "knobOff" in tokens else "knob", 6, both(when, dark)),
    ]


KNOB = {"topology": ("knob", 10, 7.5), "notation": ("ink", 9, 7.5), "paper": ("accent", 9.5, 7), "prism": (None, 9, 9)}


def slider(key, name, x0, x1, cy, fill, knob, when=None):
    width = x1 - x0
    ring, outer, inner = KNOB[key]
    at = f"{{{{{name}|scale:{width}:{x0}}}}}"
    parts = [shape("rounded", when, x=x0, y=cy - 3.5, w=width, h=7, radius=3.5, colour="@track", depth=5),
             shape("rounded", when, x=x0, y=cy - 3.5, w=f"{{{{{name}|scale:{width}:0}}}}", h=7, radius=3.5, colour=f"@{fill}", depth=5)]
    if ring:
        parts.append(disc(at, cy, outer, ring, 6, when))
    return parts + [disc(at, cy, inner, knob, 6, when)]


def bound(skin, role):
    """The input a node's parameter sets: its own role unless the parameter names another."""
    param = skin["params"].get(role, ())
    return param[3] if len(param) > 3 else role


def node_shapes(key, skin):
    out = []
    for role, n in zip(ROLES, skin["nodes"]):
        x0, y0, x1, y1 = n["rect"]
        rect = dict(x=x0, y=y0, w=x1 - x0, h=y1 - y0, radius=skin["radius"])
        if skin.get("glow"):
            out.append(shape("rounded", chosen(role), **rect, stroke=8, colour="@accent", alpha=0.22, depth=3))
        out.append(shape("rounded", chosen(role), **rect, stroke=skin["ring"], colour="@accent", depth=3))
        out.append(shape("rounded", on(role, False), **rect, colour="@canvas", alpha=0.6, depth=3))
        out += toggle(skin, f"{role}On", *n["toggle"])
        out += ports(skin, n)
    for role, x0, x1, cy, fill, knob in skin["sliders"]:
        out += slider(key, bound(skin, role), x0, x1, cy, fill, knob)
    out += [text(*t[:5], anchor=t[5], weight=t[6]) for t in skin["texts"]]
    for role, x, cy in skin["health"]:
        out += [disc(x, cy, 6, f"@{{{{{role}Health}}}}", 6), text(f"{{{{{role}Health}}}}", x + 14, cy, 14, "ink")]
    return out


def ports(skin, n):
    style = skin["port"]
    out = []
    for cx, cy, kind in n["ports"]:
        out.append(disc(cx, cy, style["r"], kind, 4))
        if style["ring"]:
            out.append(disc(cx, cy, style["r"] - style["ring"], "port", 4))
        if style["core"]:
            out.append(disc(cx, cy, style["core"], kind, 4))
    return out


# ---- the inspector ----

def link_row(key, skin, link, y, when):
    """One connection of the selected node, laid out as its design lists them."""
    dot, port, direction, peer, note = link
    ins = skin["inspector"]
    if key == "topology":
        return [disc(1255, y, 9, dot, 6, when), disc(1255, y, 6.5, "panel", 6, when), disc(1255, y, 5, dot, 6, when),
                text(port, 1277, y, 15, "ink", when=when), text(direction, 1362, y, 18, "muted", 1, when=when),
                text(peer, 1386, y, 15, "ink", when=when), text(note, 1386, y + 25, 13, "muted", when=when)]
    if key == "notation":
        return [disc(1259, y, 9, dot, 6, when), text(port, 1286, y, 16, "ink", when=when), text(peer, ins["peer"], y, 16, "ink", 2, when=when),
                text("›", 1505, y, 20, "ink", 1, when=when)]
    if key == "paper":
        return [disc(1264, y, 8, "accent", 6, when), disc(1264, y, 5, "panel", 6, when), text(port, 1286, y, 16, "ink", when=when),
                text(direction, ins["arrow"], y, 15, "muted", when=when), text(peer, ins["peer"], y, 16, "muted", when=when),
                text("›", 1503, y, 20, "muted", 1, when=when)]
    return [disc(1243, y, 8, dot, 6, when), text(port, 1264, y, 14, "ink", when=when), text(direction, ins["dir"], y, 14, "muted", when=when),
            text("←" if direction == "In" else "→", ins["arrow"], y, 16, "muted", 1, when=when), text(peer, ins["peer"], y, 15, "ink", when=when)]


def parameter(key, skin, role, when):
    ins = skin["inspector"]
    lx, ly, lsize = ins["param"]
    if role not in skin["params"]:
        return [text("No parameters", lx, ly, lsize, "muted", when=when)]
    label, value, note = skin["params"][role][:3]
    vx, vy, vsize, anchor = ins["value"]
    bx0, by0, bx1, by1 = ins["box"]
    box = dict(x=bx0, y=by0, w=bx1 - bx0, h=by1 - by0, radius=6)
    out = [shape("rounded", when, **box, colour="@fill", depth=5), shape("rounded", when, **box, stroke=1, colour="@line", depth=5),
           text(label, lx, ly, lsize, "ink", weight=600 if key == "topology" else 400, when=when),
           text(value, vx, vy, vsize, "ink", anchor, when=when)]
    out += slider(key, bound(skin, role), *ins["slider"], "accent", "accent" if key != "notation" else "knob", when)
    if isinstance(note, tuple):
        out += [text(note[0], ins["slider"][0], ins["range"], 15, "muted", when=when), text(note[1], ins["slider"][1], ins["range"], 15, "muted", 2, when=when)]
    elif note:
        out.append(text(note, 1245, ins["note"][1], ins["note"][2], "muted", when=when))
    return out


def inspector_header(key, skin, i, role, n, when):
    """The selected node's icon, name, kind and switches."""
    ins = skin["inspector"]
    cx, cy, w, h = ins["icon"]
    tx, ty, tsize = ins["title"]
    out = [shape("sprite", when, index=i + 1, x=cx, y=cy, w=w, h=h, depth=5),
           text(n["name"], tx, ty, tsize, "ink", weight=700 if key != "notation" else 400, when=when)]
    if "kind" in ins:
        out.append(text(n["kind"], ins["kind"][0], ins["kind"][1], ins["kind"][2], "muted", when=when))
    if "label" in ins:
        out.append(text(n["name"], *ins["label"], "ink", when=when))
    out += toggle(skin, f"{role}On", *ins["toggle"], when=when)
    if "enabled" in ins:
        out += toggle(skin, f"{role}On", *ins["enabled"], when=when)
    if "bypass" in ins:
        out += toggle(skin, f"{role}On", *ins["bypass"], when=when, inverse=True)
    return out


def inspector_links(key, skin, role, when):
    """The selected node's connections and, where the design shows one, its health."""
    ins = skin["inspector"]
    links = skin["links"][role]
    out = [text(f"Connections ({len(links)})", *ins["count"], "ink", weight=600, when=when)] if "count" in ins else []
    y0, step = ins["rows"]
    for j, link in enumerate(links):
        out += link_row(key, skin, link, round(y0 + j * step, 1), when)
    if "status" in ins:
        sx, sy, _ = ins["status"]
        out += [disc(sx, sy, 6, f"@{{{{{role}Health}}}}", 6, when), text(f"{{{{{role}Health}}}}", sx + 15, sy, 16, "ink", when=when)]
    return out


def inspector(key, skin):
    ins = skin["inspector"]
    out = []
    for i, (role, n) in enumerate(zip(ROLES, skin["nodes"])):
        when = chosen(role)
        out += inspector_header(key, skin, i, role, n, when) + parameter(key, skin, role, when) + inspector_links(key, skin, role, when)
    if "status" in ins:
        out.append(text("{{level|fixed:0:1000:0}} r/s", ins["status"][0] - 5, ins["status"][2], 16, "ink"))
    return out


# ---- menus ----

MENU_ROW = 40


def menus(skin):
    """Every drop-down: the skin menu under the logo and each chooser under its box, as (list, chosen, box, row)."""
    x0, y0, _, y1 = skin["menu"]
    out = [("skins", "skin", (x0, y0, x0 + 210, y1), MENU_ROW)]
    return out + [(lst, name, box, 30) for name, (_, lst, _, box) in skin.get("choosers", {}).items()]


def menu_shapes(lst, chosen_, box, row):
    x0, _, x1, y1 = box
    open_ = {"state": "menu", "in": [lst]}
    top, w = y1 + 4, x1 - x0
    rows = len(choices(lst))
    middle = top + 4 + row / 2
    return [
        shape("rounded", open_, x=x0, y=top, w=w, h=rows * row + 8, radius=10, colour="@panel", depth=8),
        shape("rounded", open_, x=x0, y=top, w=w, h=rows * row + 8, radius=10, stroke=1, colour="@border", alpha=0.5, depth=8),
        {**text("{{item.label}}", x0 + 18, middle, 15, "ink", when=open_), "each": {"in": lst, "dy": row}, "depth": 9},
        {**text("✓", x1 - 20, middle, 15, "accent", 2), "each": {"in": lst, "dy": row},
         "when": {"all": [open_, {"input": chosen_, "eq": "{{item.key}}"}]}, "depth": 9},
    ]


def choices(lst):
    return {"skins": SKINS, "rates": RATES, "waves": WAVES}[lst]


# ---- listeners ----

def area(rect, role, label, on_, when=None, **extra):
    x0, y0, x1, y1 = rect
    out = {"x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0, "role": role, "label": label, **extra}
    if on_:
        out["on"] = on_
    return {**out, "when": when} if when else out


def switch(name, role, rect, prefix="", when=None):
    """The two listeners of a switch, each labelled by what pressing it does: bypass the node while on, enable it while off."""
    flag = f"{role}On"
    return [area(rect, "button", f"{prefix}Bypass {name}", {"set": flag, "value": False, "fire": "changed"}, both(when, on(role, True))),
            area(rect, "button", f"{prefix}Enable {name}", {"set": flag, "value": True, "fire": "changed"}, both(when, on(role, False)))]


def toggle_rect(cx, cy, w, h):
    return (cx - w / 2 - 4, cy - h / 2 - 4, cx + w / 2 + 4, cy + h / 2 + 4)


def slide(role, label, x0, x1, cy, when=None):
    return area((x0 - 10, cy - 12, x1 + 10, cy + 12), "slider", label, None, when, slide={"set": role, "min": 0, "max": 1, "fire": "changed", "of": role})


def menu_items(lst, chosen_, box, row):
    x0, _, x1, y1 = box
    return {**area((x0, y1 + 8, x1, y1 + 8 + row), "menuitem", "{{item.label}}", {"set": chosen_, "value": "{{item.key}}", "fire": chooser(lst, "choose")},
                   {"state": "menu", "in": [lst]}), "each": {"in": lst, "dy": row}}


def chooser(lst, verb):
    """The trigger that opens (`toggle`) or settles (`choose`) a list's menu: toggleSkins, chooseSkin, toggleRates, chooseRate."""
    return verb + lst[0].upper() + (lst[1:] if verb == "toggle" else lst[1:-1])


def header_listeners(skin):
    """The menus, choosers, the dismissing backdrop and the history buttons."""
    out = [menu_items(*m) for m in menus(skin)]
    out.append(area(skin["menu"], "popup", f"Skin: {skin['name']}", {"fire": "toggleSkins"}))
    for name, (label, lst, _, box) in skin.get("choosers", {}).items():
        out.append(area(box, "popup", f"{label}: {{{{{name}}}}}", {"fire": chooser(lst, "toggle")}))
    out += [area((0, 0, 1536, skin["height"]), "none", "", {"fire": "dismiss"}, {"not": {"state": "menu", "in": ["closed"]}}),
            area(skin["undo"], "button", "Undo", {"fire": "undo"}, {"input": "canUndo", "eq": True})]
    if skin["redo"]:
        out.append(area(skin["redo"], "button", "Redo", {"fire": "redo"}, {"input": "canRedo", "eq": True}))
    return out


def inspector_listeners(skin, role, n):
    ins = skin["inspector"]
    out = []
    for key in ("toggle", "enabled", "bypass"):
        if key in ins:
            prefix = "Inspector: " if key == "toggle" else f"Inspector {key}: "
            out += switch(n["name"], role, toggle_rect(*ins[key]), prefix, chosen(role))
    if role in skin["params"]:
        out.append(slide(bound(skin, role), f"Inspector: {skin['params'][role][0]}", *ins["slider"], chosen(role)))
    return out


def listeners(skin):
    out = header_listeners(skin)
    names = dict(zip(ROLES, (n["name"] for n in skin["nodes"])))
    for role, n in zip(ROLES, skin["nodes"]):
        out += switch(n["name"], role, toggle_rect(*n["toggle"]))
    for role, sx0, sx1, cy, _, _ in skin["sliders"]:
        out.append(slide(bound(skin, role), f"{names[role]} {skin['params'][role][0]}", sx0, sx1, cy))
    for role, n in zip(ROLES, skin["nodes"]):
        out += inspector_listeners(skin, role, n)
    picks = [area(n["rect"], "button", f"Select {n['name']}", {"set": "selected", "value": role}) for role, n in zip(ROLES, skin["nodes"])]
    if "cables" not in skin:
        return out + picks
    return [drag.followed(skin, a) for a in out] + [drag.followed(skin, drag.grabbed(a, role)) for a, role in zip(picks, ROLES)]


def canvas(key, skin):
    atlas = [f"{{{{assets}}}}/skins/{key}/backdrop.png"] + [f"{{{{assets}}}}/skins/{key}/icon-{role}.png" for role in ROLES]
    backdrop = shape("sprite", index=0, x=768, y=skin["height"] / 2, w=1536, h=skin["height"], depth=0)
    shapes = node_shapes(key, skin) + traces(skin) + meters(skin) + inspector(key, skin)
    shapes += [s for m in menus(skin) for s in menu_shapes(*m)]
    found = listeners(skin)
    height = skin["height"]
    if "cables" in skin:
        atlas += [f"{{{{assets}}}}/skins/{key}/node-{role}.png" for role in ROLES]
        shapes = drag.cables(skin) + drag.badge(skin) + drag.nodes(skin, 1 + len(ROLES)) + [drag.followed(skin, s) for s in shapes]
    if skin.get("keys"):
        shapes, found, height = shapes + keys.shapes(1536), keys.listeners() + found, keys.height(height)
    shapes = [backdrop] + shapes
    return {"width": 1536, "height": height, "zoom": ZOOM, "atlas": atlas, "shapes": shapes, "listeners": found}


def main():
    doc = json.loads(DOCUMENT.read_text())
    for key, skin in SKINS.items():
        slice_skin(key, skin)
    logo()
    doc["inputs"].update({**drag.offsets(), **keys.inputs()})
    doc["layers"] = [layer for layer in doc["layers"] if layer["name"] != "keys"] + [keys.layer()]
    doc["skins"] = {key: {"name": skin["name"], "motion": "none", "tokens": {**skin["tokens"], "background": skin["tokens"]["canvas"]}, "canvas": canvas(key, skin)}
                    for key, skin in SKINS.items()}
    DOCUMENT.write_text(written(doc) + "\n")


def written(doc):
    """The document indented, with each canvas shape and listener on one line so diffs stay readable."""
    lines = {}
    for skin in doc["skins"].values():
        for key in ("shapes", "listeners"):
            items = skin["canvas"][key]
            skin["canvas"][key] = f"@{len(lines)}"
            lines[skin["canvas"][key]] = "[\n" + ",\n".join(json.dumps(item, ensure_ascii=False) for item in items) + "\n]"
    text = json.dumps(doc, indent=1, ensure_ascii=False)
    for marker, block in lines.items():
        text = text.replace(f'"{marker}"', block)
    return text


if __name__ == "__main__":
    main()
