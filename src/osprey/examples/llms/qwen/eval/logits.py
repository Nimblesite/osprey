#!/usr/bin/env python3
"""llama.cpp top-20 logits driver — plan 0049 Phase 4 "Compare logits with llama.cpp", spec [LLM-LOGITS-LLAMA].

On the same GGUF over ~50 prompts up to 32K tokens, Osprey and llama.cpp must
pick the same top-1 token every time and agree on the top-20 logits within a
tolerance fixed from Phase 3. `prompts` builds prompts from the repo corpus,
`llama` records llama.cpp's top-20 next-token logprobs, `compare` diffs them
against Osprey's raw logits, `selftest` runs all three against a fake
llama.cpp server. Python 3 stdlib only; run with `python3 -I`.
"""
import argparse
import http.server
import json
import os
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

EXTS = (".osp", ".ospml", ".py", ".rs", ".c", ".h", ".ts", ".md")
MAX_FILE = 400 * 1024
CORPUS_LIMIT = 200_000
TIMEOUT = 120

def post_json(server, path, payload, timeout=TIMEOUT):
    req = urllib.request.Request(server.rstrip("/") + path,
                                 data=json.dumps(payload).encode(),
                                 headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read().decode())

def read_jsonl(path):
    with open(path, encoding="utf-8") as fh:
        return [json.loads(line) for line in fh if line.strip()]

def write_jsonl(path, records):
    with open(path, "w", encoding="utf-8") as fh:
        for rec in records:
            fh.write(json.dumps(rec) + "\n")

def repo_files():
    proc = subprocess.run(["git", "ls-files"], capture_output=True, text=True)
    if proc.returncode != 0:
        return []
    return [p for p in sorted(proc.stdout.splitlines()) if p.endswith(EXTS)
            and os.path.isfile(p) and os.path.getsize(p) <= MAX_FILE]

def build_corpus(files, start, limit):
    parts, total, count = [], 0, len(files)
    for k in range(count):
        path = files[(start + k) % count]
        try:
            with open(path, encoding="utf-8", errors="replace") as fh:
                body = fh.read()
        except OSError:
            continue
        part = f"\n\n// file: {path}\n{body}"
        parts.append(part)
        total += len(part)
        if total > limit:
            break
    return "".join(parts)

def target_lengths(count, lo, hi):
    if count == 1:
        return [hi]
    ratio = (hi / lo) ** (1.0 / (count - 1))
    out = []
    for i in range(count):
        val = int(round(lo * ratio ** i))
        if out and val <= out[-1]:
            val = out[-1] + 1
        out.append(val)
    return out

def tokenize(server, text):
    return post_json(server, "/tokenize",
                     {"content": text, "add_special": False})["tokens"]

def make_prompt(server, corpus, want):
    ids = tokenize(server, corpus)[:want]
    text = post_json(server, "/detokenize", {"tokens": ids})["content"]
    redone = tokenize(server, text)
    if redone != ids:
        return text, redone, True
    return text, ids, False

def cmd_prompts(args):
    files = repo_files()
    if not files:
        print("error: git ls-files found no usable source files", file=sys.stderr)
        return 1
    records = []
    for i, want in enumerate(target_lengths(args.count, args.min, args.max)):
        corpus = build_corpus(files, (i * 37) % len(files), CORPUS_LIMIT)
        text, ids, redone = make_prompt(args.server, corpus, want)
        records.append({"index": i, "tokens": len(ids), "text": text,
                        "ids": ids, "retokenized": redone})
    write_jsonl(args.out, records)
    return 0

def top20(server, ids):
    payload = {"prompt": ids, "n_predict": 1, "n_probs": 20,
               "temperature": 0, "cache_prompt": False,
               "post_sampling_probs": False, "samplers": []}
    probs = post_json(server, "/completion", payload, 3600)
    top = [(e["id"], e["logprob"])
           for e in probs["completion_probabilities"][0]["top_logprobs"]]
    return sorted(top, key=lambda e: e[1], reverse=True)

def cmd_llama(args):
    records = []
    for p in read_jsonl(args.prompts):
        t0 = time.monotonic()
        top = top20(args.server, p["ids"])
        records.append({"index": p["index"], "tokens": p["tokens"],
                        "top": [[tid, lp] for tid, lp in top]})
        print(f"prompt {p['index']}: {p['tokens']} tokens in "
              f"{time.monotonic() - t0:.1f}s", file=sys.stderr)
    write_jsonl(args.out, records)
    return 0

def prompt_errors(llama_rec, osp_rec):
    top = llama_rec["top"]
    logits = {int(k): v for k, v in osp_rec["logits"].items()}
    top1_id = top[0][0]
    match = osp_rec.get("best") == top1_id
    errs = {}
    if top1_id in logits:
        for tid, lp in top:
            if tid in logits:
                errs[tid] = abs((lp - top[0][1])
                                - (logits[tid] - logits[top1_id]))
    return match, errs

def report_missing(llama, osp):
    ok = True
    for side, keys in (("osprey", set(llama) - set(osp)),
                       ("llama", set(osp) - set(llama))):
        if keys:
            ok = False
            print(f"missing indices on the {side} side: {sorted(keys)}")
    return ok

def compare_records(llama, osp, tol, ok):
    compared = matches = 0
    max_all = max5 = 0.0
    for idx in sorted(set(llama) & set(osp)):
        match, errs = prompt_errors(llama[idx], osp[idx])
        compared, matches = compared + 1, matches + match
        max_all = max([max_all] + list(errs.values()))
        top_ids = [tid for tid, _ in llama[idx]["top"]]
        max5 = max([max5] + [errs[t] for t in top_ids[:5] if t in errs])
        ok = ok and match and all(e <= tol for e in errs.values())
        print(f"index {idx}: tokens={llama[idx]['tokens']} "
              f"top1_match={match} max_err={max(errs.values(), default=0.0):.4f}")
    return (compared, matches, max_all, max5), ok

def cmd_compare(args):
    llama = {r["index"]: r for r in read_jsonl(args.llama)}
    osp = {r["index"]: r for r in read_jsonl(args.osprey)}
    ok = report_missing(llama, osp)
    (compared, matches, max_all, max5), ok = compare_records(
        llama, osp, args.tolerance, ok)
    print(f"compared={compared} top1_matches={matches} max_err={max_all:.4f} "
          f"max_err_top5={max5:.4f} tolerance={args.tolerance}")
    print("PASS" if ok else "FAIL")
    return 0 if ok else 1

def handle_fake(path, body):
    if path == "/tokenize":
        return {"tokens": [ord(c) for c in body["content"]]}
    if path == "/detokenize":
        return {"content": "".join(chr(i) for i in body["tokens"])}
    base = body["prompt"][-1]
    tops = [{"id": (base + k) % 0x110000, "token": "?", "logprob": -0.05 * k}
            for k in range(20)]
    return {"completion_probabilities": [{"content": "?", "top_logprobs": tops}]}

class FakeLlamaHandler(http.server.BaseHTTPRequestHandler):
    def do_POST(self):
        length = int(self.headers.get("Content-Length", 0))
        body = json.loads(self.rfile.read(length))
        data = json.dumps(handle_fake(self.path, body)).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, fmt, *args):
        pass

def start_fake():
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), FakeLlamaHandler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv

def osprey_files(llama_path, tmp):
    good, bad = [], []
    for r in read_jsonl(llama_path):
        logits = {str(tid): lp + 3.25 for tid, lp in r["top"]}
        base = {"tokens": r["tokens"], "logits": logits}
        good.append({**base, "index": r["index"], "best": r["top"][0][0]})
        bad.append({**base, "index": r["index"], "best": r["top"][1][0]})
    good_path = os.path.join(tmp, "osprey-good.jsonl")
    bad_path = os.path.join(tmp, "osprey-bad.jsonl")
    write_jsonl(good_path, good)
    write_jsonl(bad_path, bad)
    return good_path, bad_path

def run_selftest(server, tmp):
    prom = os.path.join(tmp, "prompts.jsonl")
    lla = os.path.join(tmp, "llama.jsonl")
    assert cmd_prompts(argparse.Namespace(server=server, out=prom, count=3,
                                          min=8, max=40)) == 0
    assert cmd_llama(argparse.Namespace(server=server, prompts=prom,
                                        out=lla)) == 0
    good, bad = osprey_files(lla, tmp)
    passed = cmd_compare(argparse.Namespace(llama=lla, osprey=good,
                                            tolerance=0.5))
    failed = cmd_compare(argparse.Namespace(llama=lla, osprey=bad,
                                            tolerance=0.5))
    assert (passed, failed) == (0, 1), (passed, failed)
    return 0

def selftest(_args):
    global CORPUS_LIMIT
    saved = CORPUS_LIMIT
    srv = start_fake()
    CORPUS_LIMIT = 4000
    try:
        server = f"http://127.0.0.1:{srv.server_address[1]}"
        code = run_selftest(server, tempfile.mkdtemp(prefix="logits-selftest-"))
    finally:
        CORPUS_LIMIT = saved
        srv.shutdown()
    print("selftest ok" if code == 0 else "selftest FAILED")
    return code

def add_prompts(sub):
    sp = sub.add_parser("prompts", help="build prompts from the repo corpus")
    sp.add_argument("--server", required=True)
    sp.add_argument("--out", required=True)
    sp.add_argument("--count", type=int, default=50)
    sp.add_argument("--max", type=int, default=32000)
    sp.add_argument("--min", type=int, default=8)
    sp.set_defaults(func=cmd_prompts)

def add_llama(sub):
    sl = sub.add_parser("llama", help="record llama.cpp top-20 logprobs")
    sl.add_argument("--server", required=True)
    sl.add_argument("--prompts", required=True)
    sl.add_argument("--out", required=True)
    sl.set_defaults(func=cmd_llama)

def add_compare(sub):
    sc = sub.add_parser("compare", help="diff llama.cpp logprobs vs Osprey")
    sc.add_argument("--llama", required=True)
    sc.add_argument("--osprey", required=True)
    sc.add_argument("--tolerance", type=float, default=0.5)
    sc.set_defaults(func=cmd_compare)

def build_parser():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    add_prompts(sub)
    add_llama(sub)
    add_compare(sub)
    st = sub.add_parser("selftest", help="end-to-end test on a fake server")
    st.set_defaults(func=selftest)
    return parser

def main(argv=None):
    args = build_parser().parse_args(argv)
    return args.func(args)

if __name__ == "__main__":
    sys.exit(main())
