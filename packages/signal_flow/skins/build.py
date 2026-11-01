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

from spec import ROLES, SKINS

HERE = Path(__file__).resolve().parent
DESIGNS = HERE.parent / "design"
DOCUMENT = HERE.parent / "signal_flow.machine.json"
SUPER = 4


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
    Image.fromarray(np.clip(px, 0, 255).astype(np.uint8)[:skin["height"]]).save(folder / "backdrop.png", optimize=True)


# ---- shapes ----

def on(role, flag=True):
    return {"input": f"{role}On", "eq": flag}


def chosen(role):
    return {"input": "selected", "eq": role}


def both(*conditions):
    live = [c for c in conditions if c]
    return live[0] if len(live) == 1 else {"all": live}


def shape(kind, when=None, **members):
    out = {"shape": kind, **members}
    return {**out, "when": when} if when else out


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


def node_shapes(key, skin):
    out = []
    for role, n in zip(ROLES, skin["nodes"]):
        x0, y0, x1, y1 = n["rect"]
        rect = dict(x=x0, y=y0, w=x1 - x0, h=y1 - y0, radius=skin["radius"])
        if skin.get("glow"):
            out.append(shape("rounded", chosen(role), **rect, stroke=8, colour="@accent", alpha=0.22, depth=2))
        out.append(shape("rounded", chosen(role), **rect, stroke=skin["ring"], colour="@accent", depth=2))
        out.append(shape("rounded", on(role, False), **rect, colour="@canvas", alpha=0.6, depth=3))
        out += toggle(skin, f"{role}On", *n["toggle"])
        out += ports(skin, n)
    for name, x0, x1, cy, fill, knob in skin["sliders"]:
        out += slider(key, name, x0, x1, cy, fill, knob)
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
    label, value, note = skin["params"][role]
    vx, vy, vsize, anchor = ins["value"]
    bx0, by0, bx1, by1 = ins["box"]
    box = dict(x=bx0, y=by0, w=bx1 - bx0, h=by1 - by0, radius=6)
    out = [shape("rounded", when, **box, colour="@fill", depth=5), shape("rounded", when, **box, stroke=1, colour="@line", depth=5),
           text(label, lx, ly, lsize, "ink", weight=600 if key == "topology" else 400, when=when),
           text(value, vx, vy, vsize, "ink", anchor, when=when)]
    out += slider(key, role, *ins["slider"], "accent", "accent" if key != "notation" else "knob", when)
    if isinstance(note, tuple):
        out += [text(note[0], ins["slider"][0], ins["range"], 15, "muted", when=when), text(note[1], ins["slider"][1], ins["range"], 15, "muted", 2, when=when)]
    elif note:
        out.append(text(note, 1245, ins["note"][1], ins["note"][2], "muted", when=when))
    return out


def inspector(key, skin):
    ins = skin["inspector"]
    out = []
    for i, (role, n) in enumerate(zip(ROLES, skin["nodes"])):
        when = chosen(role)
        cx, cy, w, h = ins["icon"]
        out.append(shape("sprite", when, index=i + 1, x=cx, y=cy, w=w, h=h, depth=5))
        tx, ty, tsize = ins["title"]
        out.append(text(n["name"], tx, ty, tsize, "ink", weight=700 if key != "notation" else 400, when=when))
        if "kind" in ins:
            out.append(text(n["kind"], ins["kind"][0], ins["kind"][1], ins["kind"][2], "muted", when=when))
        if "label" in ins:
            out.append(text(n["name"], *ins["label"], "ink", when=when))
        out += toggle(skin, f"{role}On", *ins["toggle"], when=when)
        if "enabled" in ins:
            out += toggle(skin, f"{role}On", *ins["enabled"], when=when)
        if "bypass" in ins:
            out += toggle(skin, f"{role}On", *ins["bypass"], when=when, inverse=True)
        out += parameter(key, skin, role, when)
        links = skin["links"][role]
        if "count" in ins:
            out.append(text(f"Connections ({len(links)})", *ins["count"], "ink", weight=600, when=when))
        y0, step = ins["rows"]
        for j, link in enumerate(links):
            out += link_row(key, skin, link, round(y0 + j * step, 1), when)
        if "status" in ins:
            sx, sy, ty2 = ins["status"]
            out += [disc(sx, sy, 6, f"@{{{{{role}Health}}}}", 6, when), text(f"{{{{{role}Health}}}}", sx + 15, sy, 16, "ink", when=when)]
    if "status" in ins:
        out.append(text("{{level|fixed:0:1000:0}} r/s", ins["status"][0] - 5, ins["status"][2], 16, "ink"))
    return out


# ---- the skin menu ----

MENU_ROW = 40


def menu_shapes(skin):
    x0, _, _, y1 = skin["menu"]
    open_ = {"state": "menu", "in": ["skins"]}
    top = y1 + 4
    rows = len(SKINS)
    return [
        shape("rounded", open_, x=x0, y=top, w=210, h=rows * MENU_ROW + 8, radius=10, colour="@panel", depth=8),
        shape("rounded", open_, x=x0, y=top, w=210, h=rows * MENU_ROW + 8, radius=10, stroke=1, colour="@border", alpha=0.5, depth=8),
        {**text("{{item.label}}", x0 + 18, top + 4 + MENU_ROW / 2, 15, "ink", when=open_), "x": x0 + 18, "y": round(top + 4 + MENU_ROW / 2 - 9, 1),
         "each": {"in": "skins", "dy": MENU_ROW}, "depth": 9},
        {**text("✓", x0 + 190, top + 4 + MENU_ROW / 2, 15, "accent", 2), "y": round(top + 4 + MENU_ROW / 2 - 9, 1),
         "each": {"in": "skins", "dy": MENU_ROW}, "when": {"all": [open_, {"input": "skin", "eq": "{{item.key}}"}]}, "depth": 9},
    ]


# ---- listeners ----

def area(rect, role, label, on_, when=None, **extra):
    x0, y0, x1, y1 = rect
    out = {"x": x0, "y": y0, "w": x1 - x0, "h": y1 - y0, "role": role, "label": label, **extra}
    if on_:
        out["on"] = on_
    return {**out, "when": when} if when else out


def switch(name, role, rect, prefix="", when=None, inverse=False):
    """The two listeners of a switch: pressing it while on turns `role`On off, and while off turns it on."""
    flag = f"{role}On"
    lit = "Bypass" if not inverse else "Enable"
    dark = "Enable" if not inverse else "Bypass"
    return [area(rect, "button", f"{prefix}{lit} {name}", {"set": flag, "value": False, "fire": "changed"}, both(when, on(role, True))),
            area(rect, "button", f"{prefix}{dark} {name}", {"set": flag, "value": True, "fire": "changed"}, both(when, on(role, False)))]


def toggle_rect(cx, cy, w, h):
    return (cx - w / 2 - 4, cy - h / 2 - 4, cx + w / 2 + 4, cy + h / 2 + 4)


def slide(role, label, x0, x1, cy, when=None):
    return area((x0 - 10, cy - 12, x1 + 10, cy + 12), "slider", label, None, when, slide={"set": role, "min": 0, "max": 1, "fire": "changed", "of": role})


def listeners(skin):
    x0, _, _, y1 = skin["menu"]
    open_ = {"state": "menu", "in": ["skins"]}
    out = [{**area((x0, y1 + 8, x0 + 210, y1 + 8 + MENU_ROW), "menuitem", "{{item.label}}", {"set": "skin", "value": "{{item.key}}", "fire": "chooseSkin"}, open_),
            "each": {"in": "skins", "dy": MENU_ROW}},
           area(skin["menu"], "popup", f"Skin: {skin['name']}", {"fire": "toggleSkins"}),
           area((0, 0, 1536, skin["height"]), "none", "", {"fire": "dismiss"}, {"not": {"state": "menu", "in": ["closed"]}}),
           area(skin["undo"], "button", "Undo", {"fire": "undo"}, {"input": "canUndo", "eq": True})]
    if skin["redo"]:
        out.append(area(skin["redo"], "button", "Redo", {"fire": "redo"}, {"input": "canRedo", "eq": True}))
    names = dict(zip(ROLES, (n["name"] for n in skin["nodes"])))
    for role, n in zip(ROLES, skin["nodes"]):
        out += switch(n["name"], role, toggle_rect(*n["toggle"]))
    for role, sx0, sx1, cy, _, _ in skin["sliders"]:
        out.append(slide(role, f"{names[role]} {skin['params'][role][0]}", sx0, sx1, cy))
    ins = skin["inspector"]
    for role, n in zip(ROLES, skin["nodes"]):
        for key, inverse in (("toggle", False), ("enabled", False), ("bypass", True)):
            if key in ins:
                prefix = "Inspector: " if key == "toggle" else f"Inspector {key}: "
                out += switch(n["name"], role, toggle_rect(*ins[key]), prefix, chosen(role), inverse)
        if role in skin["params"]:
            out.append(slide(role, f"Inspector: {skin['params'][role][0]}", *ins["slider"], chosen(role)))
    for role, n in zip(ROLES, skin["nodes"]):
        out.append(area(n["rect"], "button", f"Select {n['name']}", {"set": "selected", "value": role}))
    return out


def canvas(key, skin):
    atlas = [f"{{{{assets}}}}/skins/{key}/backdrop.png"] + [f"{{{{assets}}}}/skins/{key}/icon-{role}.png" for role in ROLES]
    backdrop = shape("sprite", index=0, x=768, y=skin["height"] / 2, w=1536, h=skin["height"], depth=0)
    shapes = [backdrop] + node_shapes(key, skin) + inspector(key, skin) + menu_shapes(skin)
    return {"width": 1536, "height": skin["height"], "atlas": atlas, "shapes": shapes, "listeners": listeners(skin)}


def main():
    doc = json.loads(DOCUMENT.read_text())
    for key, skin in SKINS.items():
        slice_skin(key, skin)
    doc["skins"] = {key: {"name": skin["name"], "motion": "none", "tokens": skin["tokens"], "canvas": canvas(key, skin)} for key, skin in SKINS.items()}
    DOCUMENT.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
