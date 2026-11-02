"""The Prism skin's keyboard deck ([UI-MACHINE-FLOW-SKINS], [UI-MACHINE-FLOW-SYNTH]).

Two octaves of piano keys under the graph, shown only while the `keys` input is 1, which an
application with a voice sets: the bare editor stays 828 tall. Pressing a key sets `note` to its name
and fires `strike`, and the `keys` layer asks `synth.strike` for it, so only keys make sound; the
controls above shape it. The struck key stays lit and the readout shows its note and pitch.
"""
from traces import shape

TOP = 828
DECK = 180
NAMES = ["C", "D", "E", "F", "G", "A", "B"]
SHARPS = {"C", "D", "F", "G", "A"}
OCTAVES = (3, 4)
LEFT, WHITE, GAP, Y, TALL = 32, 60, 4, 866, 124
BLACK, SHORT = 38, 78
SHOWN = {"input": "keys", "eq": 1}


def height(base):
    """The canvas height: the graph alone, or the graph and the deck while keys are shown."""
    return f"{{{{keys|scale:{DECK}:{base}}}}}"


def inputs():
    return {"keys": {"type": "number", "value": 0}, "note": {"type": "text", "value": "A3"},
            "hertz": {"type": "number", "value": 220}, "strike": {"type": "trigger"}}


def layer():
    """Entering `struck` asks the application to play `note`; the layer rests again at once."""
    return {"name": "keys", "initial": "resting",
            "states": {"resting": {"enter": []}, "struck": {"enter": [{"request": "synth.strike", "with": ["note"]}]}},
            "transitions": [{"from": "*", "to": "struck", "when": {"fired": "strike"}}, {"from": "struck", "to": "resting"}]}


def whites():
    """Each white key as (name, rect)."""
    out = []
    for octave in OCTAVES:
        for name in NAMES:
            x = LEFT + len(out) * (WHITE + GAP)
            out.append((f"{name}{octave}", (x, Y, x + WHITE, Y + TALL)))
    return out


def blacks():
    """Each black key as (name, rect), centred on the gap after its white key."""
    out = []
    for name, (_, _, x1, _) in whites():
        if name[0] in SHARPS:
            cx = x1 + GAP / 2
            out.append((f"{name[0]}#{name[1:]}", (cx - BLACK / 2, Y, cx + BLACK / 2, Y + SHORT)))
    return out


def struck(name):
    return {"all": [SHOWN, {"input": "note", "eq": name}]}


def key(name, rect, fill, depth):
    x0, y0, x1, y1 = rect
    box = dict(x=x0, y=y0, w=x1 - x0, h=y1 - y0, radius=6, depth=depth)
    return [shape("rounded", SHOWN, **box, colour=fill),
            shape("rounded", struck(name), **box, colour="@accent"),
            shape("rounded", struck(name), **{**box, "x": x0 - 3, "y": y0 - 3, "w": x1 - x0 + 6, "h": y1 - y0 + 6, "radius": 9},
                  stroke=4, colour="@accent", alpha=0.35),
            shape("rounded", SHOWN, **box, stroke=1, colour="@border")]


def label(words, x, y, size, colour, weight=400, anchor=0):
    return shape("text", SHOWN, text=words, x=x, y=y, size=size, colour=f"@{colour}", anchor=anchor, weight=weight, depth=6)


def readout():
    """What the deck says: what it is, what is playing, and that only keys play."""
    return [label("KEYBOARD", LEFT, 840, 12, "muted", 600), label("NOW PLAYING", 980, 872, 12, "muted", 600),
            label("{{note}}", 980, 892, 44, "ink", 700), label("{{hertz|fixed:1:1:0}} Hz", 980, 950, 18, "accent", 600),
            label("Click a key to play the patch.", 1180, 880, 17, "ink", 600),
            label("Switches, sliders and menus", 1180, 912, 14, "muted"), label("shape the sound; only keys play it.", 1180, 934, 14, "muted")]


def shapes(width):
    deck = [shape("rounded", SHOWN, x=0, y=TOP, w=width, h=DECK, radius=0, colour="@panel", depth=1),
            shape("rounded", SHOWN, x=0, y=TOP, w=width, h=1, radius=0, colour="@line", depth=1)]
    keys = [s for name, rect in whites() for s in key(name, rect, "@keyWhite", 5)]
    keys += [label(name, (r[0] + r[2]) / 2, r[3] - 22, 12, "keyInk", 600, 1) for name, r in whites()]
    keys += [s for name, rect in blacks() for s in key(name, rect, "@keyBlack", 7)]
    return deck + keys + readout()


def listeners():
    """Black keys first, since they sit over the white ones."""
    area = lambda name, r: {"x": r[0], "y": r[1], "w": r[2] - r[0], "h": r[3] - r[1], "role": "button", "label": f"Play {name}",
                            "on": {"set": "note", "value": name, "fire": "strike"}, "when": SHOWN}
    return [area(name, r) for name, r in blacks() + whites()]
