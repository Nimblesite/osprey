#!/usr/bin/env python3
"""Aggregate harness output into an HTML report — generated MECHANICALLY, never
hand-edited.

Reads <out>/raw.jsonl (per case/lang: status + peak RSS) and the hyperfine
exports in <out>/hf/<case>.json (per case/lang: timing), then renders:

  <out>/results.html  — self-contained report (Osprey website CSS inlined)
  <out>/results.json  — same data, structured, for tracking over time
  website/src/_includes/benchmarks-tables.html — the same tables as a fragment,
      "baked" into the website so the /benchmarks page renders without the
      (gitignored) results present.

The tables (CPU time, peak memory) use the website's own `.comparison-table`
classes; the fastest cell per row is badged and Osprey's outright wins starred.
"""
import html
import json
import math
import sys
from pathlib import Path
from typing import Callable, Optional, cast

ORDER: list[str] = ["osprey", "osprey-arc", "osprey-gc", "rust", "c", "csharp", "dart",
                    "ocaml", "haskell", "osprey-wasm", "rust-wasm", "c-wasm"]
LABEL: dict[str, str] = {"osprey": "Osprey", "osprey-arc": "Osprey (ARC)",
                         "osprey-gc": "Osprey (GC)",
                         "rust": "Rust", "c": "C", "csharp": "C# (AOT)", "dart": "Dart (AOT)",
                         "ocaml": "OCaml", "haskell": "Haskell",
                         "osprey-wasm": "Osprey (wasm)", "rust-wasm": "Rust (wasm)",
                         "c-wasm": "C (wasm)"}
REPO = Path(__file__).resolve().parent.parent

Cell = dict[str, object]
Data = dict[str, dict[str, Cell]]


def load(out: Path) -> Data:
    """Merge raw status/RSS records with hyperfine timings, keyed by case/lang."""
    data: Data = cast(Data, {})
    for line in (out / "raw.jsonl").read_text().splitlines():
        r = json.loads(line)
        data.setdefault(r["case"], {})[r["lang"]] = {"status": r["status"], "rss": r["rss"]}
    for hf in (out / "hf").glob("*.json"):
        for res in json.loads(hf.read_text())["results"]:
            cell = data.setdefault(hf.stem, {}).setdefault(res["command"], {})
            cell.update(mean=res["mean"], stddev=res["stddev"], min=res["min"], max=res["max"])
    return data


def present_langs(data: Data) -> list[str]:
    seen = {lang for case in data.values() for lang, c in case.items() if c.get("status") == "ok"}
    return [l for l in ORDER if l in seen]


def fmt_time(sec: float) -> str:
    ms = sec * 1000.0
    return f"{ms:.1f}&nbsp;ms" if ms < 1000 else f"{sec:.3f}&nbsp;s"


def fmt_mem(b: float) -> str:
    mib = b / (1024 * 1024)
    return f"{b / 1024:.0f}&nbsp;KiB" if mib < 1 else f"{mib:.1f}&nbsp;MiB"


def geomean(xs: list[float]) -> float:
    ys = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in ys) / len(ys)) if ys else float("nan")


def vals(case: dict[str, Cell], key: str) -> dict[str, float]:
    """The numeric `key` (mean / rss) of every language that ran OK in this case.

    A falsy value (rss == 0) means "not measured" — wasm runs under wasmtime, whose
    host RSS isn't comparable — so it is excluded here and rendered as "—"."""
    return {l: float(c[key]) for l, c in case.items()
            if c.get("status") == "ok" and c.get(key)}


def ratios(data: Data, cases: list[str], lang: str, key: str) -> list[float]:
    rs: list[float] = []
    for n in cases:
        o, x = data[n].get("osprey", {}), data[n].get(lang, {})
        if o.get("status") == "ok" and x.get("status") == "ok" and o.get(key) and x.get(key):
            rs.append(float(o[key]) / float(x[key]))
    return rs


def wins(data: Data, cases: list[str], key: str) -> list[str]:
    """Cases where Osprey is STRICTLY the fastest / lightest of all languages."""
    return [n for n in cases
            if (v := vals(data[n], key)) and "osprey" in v
            and all(v["osprey"] < x for l, x in v.items() if l != "osprey")]


def cell_missing(c: Cell) -> str:
    return {"build_failed": "build✗", "wrong_output": "wrong✗"}.get(str(c.get("status")), "—")


def value_table(data: Data, langs: list[str], cases: list[str], key: str, fmt: Callable[[float], str]) -> str:
    """A `.comparison-table` of `key` per case/lang: fastest cell badged, Osprey wins starred."""
    head = "<tr><th>Benchmark</th>" + "".join(
        f'<th class="num{" col-osprey" if l.startswith("osprey") else ""}">{LABEL[l]}</th>' for l in langs) + "</tr>"
    body = "".join(_row(data[n], n, langs, key, fmt) for n in cases)
    return ('<div class="comparison-table"><table><thead>' + head
            + "</thead><tbody>" + body + "</tbody></table></div>")


def _row(case: dict[str, Cell], name: str, langs: list[str], key: str, fmt: Callable[[float], str]) -> str:
    v = vals(case, key)
    best = min(v.values()) if v else None
    return (f"<tr><td>{html.escape(name)}</td>"
            + "".join(_td(case.get(l, {}), l, key, fmt, v, best) for l in langs) + "</tr>")


def _td(c: Cell, lang: str, key: str, fmt: Callable[[float], str],
        v: dict[str, float], best: Optional[float]) -> str:
    ok = c.get("status") == "ok" and bool(c.get(key))
    txt = fmt(float(c[key])) if ok else cell_missing(c)
    classes = ["num"] + (["col-osprey"] if lang.startswith("osprey") else [])
    if ok and best is not None and float(c[key]) == best:
        strict = all(float(c[key]) < o for l, o in v.items() if l != lang)
        if lang == "osprey" and strict:
            classes.append("win")
            txt += " ★"
        else:
            classes.append("best")
    return f'<td class="{" ".join(classes)}">{txt}</td>'


def summary(data: Data, langs: list[str], cases: list[str]) -> str:
    """Headline cards: outright CPU wins + Osprey's CPU geomean vs each language."""
    cw = wins(data, cases, "mean")
    cards = [_card(str(len(cw)), "CPU wins (fastest of all)", "is-accent")]
    for lang in langs:
        if not lang.startswith("osprey") and (rs := ratios(data, cases, lang, "mean")):
            g = geomean(rs)
            cards.append(_card(f"{g:.2f}×", f"CPU vs {LABEL[lang]}", "is-good" if g <= 1.05 else ""))
    note = (f'<p>Osprey is the fastest of all five languages on <strong>'
            f'{html.escape(", ".join(cw)) or "—"}</strong>. Lower is better; ★ marks an Osprey win.</p>')
    return '<div class="bench-summary">' + "".join(cards) + "</div>" + note


def _card(big: str, label: str, kind: str) -> str:
    return (f'<div class="bench-card {kind}"><span class="big">{big}</span>'
            f'<span class="lbl">{html.escape(label)}</span></div>')


def tables_fragment(data: Data, langs: list[str], cases: list[str]) -> str:
    """The shared report body — summary + both tables — with no page shell or CSS.
    Reused verbatim by the standalone file and the website /benchmarks page."""
    return (summary(data, langs, cases)
            + "\n<h2>CPU time</h2>\n" + value_table(data, langs, cases, "mean", fmt_time)
            + "\n<h2>Peak memory</h2>\n" + value_table(data, langs, cases, "rss", fmt_mem))


def website_css() -> str:
    """The actual Osprey website CSS (tokens + base + components), concatenated so
    the standalone report renders identically to the site with no network."""
    css_dir = REPO / "website" / "src" / "css"
    return "\n".join((css_dir / f).read_text() for f in ("variables.css", "base.css", "components.css")
                     if (css_dir / f).exists())


def render_standalone(fragment: str, langs: list[str]) -> str:
    return STANDALONE.format(css=website_css(), langs=html.escape(", ".join(LABEL[l] for l in langs)), body=fragment)


def bake_website_fragment(fragment: str) -> Optional[Path]:
    """Write the tables fragment into the website includes so /benchmarks renders
    it at site-build time without the (gitignored) results present."""
    inc = REPO / "website" / "src" / "_includes"
    if not inc.exists():
        return None
    path = inc / "benchmarks-tables.html"
    path.write_text("<!-- Generated by benchmarks/report.py — do not edit. Run `make bench`. -->\n" + fragment + "\n")
    return path


def render(out: Path) -> None:
    data = load(out)
    langs, cases = present_langs(data), sorted(data)
    fragment = tables_fragment(data, langs, cases)
    (out / "results.html").write_text(render_standalone(fragment, langs))
    (out / "results.json").write_text(json.dumps({"languages": langs, "cases": data}, indent=2))
    (out / "results.md").unlink(missing_ok=True)  # superseded by the HTML report
    baked = bake_website_fragment(fragment)
    print(f"wrote {out / 'results.html'}" + (f" and baked {baked}" if baked else ""))


STANDALONE = """<!doctype html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Osprey benchmarks</title>
<style>{css}</style></head>
<body><main class="container" style="padding-top:3rem;padding-bottom:4rem">
<h1>Osprey cross-language benchmarks</h1>
<p>{langs} — same naive algorithm and parameters in every language, native
release builds, output checked against an integer oracle. CPU = hyperfine
mean&nbsp;±&nbsp;stddev; memory = peak resident set size. Generated mechanically
by <code>benchmarks/report.py</code>; re-run with <code>make bench</code>.</p>
{body}
</main></body></html>
"""


if __name__ == "__main__":
    render(Path(sys.argv[1]))
