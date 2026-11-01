"""The Prism skin's live displays ([UI-MACHINE-FLOW-SKINS]): the LFO, oscillator, filter and scope traces as
canvas polylines and the master's level meters, each bound to the inputs it shows."""
import math

from spec import RATES, WAVES


def shape(kind, when=None, **members):
    """A canvas shape, drawn only while `when` holds when it has one."""
    out = {"shape": kind, **members}
    return {**out, "when": when} if when else out

def polyline(points, colour, when=None, stroke=2.5):
    return shape("polyline", when, points=points, stroke=stroke, colour=f"@{colour}", depth=6)


def wave(name, phase):
    turn = phase % 1.0
    return {"Sine": math.sin(2 * math.pi * turn), "Triangle": 1 - 4 * abs(turn - 0.5), "Saw": 2 * turn - 1,
            "Square": 1.0 if turn < 0.5 else -1.0}[name]


def sampled(x0, x1, step=1.5):
    n = int((x1 - x0) / step)
    return [x0 + (x1 - x0) * i / n for i in range(n + 1)]


def response(u, q=2.0):
    """A resonant two-pole low-pass's gain in decibels at `u` times its cutoff."""
    return -10 * math.log10((1 - u * u) ** 2 + (u / q) ** 2)


def lfo_traces(x0, x1, cy, amp):
    """The LFO's sine at each rate, 1.25 cycles shown per hertz."""
    return [polyline([[round(x, 1), round(cy - amp * math.sin(2 * math.pi * 1.25 * hz * (x - x0) / (x1 - x0)), 1)] for x in sampled(x0, x1)],
                     "mod", {"input": "rate", "eq": rate}) for rate, hz in RATES.items()]


def oscillator_traces(x0, x1, cy, amp):
    """Two and a half cycles of each wave."""
    return [polyline([[round(x, 1), round(cy - amp * wave(name, 2.5 * (x - x0) / (x1 - x0)), 1)] for x in sampled(x0, x1, 1)],
                     "mon", {"input": "wave", "eq": name}) for name in WAVES]


def filter_traces(x0, x1, top, bottom, steps=24):
    """The response over three decades, drawn for each of `steps` cutoff bands, +8 dB at the top to -24 dB at the bottom."""
    out = []
    for k in range(steps):
        cut = x0 + (x1 - x0) * (k + 0.5) / steps
        db = lambda x: max(-24.0, min(8.0, response(10 ** ((x - cut) * 3 / (x1 - x0)))))
        band = {"all": [{"input": "process", "gt": -1 if k == 0 else round(k / steps - 1e-9, 9)},
                        {"input": "process", "lt": 2 if k == steps - 1 else round((k + 1) / steps - 1e-9, 9)}]}
        out.append(polyline([[round(x, 1), round(top + (8 - db(x)) / 32 * (bottom - top), 1)] for x in sampled(x0, x1)], "curve", band))
    return out


def scope_traces(x0, x1, cy, amp):
    """Four cycles of the wave, its height the monitored level."""
    return [polyline([[round(x, 1), f"{{{{monitored|scale:{round(-amp * wave(name, 4 * (x - x0) / (x1 - x0)), 2)}:{cy}}}}}"] for x in sampled(x0, x1, 2)],
                     "mon", {"input": "wave", "eq": name}, 2) for name in WAVES]


def traces(skin):
    """The LFO at its rate, the oscillator's wave, the filter's response at its cutoff and the scope's output."""
    spec = skin.get("traces")
    if not spec:
        return []
    return lfo_traces(*spec["lfo"]) + oscillator_traces(*spec["oscillator"]) + filter_traces(*spec["filter"]) + scope_traces(*spec["scope"])


def meters(skin):
    """Two rows of bars, each lit once the delivered level passes its share, green through amber."""
    spec = skin.get("meters")
    if not spec:
        return []
    ramp = [(0x4F, 0xD1, 0xA0), (0xF5, 0xE0, 0x5A), (0xFB, 0x86, 0x48)]
    out = []
    for i in range(spec["bars"]):
        t = i / (spec["bars"] - 1)
        a, b = (ramp[0], ramp[1]) if t < 0.6 else (ramp[1], ramp[2])
        f = t / 0.6 if t < 0.6 else (t - 0.6) / 0.4
        lit = "#" + "".join(f"{round(a[c] + (b[c] - a[c]) * f):02X}" for c in range(3))
        x = round(spec["x"] + i * spec["step"], 1)
        for cy in spec["rows"]:
            out.append(shape("rounded", x=x, y=cy - 6, w=2.4, h=12, radius=1, colour="@track", depth=5))
            out.append(shape("rounded", {"input": "delivered", "gt": round(i / spec["bars"], 4)}, x=x, y=cy - 6, w=2.4, h=12, radius=1, colour=lit, depth=6))
    return out
