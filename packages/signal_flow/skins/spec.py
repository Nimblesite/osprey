"""Every live part of the four signal-flow designs, measured in mock pixels ([UI-MACHINE-FLOW-SKINS]).

The five nodes are always in role order: control, source, process, output, monitor. Rects are
(left, top, right, bottom); toggles and texts are placed by their centre; sliders run from x0 to
x1 along a centre line. `erase` lists the regions build.py paints out of the mock before the canvas
redraws them live. Each skin's connections are (dot token, port, direction, peer, note).
"""

ROLES = ["control", "source", "process", "output", "monitor"]

# The LFO's rates as note lengths at 120 bpm, in hertz, and the oscillator's waves.
RATES = {"1/1": 0.5, "1/2": 1.0, "1/4": 2.0, "1/8": 4.0, "1/16": 8.0}

WAVES = ["Sine", "Triangle", "Saw", "Square"]

HEALTH = {"Healthy": "#0E9F63", "Degraded": "#F59E0B", "Offline": "#E5484D", "Bypassed": "#8A94A6"}


def node(name, kind, rect, toggle, icon, ports):
    return {"name": name, "kind": kind, "rect": rect, "toggle": toggle, "icon": icon, "ports": ports}


TOPOLOGY = {
    "name": "Topology", "design": "topology.png", "height": 793, "radius": 10, "ring": 2.5,
    "tokens": {"canvas": "#EEF4FC", "fill": "#FDFDFE", "panel": "#F6F9FD", "ink": "#1B2333", "muted": "#7D8AA6",
               "accent": "#2F7BFC", "track": "#D5E0EC", "knob": "#FFFFFF", "mod": "#FB9C0A", "sig": "#2F7BFC",
               "mon": "#2F7BFC", "modText": "#C9772F", "border": "#C9D6E6", "port": "#FFFFFF", "line": "#DCE5F0", **HEALTH},
    "port": {"r": 10, "ring": 2, "core": 6},
    "nodes": [
        node("Policy", "Control", (491, 85, 797, 222), (720.5, 111.5, 35, 20), (511, 98, 551, 138), [(642, 213, "mod")]),
        node("Client", "Source", (42, 339, 331, 497), (257, 366, 35, 20), (64, 354, 106, 390), [(331, 417, "sig")]),
        node("Gateway", "Process", (488, 337, 801, 501), (727, 366, 35, 20), (512, 352, 552, 393), [(642, 336, "mod"), (489, 417, "sig"), (801, 417, "sig"), (643, 500, "sig")]),
        node("Service", "Output", (929, 339, 1195, 501), (1123, 366, 35, 20), (952, 352, 990, 390), [(930, 417, "sig")]),
        node("Telemetry", "Monitor", (492, 601, 797, 768), (721, 629, 35, 20), (512, 615, 550, 652), [(643, 599, "sig")]),
    ],
    "sliders": [("control", 513, 701, 185, "mod", "track"), ("process", 571, 708, 441, "accent", "accent")],
    "texts": [
        ("{{control|percent}}%", 745, 178, 15, "ink", 1, 400),
        ("{{control|percent}}%", 679, 269, 14, "modText", 1, 400),
        ("{{process|percent}}%", 751, 443, 15, "ink", 1, 400),
        ("{{level|fixed:0:1000:0}} r/s", 778, 479, 14, "muted", 2, 400),
        ("{{monitored|fixed:1:25:0}} MB/s", 781, 704, 16, "ink", 2, 400),
    ],
    "health": [("source", 70, 472), ("process", 519, 479), ("output", 955, 480), ("monitor", 519, 745)],
    "erase": [(505, 172, 706, 198), (722, 168, 770, 190), (660, 260, 700, 279), (565, 430, 712, 452), (728, 433, 776, 454),
              (727, 470, 786, 489), (60, 462, 135, 483), (509, 469, 590, 490), (945, 470, 1025, 491), (509, 735, 590, 756),
              (700, 693, 786, 715), (1240, 75, 1480, 134), (1460, 160, 1522, 195), (1240, 213, 1527, 306), (1452, 240, 1519, 276),
              (1240, 345, 1527, 596), (1380, 637, 1522, 703)],
    "inspector": {
        "icon": (1263, 100, 40, 41), "title": (1298, 93, 22), "kind": (1298, 121, 15), "toggle": (1490, 177, 48, 26),
        "param": (1245, 229, 16), "slider": (1246, 1441, 260), "value": (1485, 258, 15, 1), "box": (1455, 243, 1515, 273), "note": (1245, 293, 14),
        "count": (1245, 360, 16), "rows": (396, 54), "status": (1395, 651, 689),
    },
    "params": {"control": ("Threshold", "{{control|percent}}%", "Caps the gateway's rate limit"),
               "process": ("Rate limit", "{{process|percent}}%", "Limits requests per client")},
    "links": {
        "control": [("mod", "out", "→", "Gateway.limit", "Modulation")],
        "source": [("sig", "egress", "→", "Gateway.ingress", "Signal")],
        "process": [("mod", "limit", "←", "Policy.out", "Modulation"), ("sig", "ingress", "←", "Client.egress", "Signal"),
                    ("sig", "egress", "→", "Service.ingress", "Signal"), ("sig", "tap", "→", "Telemetry.ingress", "Signal")],
        "output": [("sig", "ingress", "←", "Gateway.egress", "Signal")],
        "monitor": [("sig", "ingress", "←", "Gateway.tap", "Signal")],
    },
    "menu": (20, 8, 225, 52), "undo": (1135, 12, 1170, 48), "redo": (1180, 12, 1215, 48),
}

NOTATION = {
    "name": "Notation", "design": "notation.png", "height": 858, "radius": 8, "ring": 2,
    "tokens": {"canvas": "#F7F4EE", "fill": "#FAF7F1", "panel": "#F8F6EF", "ink": "#15273D", "muted": "#5D6573",
               "accent": "#D9502E", "track": "#D8D8D4", "knob": "#FFFFFF", "mod": "#D34C2A", "sig": "#15273D",
               "mon": "#15273D", "modText": "#D34C2A", "border": "#1E2A3A", "port": "#15273D", "line": "#E2DED6",
               "switch": "#15273D", **HEALTH},
    "port": {"r": 7, "ring": 0, "core": 0},
    "nodes": [
        node("k", "Constant", (483, 103, 722, 257), (652, 126, 34, 18), (495, 112, 526, 140), [(602, 257, "mod")]),
        node("x(t)", "Signal", (62, 375, 304, 541), (237, 398, 34, 18), (75, 384, 109, 412), [(304, 459, "sig")]),
        node("H(s)", "Transfer", (476, 371, 729, 546), (656, 396, 34, 18), (491, 379, 533, 416), [(602, 371, "mod"), (476, 459, "sig"), (729, 459, "sig"), (602, 546, "sig")]),
        node("y(t)", "Signal", (902, 375, 1153, 542), (1084, 399, 34, 18), (915, 385, 951, 413), [(902, 459, "sig")]),
        node("Scope", "Monitor", (479, 644, 726, 827), (657, 671, 34, 18), (494, 655, 530, 688), [(602, 644, "sig")]),
    ],
    "sliders": [],
    "texts": [("{{control|fixed:2}}", 617, 225, 15, "ink", 0, 400), ("{{process|fixed:2}}", 634, 518, 15, "ink", 0, 400)],
    "health": [],
    "erase": [(612, 213, 672, 237), (628, 506, 688, 530), (1245, 125, 1478, 186), (1362, 256, 1502, 283),
              (1348, 300, 1414, 339), (1245, 410, 1345, 441), (1242, 450, 1420, 478), (1416, 440, 1520, 486),
              (1245, 568, 1517, 736)],
    "inspector": {
        "icon": (1274, 155, 50, 48), "title": (1322, 154, 22), "toggle": (1452, 153, 40, 24), "label": (1368, 269, 16),
        "enabled": (1381, 319, 52, 28), "param": (1250, 425, 15), "slider": (1250, 1411, 464), "value": (1441, 461, 15, 0), "box": (1420, 443, 1517, 483),
        "rows": (587, 44), "peer": 1484,
    },
    "params": {"control": ("Value  k", "{{control|fixed:2}}", ""), "process": ("Cutoff  ωc", "{{process|fixed:2}}", "")},
    "links": {
        "control": [("mod", "Output (mod)", "", "H(s)", "")],
        "source": [("sig", "Output (signal)", "", "H(s)", "")],
        "process": [("sig", "Input (signal)", "", "x(t)", ""), ("sig", "Output (signal)", "", "y(t)", ""),
                    ("mod", "Parameter (mod)", "", "k", ""), ("sig", "Monitor (signal)", "", "Scope", "")],
        "output": [("sig", "Input (signal)", "", "H(s)", "")],
        "monitor": [("sig", "Input (signal)", "", "H(s)", "")],
    },
    "menu": (20, 10, 160, 52), "undo": (1027, 12, 1080, 48), "redo": None,
}

PAPER = {
    "name": "Paper", "design": "paper.png", "height": 808, "radius": 10, "ring": 2.5,
    "tokens": {"canvas": "#F8F7F4", "fill": "#FEFCFA", "panel": "#FAF7F5", "ink": "#15171C", "muted": "#5E636E",
               "accent": "#176DFB", "track": "#CBCCCD", "knob": "#FFFFFF", "mod": "#176DFB", "sig": "#176DFB",
               "mon": "#176DFB", "modText": "#176DFB", "border": "#6D6D6D", "port": "#FFFFFF", "line": "#E4E2DE", **HEALTH},
    "port": {"r": 8, "ring": 3, "core": 0},
    "nodes": [
        node("Parameter", "Control", (489, 113, 777, 222), (703, 143, 33, 20), (505, 124, 536, 160), []),
        node("Source", "Source", (57, 365, 345, 514), (274, 393, 33, 20), (72, 374, 114, 413), [(345, 435, "sig")]),
        node("Transform", "Process", (494, 364, 773, 507), (702, 396, 33, 20), (511, 377, 547, 413), [(633, 364, "mod"), (494, 435, "sig"), (773, 435, "sig"), (670, 507, "sig")]),
        node("Output", "Output", (912, 365, 1195, 514), (1122, 394, 33, 20), (929, 374, 969, 413), [(912, 435, "sig")]),
        node("Monitor", "Monitor", (563, 618, 851, 767), (778, 648, 33, 20), (579, 630, 617, 665), [(690, 618, "sig")]),
    ],
    "sliders": [("control", 634, 749, 192, "accent", "accent"), ("process", 634, 749, 445, "accent", "accent")],
    "texts": [("{{control|fixed:2}}", 597, 192, 15, "ink", 1, 400), ("{{control|fixed:2}}", 633, 282, 15, "modText", 1, 400),
              ("{{process|fixed:2}}", 597, 445, 15, "ink", 1, 400)],
    "health": [],
    "erase": [(575, 180, 620, 205), (628, 181, 757, 203), (612, 271, 655, 294), (575, 433, 620, 458), (628, 434, 757, 456),
              (1245, 90, 1470, 136), (1458, 163, 1517, 198), (1245, 225, 1325, 255), (1420, 220, 1516, 259),
              (1244, 260, 1522, 290), (1250, 380, 1515, 553)],
    "inspector": {
        "icon": (1265, 112, 34, 34), "title": (1297, 112, 22), "toggle": (1488, 180, 44, 24), "param": (1252, 240, 16),
        "slider": (1254, 1510, 274), "value": (1505, 239, 16, 2), "box": (1424, 223, 1512, 256), "rows": (397, 46.3), "arrow": 1330, "peer": 1351,
    },
    "params": {"control": ("Value", "{{control|fixed:2}}", ""), "process": ("Gain", "{{process|fixed:2}}", "")},
    "links": {
        "control": [("sig", "Value", "→", "Transform.gain", "")],
        "source": [("sig", "Out", "→", "Transform.in", "")],
        "process": [("sig", "In", "←", "Source.out", ""), ("sig", "Out", "→", "Output.in", ""),
                    ("sig", "Gain", "←", "Parameter.value", ""), ("sig", "Mon", "→", "Monitor.in", "")],
        "output": [("sig", "In", "←", "Transform.out", "")],
        "monitor": [("sig", "In", "←", "Transform.mon", "")],
    },
    "menu": (25, 12, 145, 62), "undo": (1275, 15, 1328, 60), "redo": None,
}

PRISM = {
    "name": "Prism", "design": "prism-audio-modulation.png", "height": 828, "radius": 10, "ring": 2, "glow": True,
    "tokens": {"canvas": "#100D21", "fill": "#181832", "panel": "#111022", "ink": "#F1F0FF", "muted": "#9C99C8",
               "accent": "#FB8648", "track": "#2E2C51", "knob": "#FFE2DF", "mod": "#BB7BF8", "sig": "#FD8146",
               "mon": "#61ECDD", "modText": "#D9B8FF", "border": "#4A4790", "port": "#1A1530", "line": "#25233F",
               "switchOff": "#2A2848", "knobOff": "#C1C0E7", "curve": "#F7BE5C", **HEALTH},
    "port": {"r": 10, "ring": 3, "core": 0},
    "nodes": [
        node("LFO", "Modulator", (482, 90, 728, 232), (668, 111, 34, 18), (494, 98, 527, 125), [(606, 239, "mod")]),
        node("Oscillator", "Source", (38, 354, 313, 523), (251, 377, 34, 18), (51, 362, 89, 392), [(314, 438, "sig")]),
        node("Filter", "Process", (461, 345, 753, 519), (689, 371, 36, 20), (475, 356, 507, 387), [(606, 345, "mod"), (461, 438, "sig"), (753, 438, "sig"), (607, 521, "mon")]),
        node("Master", "Output", (893, 349, 1177, 524), (1115, 372, 34, 18), (907, 358, 941, 389), [(894, 438, "sig")]),
        node("Scope", "Monitor", (482, 618, 736, 772), (674, 640, 34, 18), (495, 628, 531, 657), [(607, 614, "mon")]),
    ],
    "sliders": [("process", 538, 650, 490, "accent", "accent"), ("output", 968, 1075, 487, "accent", "accent")],
    "texts": [("{{process|expo:1:0.02:20}} kHz", 699, 491, 14, "ink", 1, 400),
              ("{{volume|fixed:1:24:-24}} dB", 1124, 496, 14, "ink", 1, 400),
              ("{{rate}}", 582, 208, 14, "ink", 0, 400), ("{{wave}}", 140, 492, 14, "ink", 0, 400),
              ("{{delivered|fixed:1:24:-24}}", 1157, 419, 14, "ink", 2, 400), ("{{delivered|fixed:1:24:-24}}", 1157, 447, 14, "ink", 2, 400)],
    "health": [],
    "erase": [(1094, 485, 1156, 507), (628, 281, 671, 303), (530, 479, 657, 501), (668, 481, 731, 501), (1228, 106, 1524, 172),
              (938, 10, 1082, 56), (1220, 10, 1416, 56), (692, 98, 720, 122), (274, 364, 306, 390), (710, 358, 744, 384),
              (1140, 362, 1168, 384), (695, 628, 727, 652), (499, 135, 713, 183), (578, 198, 640, 218), (60, 404, 295, 465),
              (134, 480, 206, 508), (522, 397, 693, 465), (972, 407, 1162, 459), (958, 472, 1084, 508), (497, 664, 721, 722),
              (496, 728, 732, 766),
              (1462, 178, 1524, 212), (1462, 220, 1524, 254), (1229, 324, 1305, 349), (1392, 318, 1524, 355),
              (1226, 354, 1524, 381), (1226, 386, 1526, 409), (1229, 484, 1500, 619)],
    "inspector": {
        "icon": (1259, 138, 54, 54), "title": (1301, 140, 26), "toggle": (1495, 195, 46, 24), "bypass": (1495, 237, 46, 24),
        "param": (1234, 336, 16), "slider": (1234, 1517, 367), "value": (1472, 336, 15, 1), "box": (1396, 321, 1518, 351), "range": 397,
        "rows": (497, 36), "dir": 1322, "arrow": 1374, "peer": 1400,
    },
    "params": {"control": ("Depth", "+{{control|percent}}%", ("0%", "100%")),
               "process": ("Cutoff", "{{process|expo:1:0.02:20}} kHz", ("20 Hz", "20 kHz")),
               "output": ("Level", "{{volume|fixed:1:24:-24}} dB", ("-24 dB", "0 dB"), "volume")},
    "choosers": {"rate": ("LFO Rate", "rates", "Rates", (571, 194, 715, 222)), "wave": ("Oscillator Wave", "waves", "Waves", (128, 478, 298, 506))},
    "traces": {"lfo": (500, 712, 159, 16), "oscillator": (62, 292, 433, 19), "filter": (524, 691, 400, 465), "scope": (502, 716, 692, 24)},
    "meters": {"x": 976, "step": 4.6, "bars": 30, "rows": (419, 447)},
    "links": {
        "control": [("mod", "CUTOFF", "Out", "Filter", "")],
        "source": [("sig", "AUDIO", "Out", "Filter", "")],
        "process": [("sig", "AUDIO", "In", "Oscillator", ""), ("sig", "AUDIO", "Out", "Master", ""),
                    ("mon", "AUDIO", "Out", "Scope", ""), ("mod", "CUTOFF", "In", "LFO", "")],
        "output": [("sig", "AUDIO", "In", "Filter", "")],
        "monitor": [("mon", "AUDIO", "In", "Filter", "")],
    },
    "menu": (115, 8, 265, 58), "undo": (1108, 15, 1148, 52), "redo": (1160, 15, 1200, 52),
    # The logo and title moved right of the window's traffic lights, as (box, distance).
    "shift": ((0, 2, 240, 62), 100),
    # Dragging: each connection as (from role, port, to role, port, kind); the port labels drawn outside
    # their cards; the cable runs and badge cleared from the graph; the graph's bounds and grid period;
    # and the badge as (cable, (offset from the cable's middle, width, height), text).
    "cables": [("source", 0, "process", 1, "sig"), ("process", 2, "output", 0, "sig"), ("control", 0, "process", 0, "mod"),
               ("process", 3, "monitor", 0, "mon")],
    "labels": {"source": [(318, 410, 366, 430)], "control": [(618, 236, 656, 256)],
               "process": [(620, 326, 678, 344), (618, 522, 668, 542)], "monitor": [(620, 596, 670, 616)]},
    "wires": [(314, 418, 461, 458), (753, 418, 894, 458), (584, 239, 630, 345), (585, 521, 629, 614), (608, 272, 692, 314)],
    "graph": (0, 66, 1209, 789), "grid": (25.43, 25.11),
    "badge": (2, (43, 58, 22), "+{{control|percent}}%"),
}

SKINS = {"topology": TOPOLOGY, "notation": NOTATION, "paper": PAPER, "prism": PRISM}
