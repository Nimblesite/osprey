#!/usr/bin/env python3
"""Aggregate harness output into the website's benchmark report — generated
MECHANICALLY, never hand-edited.

Reads <out>/raw.jsonl (per case/lang: status + peak RSS), the hyperfine
exports in <out>/hf/<case>.json (per case/lang: timing) and, for a case with no
export on this machine, the timing already published in <out>/results.json,
then writes:

  <out>/results.json — every published cell, the run that measured it, and each
      run's machine and toolchain versions.
  src/toolchain/website/src/_includes/benchmarks-tables.html — the report itself,
      rendered by the /benchmarks page. There is no second, standalone copy.
  src/toolchain/benchmarks/README.md — measured binarytrees figures between generated markers.
"""
import html
import json
import math
import os
import platform
import re
import subprocess
import sys
from datetime import date
from pathlib import Path
from typing import Callable, Optional

from merge_results import TIMING, atomic_write, valid_timing

ORDER: list[str] = ["osprey-arc", "osprey-gc", "rust", "rust-checked", "c", "csharp",
                    "dart", "codon", "ocaml", "haskell", "osprey-wasm", "rust-wasm", "c-wasm"]
LABEL: dict[str, str] = {"osprey-arc": "Osprey (ARC)",
                         "osprey-gc": "Osprey (GC)",
                         "rust": "Rust", "rust-checked": "Rust (checked)", "c": "C",
                         "csharp": "C# (AOT)", "dart": "Dart (AOT)", "codon": "Codon",
                         "ocaml": "OCaml", "haskell": "Haskell",
                         "osprey-wasm": "Osprey (wasm)", "rust-wasm": "Rust (wasm)",
                         "c-wasm": "C (wasm)"}
OSPREY_COLUMNS = ("osprey-arc", "osprey-gc", "osprey-wasm")
REPO = Path(__file__).resolve().parent.parent.parent.parent

Cell = dict[str, object]
Data = dict[str, dict[str, Cell]]


def recorded_results(path: Path) -> Data:
    return json.loads(path.read_text()).get("cases", {}) if path.exists() else {}


def load(out: Path, run: Optional[str] = None) -> Data:
    """Merge raw status/RSS records with timings, keyed by case/lang.

    Start from complete published cells, then overwrite only measured values.
    Missing rows, failed runs and zero RSS samples retain their previous values.
    Hyperfine exports replace timings only when all four fields are valid; a
    replaced cell takes this export's median (or none) and is stamped with `run`.
    """
    data = recorded_results(out / "results.json")
    for line in (out / "raw.jsonl").read_text().splitlines():
        r = json.loads(line)
        cell = data.setdefault(r["case"], {}).setdefault(r["lang"], {})
        if r["status"] == "ok" or cell.get("status") != "ok":
            cell.update(status=r["status"], rss=r["rss"] or cell.get("rss", 0))
    for hf in (out / "hf").glob("*.json"):
        for res in json.loads(hf.read_text())["results"]:
            cell = data.get(hf.stem, {}).get(res.get("command"))
            if cell is not None and cell.get("status") == "ok" and valid_timing(res):
                retime(cell, res, run)
    return {name: {l: c for l, c in case.items() if l in ORDER} for name, case in data.items()}


def retime(cell: Cell, res: dict, run: Optional[str]) -> None:
    cell.update({k: res[k] for k in TIMING})
    cell.pop("median", None)
    if valid_timing({**res, "mean": res.get("median")}):
        cell["median"] = res["median"]
    if run:
        cell["run"] = run


def present_langs(data: Data) -> list[str]:
    seen = {lang for case in data.values() for lang, c in case.items() if c.get("status") == "ok"}
    return [l for l in ORDER if l in seen]


def fmt_time(sec: float) -> str:
    ms = sec * 1000.0
    return f"{ms:.1f}&nbsp;ms" if ms < 1000 else f"{sec:.3f}&nbsp;s"


def fmt_sd(mean: float, sd: float) -> str:
    """The deviation in the mean's own unit, without repeating the unit."""
    return f"{sd * 1000:.1f}" if mean < 1 else f"{sd:.3f}"


def fmt_mem(b: float) -> str:
    mib = b / (1024 * 1024)
    return f"{b / 1024:.0f}&nbsp;KiB" if mib < 1 else f"{mib:.1f}&nbsp;MiB"


def plain(text: str) -> str:
    return text.replace("&nbsp;", " ")


def geomean(xs: list[float]) -> float:
    ys = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in ys) / len(ys)) if ys else float("nan")


def vals(case: dict[str, Cell], key: str) -> dict[str, float]:
    """The numeric `key` (mean / rss) of every language that ran OK in this case.

    A falsy value (rss == 0) means "not measured" — wasm runs under wasmtime, whose
    host RSS isn't comparable — so it is excluded here and rendered as "—"."""
    return {l: float(c[key]) for l, c in case.items()
            if c.get("status") == "ok" and c.get(key)}


def ratios(data: Data, cases: list[str], osprey: str, lang: str, key: str) -> list[float]:
    """`osprey`'s value divided by `lang`'s, for every case both measured."""
    rs: list[float] = []
    for n in cases:
        o, x = data[n].get(osprey, {}), data[n].get(lang, {})
        if o.get("status") == "ok" and x.get("status") == "ok" and o.get(key) and x.get(key):
            rs.append(float(o[key]) / float(x[key]))
    return rs


def wins(data: Data, cases: list[str], key: str) -> list[str]:
    """Cases where one Osprey column is STRICTLY the fastest / lightest of all."""
    return [n for n in cases if osprey_wins(vals(data[n], key))]


def osprey_wins(v: dict[str, float]) -> bool:
    return any(l.startswith("osprey") and all(x < o for k, o in v.items() if k != l) for l, x in v.items())


def cell_missing(c: Cell) -> str:
    return {"build_failed": "build✗", "wrong_output": "wrong✗"}.get(str(c.get("status")), "—")


def table(head: list[str], rows: str, numeric: set[int] = frozenset()) -> str:
    """A `.comparison-table` with `head` columns; `numeric` column indexes align right."""
    cells = "".join(f'<th{" class=\"num\"" if i in numeric else ""}>{h}</th>' for i, h in enumerate(head))
    return (f'<div class="comparison-table"><table><thead><tr>{cells}</tr></thead>'
            f"<tbody>{rows}</tbody></table></div>")


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
        classes.append("win" if lang.startswith("osprey") and strict else "best")
        txt += " ★" if classes[-1] == "win" else ""
    if ok and key == "mean" and c.get("stddev") is not None:
        txt += f'<span class="sd">±{fmt_sd(float(c["mean"]), float(c["stddev"]))}</span>'
    tip = f' title="{html.escape(tooltip(c, key))}"' if ok else ""
    return f'<td class="{" ".join(classes)}"{tip}>{txt}</td>'


def tooltip(c: Cell, key: str) -> str:
    """Every recorded statistic for one cell, and where it was measured."""
    if key == "mean":
        stats = [f"mean {plain(fmt_time(float(c['mean'])))}"] + [
            f"{label} {plain(fmt_time(float(c[k])))}" for k, label in
            (("median", "median"), ("stddev", "standard deviation"), ("min", "min"), ("max", "max"))
            if c.get(k) is not None]
    else:
        stats = [f"peak RSS {plain(fmt_mem(float(c['rss'])))}, highest of three runs"]
    return ", ".join(stats) + f" — measured {c.get('run', 'in an unrecorded run')}"


def summary(data: Data, langs: list[str], cases: list[str]) -> str:
    """Osprey time ÷ each language's time: the geometric mean over shared cases.
    Native languages compare with the native Osprey columns, wasm with wasm."""
    columns = [o for o in OSPREY_COLUMNS if o in langs]
    head = "<tr><th>Compared with</th>" + "".join(
        f'<th class="num col-osprey">{LABEL[o]}</th>' for o in columns) + '<th class="num">Cases</th></tr>'
    rows = "".join(_summary_row(data, cases, columns, lang) for lang in langs if not lang.startswith("osprey"))
    fastest = "".join(f'<td class="num col-osprey">{sum(_fastest(data[n], o) for n in cases)}</td>' for o in columns)
    rows += f'<tr><td>Fastest of all</td>{fastest}<td class="num">{len(cases)}</td></tr>'
    return ('<div class="comparison-table"><table><thead>' + head
            + "</thead><tbody>" + rows + "</tbody></table></div>")


def _fastest(case: dict[str, Cell], osprey: str) -> bool:
    v = vals(case, "mean")
    return osprey in v and all(v[osprey] < x for l, x in v.items() if l != osprey)


def _summary_row(data: Data, cases: list[str], columns: list[str], lang: str) -> str:
    cells, counts = "", set()
    for o in columns:
        rs = ratios(data, cases, o, lang, "mean") if o.endswith("wasm") == lang.endswith("wasm") else []
        counts |= {len(rs)} if rs else set()
        cells += f'<td class="num col-osprey">{f"{geomean(rs):.2f}×" if rs else "—"}</td>'
    count = " / ".join(str(n) for n in sorted(counts))
    return f'<tr><td>{LABEL[lang]}</td>{cells}<td class="num">{count}</td></tr>' if counts else ""


def runs_table(runs: dict[str, dict], data: Data, langs: list[str], cases: list[str]) -> str:
    """One row per run: when, on what, with which toolchains, and which cells it measured."""
    rows = ""
    for rid in sorted(runs, key=lambda r: runs[r].get("date", ""), reverse=True):
        run = runs[rid]
        figures = _run_figures(data, cases, langs, rid)
        tools = ", ".join(f"{name} {version}" for name, version in run.get("tools", {}).items())
        note = f'<span class="sd">{html.escape(run["note"])}</span>' if run.get("note") else ""
        rows += "<tr>" + "".join(f"<td>{html.escape(v)}</td>" for v in (
            run.get("date", "—"), run.get("cpu", "—"), run.get("os", "—"))) + (
            f"<td>{html.escape(tools) or '—'}{note}</td><td>{html.escape(figures)}</td></tr>")
    return table(["Measured", "CPU", "OS", "Toolchains", "Figures"], rows)


def _run_figures(data: Data, cases: list[str], langs: list[str], rid: str) -> str:
    """Languages grouped by the cases this run measured for them, e.g.
    `Rust, C: every case except fib, tak` or `Codon: every case`."""
    groups: dict[tuple[str, ...], list[str]] = {}
    for lang in langs:
        hit = tuple(n for n in cases if data[n].get(lang, {}).get("run") == rid)
        if hit:
            groups.setdefault(hit, []).append(LABEL[lang])
    return "; ".join(f"{', '.join(names)}: {_case_list(hit, cases)}" for hit, names in groups.items())


def _case_list(hit: tuple[str, ...], cases: list[str]) -> str:
    rest = [n for n in cases if n not in hit]
    if not rest:
        return "every case"
    return f"every case except {', '.join(rest)}" if len(rest) < len(hit) else ", ".join(hit)


def tables_fragment(data: Data, langs: list[str], cases: list[str], runs: dict[str, dict]) -> str:
    """The whole report body, rendered by the website /benchmarks page."""
    return ("<h2>Osprey time relative to each language</h2>\n" + summary(data, langs, cases)
            + "\n<h2>CPU time</h2>\n" + value_table(data, langs, cases, "mean", fmt_time)
            + "\n<h2>Peak memory</h2>\n" + value_table(data, langs, cases, "rss", fmt_mem)
            + "\n<h2>Where each figure was measured</h2>\n" + runs_table(runs, data, langs, cases))


def bake_website_fragment(fragment: str) -> Optional[Path]:
    """Write the report into the website includes so /benchmarks renders it at
    site-build time without the (gitignored) hyperfine exports present."""
    inc = REPO / "src" / "toolchain" / "website" / "src" / "_includes"
    if not inc.exists():
        return None
    path = inc / "benchmarks-tables.html"
    atomic_write(path, "<!-- Generated by src/toolchain/benchmarks/report.py — do not edit. Run `make -C src/toolchain/benchmarks bench`. -->\n" + fragment + "\n")
    return path


def measured_peak(cell: Cell) -> str:
    """Human-readable decimal peak RSS, e.g. `2.53 GB` or `2.98 MB`.

    `:.3g` alone rendered a gigabyte-scale peak as `2.53e+03 MB` — technically
    right, unreadable in prose, and this string is baked into the README and the
    published website page. The unit is chosen AFTER rounding: 999,999,999
    bytes rounds to 1000 MB, which `:.3g` renders as `1e+03 MB` again.
    """
    mb = float(cell["rss"]) / 1_000_000
    return f"{mb / 1000:.3g} GB" if float(f"{mb:.3g}") >= 1000 else f"{mb:.3g} MB"


README = REPO / "src" / "toolchain" / "benchmarks" / "README.md"


def update_readme(data: Data, path: Path = README) -> Optional[Path]:
    """Refresh measured prose without making the whole hand-written README generated."""
    start = "> <!-- binarytrees-results:start -->"
    end = "> <!-- binarytrees-results:end -->"
    before, marked, after = path.read_text().partition(start)
    _, found, after = after.partition(end)
    if not marked or not found:
        raise ValueError("benchmarks README is missing binarytrees result markers")
    cells = data.get("binarytrees", {})
    if not {"osprey-arc", "osprey-gc"} <= cells.keys():
        return None  # filtered run: nothing measured for the prose figures
    arc, gc = (measured_peak(cells[lang]) for lang in ("osprey-arc", "osprey-gc"))
    generated = (
        f"{start}\n"
        f"> Current measured peaks: `--memory=arc` **{arc}** and `--memory=gc` **{gc}**.\n"
        f"{end}"
    )
    atomic_write(path, before + generated + after)
    return path


TOOL_COMMANDS: dict[str, list[tuple[str, list[str]]]] = {
    "osprey": [("Osprey", ["git", "-C", str(REPO), "rev-parse", "--short", "HEAD"]),
               ("Clang", [os.environ.get("OSPREY_CC", "clang"), "--version"])],
    "rust": [("Rust", ["rustc", "--version"])],
    "c": [("C compiler", ["cc", "--version"])],
    "csharp": [(".NET", ["dotnet", "--version"])],
    "dart": [("Dart", ["dart", "--version"])],
    "codon": [("Codon", ["codon", "--version"])],
    "ocaml": [("OCaml", ["ocamlopt", "-version"])],
    "haskell": [("GHC", ["ghc", "--numeric-version"])],
    "wasm": [("Wasmtime", ["wasmtime", "--version"])],
}


def first_line(command: list[str]) -> str:
    try:
        done = subprocess.run(command, capture_output=True, text=True, timeout=20)
    except (OSError, subprocess.TimeoutExpired):
        return "unknown"
    lines = (done.stdout or done.stderr).strip().splitlines()
    if done.returncode != 0 or not lines:
        return "unknown"
    version = re.search(r"\d+(?:\.\d+)+", lines[0])
    return version.group(0) if version else lines[0].strip()


def host() -> tuple[str, str]:
    """This machine's CPU model and operating system, as the OS reports them."""
    if sys.platform == "darwin":
        return (first_line(["sysctl", "-n", "machdep.cpu.brand_string"]),
                f"macOS {first_line(['sw_vers', '-productVersion'])}")
    info = Path("/proc/cpuinfo")
    lines = info.read_text().splitlines() if info.exists() else []
    cpu = next((l.split(":", 1)[1].strip() for l in lines if l.startswith("model name")), "unknown")
    return cpu, f"{platform.system()} {platform.release()}"


def tool_versions(langs: set[str]) -> dict[str, str]:
    families = {l.split("-")[0] for l in langs} | ({"wasm"} if any(l.endswith("wasm") for l in langs) else set())
    tools = {name: first_line(cmd) for f in TOOL_COMMANDS if f in families for name, cmd in TOOL_COMMANDS[f]}
    return {**tools, "hyperfine": first_line(["hyperfine", "--version"])}


def record_runs(out: Path, data: Data, run: str, cpu: str, os_name: str) -> dict[str, dict]:
    """The published runs still referenced by a cell, plus this one if it measured anything."""
    path = out / "results.json"
    runs = json.loads(path.read_text()).get("runs", {}) if path.exists() else {}
    measured = {l for case in data.values() for l, c in case.items() if c.get("run") == run}
    if measured:
        runs[run] = {"date": run.split(" · ")[0], "cpu": cpu, "os": os_name, "tools": tool_versions(measured)}
    used = {c.get("run") for case in data.values() for c in case.values()}
    return {rid: r for rid, r in runs.items() if rid in used}


def render(out: Path) -> None:
    cpu, os_name = host()
    run = f"{date.today().isoformat()} · {cpu}"
    data = load(out, run)
    langs, cases = present_langs(data), sorted(data)
    runs = record_runs(out, data, run, cpu, os_name)
    atomic_write(out / "results.json", json.dumps({"languages": langs, "runs": runs, "cases": data},
                                                  indent=2, allow_nan=False, ensure_ascii=False))
    baked = bake_website_fragment(tables_fragment(data, langs, cases, runs))
    readme = update_readme(data)
    print(f"wrote {out / 'results.json'}" + (f" and the website report {baked}" if baked else "")
          + (f" and refreshed {readme}" if readme else " (README figures unchanged)"))


if __name__ == "__main__":
    render(Path(sys.argv[1]))
