#!/usr/bin/env python3
# Deterministically add an osprey-ml twin after every osprey block in the
# website reference docs. Idempotent: never inserts a twin if one already follows.
#
# ML translation rules (verified against the paired tests/flavors/*.ospml corpus):
#   let x = v        -> x = v
#   mut c = 0        -> mut c = 0 ; reassign c = e -> c := e
#   fn f(a, b) = e   -> f (a, b) = e     ; fn f(x) = e -> f x = e
#   fn(x) => e       -> lambda           ; fn(a,b) => e -> lambda tuple
#   f(x)             -> f x              ; f(x, y) -> f (x, y)
#   print("...")     -> print "..."      ; print(x) -> print x
#   match ... { }    -> offside layout with => arms
#   Foo { a: 1 }     -> Foo / a = 1 (offside)
#   extern fn n(p: T) -> R  -> extern n (p : T) -> R
# Type annotations that HM can infer are stripped.
import re
import sys
import pathlib

# Default to website/src/docs relative to THIS script (website/scripts/), so the
# build pipeline can invoke `python3 scripts/add-ml-twins.py` from website/.
_HERE = pathlib.Path(__file__).resolve().parent
DOCS = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else (_HERE.parent / "src" / "docs")

# Hand-authored ML twins for blocks the automatic translator can't safely do
# (multi-construct blocks, brace-body fns, multi-match, comment-embedded records).
# Keyed by path relative to DOCS. These OVERRIDE the auto-translation. Because
# `osprey --docs` regenerates function docs on every build and strips twins, this
# post-processor re-applies both auto and hand twins after generation.
HAND_TWINS = {
    "keywords/fn.md": 'add (a, b) = a + b\n\ngreet name = print ("Hello, " + name)',
    "keywords/match.md": (
        'match value\n    Some x => x\n    None => 0\n\n'
        'match status\n    Active => "User is active"\n    Inactive => "User is inactive"'
    ),
    "keywords/type.md": (
        'type UserId =\n    int\n\n'
        'type Status =\n    Active\n    Inactive\n\n'
        'type User =\n    name : string\n    age : int'
    ),
    "functions/jsonparse.md": (
        'match jsonParse "{\\"name\\": \\"osprey\\"}"\n'
        '    Success value => print "parsed"\n    Error message => print message'
    ),
    "functions/spawnprocess.md": (
        'processEventHandler (processID, eventType, data) =\n'
        '    match eventType\n        1 => print "STDOUT: ${data}"\n'
        '        2 => print "STDERR: ${data}"\n        3 => print "EXIT: ${data}"\n'
        '        _ => print "Unknown event"\n\n'
        'result = spawnProcess ("echo hello", processEventHandler)\n'
        'match result\n    Success value =>\n        exitCode = awaitProcess value\n'
        '        cleanupProcess value\n    Error message => print "Failed"'
    ),
    # spawnProcess is arity-2 (command + mandatory callback) per builtins.rs;
    # a 1-arg call is a type error, so the twin passes the callback `onEvent`.
    "types/processhandle.md": (
        'result = spawnProcess ("echo hello", onEvent)\n'
        'match result\n    Success value =>\n        exitCode = awaitProcess value\n'
        '        cleanupProcess value\n    Error message => print "Process failed"'
    ),
    "types/httpresponse.md": (
        'HttpResponse\n    status = 200\n'
        '    headers = "Content-Type: application/json"\n'
        '    contentType = "application/json"\n    streamFd = -1\n'
        '    isComplete = true\n    partialBody = "{\\"message\\": \\"Hello\\"}"'
    ),
    # `if (c) { e }` block form is Default-only; ML has no `if` — bool `match`.
    "keywords/false.md": (
        'isComplete = false\nmatch isComplete\n'
        '    false => print "Not done yet"\n    true => print "done"'
    ),
    "keywords/true.md": (
        'isReady = true\nmatch isReady\n'
        '    true => print "Ready!"\n    false => print "Wait"'
    ),
    # Elvis `?:` has no ML surface; the AST-equivalent form is a match on Result.
    "functions/randombelow.md": (
        'd =\n    match randomBelow 6\n'
        '        Success value => value  // a fair die face 0..5\n'
        '        Error message => 0'
    ),
    # Default block was an illustrative fragment with `...`; give a real ML match.
    "functions/indexof.md": (
        'match indexOf ("foo=bar", "=")\n'
        '    Success value => print value\n'
        '    Error message => print "not found"'
    ),
    # ML value bindings carry no `Any` annotation (a `value : Any` signature
    # type-errors against the inferred int); the equivalent ML binds directly.
    "types/any.md": 'value : Any\nvalue = 42\n\ntext : Any\ntext = "Hello"',
    "functions/yield.md": 'yield',
    "functions/padend.md": 'padEnd ("7", 3, ".")  // Success { value: "7.." }',
    "functions/padstart.md": 'padStart ("7", 3, "0")  // Success { value: "007" }',
    "functions/repeat.md": 'repeat ("ab", 3)  // Success { value: "ababab" }',
    "functions/replace.md": 'replace ("a-b-c", "-", "_")  // Success { value: "a_b_c" }',
    "functions/substring.md": 'substring ("hello", 1, 4)  // Success { value: "ell" }',
    "functions/lines.md": 'lines "a\\\nb\\\nc"  // ["a","b","c"]',
    # WebSocket builtins return raw handles/statuses; ML uses whitespace application.
    "functions/websocketclose.md": (
        'status = websocketClose wsId\nprint status'
    ),
    "functions/websocketcreateserver.md": (
        'serverId = websocketCreateServer 8080 "127.0.0.1" "/chat"\nprint serverId'
    ),
    "functions/websocketsend.md": (
        'status = websocketSend wsId "Hello, WebSocket!"\nprint status'
    ),
    "functions/websocketserverbroadcast.md": (
        'sent = websocketServerBroadcast serverId "Welcome!"\nprint sent'
    ),
    "functions/websocketserverlisten.md": (
        'status = websocketServerListen serverId\nprint status'
    ),
    # map-literal pages: generator emits `["k" => v]` ([FLAVOR-ML-MAP]); no hand twin.
    # parsefloat/parseint/split: generator comment-preserve handles them; no hand twin.
    # send/recv: generator emits curried `send ch 42` (AST-equal to Default); no hand twin.
    "index.md": (
        'type Shape =\n    Circle\n    Square\n\n'
        'area (s, size) =\n    match s\n'
        '        Circle => size * size * 3\n        Square => size * size\n\n'
        'total = area (Circle, 4) + area (Square, 2)\n'
        'print "total: ${total}"'
    ),
}

# ---- token-level helpers -------------------------------------------------

def strip_ann_in_let(line):
    # `let name: Type = v` -> keep annotation only if it's `Any` (load-bearing) else drop
    m = re.match(r'^(\s*)let\s+(\w+)\s*:\s*(\w+)\s*=\s*(.*)$', line)
    if m:
        indent, name, typ, val = m.groups()
        if typ == "Any":
            return f"{indent}{name} : Any = {translate_expr(val)}"
        return f"{indent}{name} = {translate_expr(val)}"
    m = re.match(r'^(\s*)let\s+(\w+)\s*=\s*(.*)$', line)
    if m:
        indent, name, val = m.groups()
        return f"{indent}{name} = {translate_expr(val)}"
    return None


def split_top_commas(s):
    """Split on commas not inside quotes, parens, brackets, or braces."""
    parts, depth, buf, i = [], 0, [], 0
    quote = None
    while i < len(s):
        c = s[i]
        if quote:
            buf.append(c)
            if c == quote and s[i-1] != "\\":
                quote = None
        elif c in '"\'':
            quote = c; buf.append(c)
        elif c in "([{":
            depth += 1; buf.append(c)
        elif c in ")]}":
            depth -= 1; buf.append(c)
        elif c == "," and depth == 0:
            parts.append("".join(buf)); buf = []
        else:
            buf.append(c)
        i += 1
    parts.append("".join(buf))
    return [p.strip() for p in parts]


def has_toplevel_comma(s):
    return len(split_top_commas(s)) > 1


def _find_match(s, open_idx):
    """Given s[open_idx] == '(', return index of the matching ')'. Quote-aware."""
    depth, i, quote = 0, open_idx, None
    while i < len(s):
        c = s[i]
        if quote:
            if c == quote and s[i-1] != "\\":
                quote = None
        elif c in '"\'':
            quote = c
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1


def _is_atom(s):
    """True if s is a single ML argument atom needing no wrapping parens: a bare
    identifier, number, string literal, or an already-bracketed group. Whitespace
    application binds tighter than operators, so `0 - 5` is NOT an atom -> needs ()."""
    s = s.strip()
    if not s:
        return False
    if re.fullmatch(r'-?\d+(\.\d+)?', s):          # number
        return True
    if re.fullmatch(r'[A-Za-z_]\w*', s):           # identifier
        return True
    if len(s) >= 2 and s[0] in '"\'' and s[-1] == s[0]:  # whole string literal
        return s.count(s[0]) == 2
    if s[0] in "([" and _find_bracket(s, 0) == len(s) - 1:  # whole bracket group
        return True
    return False


def _find_bracket(s, open_idx):
    pairs = {"(": ")", "[": "]", "{": "}"}
    want, depth, i, quote = pairs[s[open_idx]], 0, open_idx, None
    while i < len(s):
        c = s[i]
        if quote:
            if c == quote and s[i-1] != "\\":
                quote = None
        elif c in '"\'':
            quote = c
        elif c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1


def paren_call_to_ws(s):
    """Recursively rewrite `name(args)` calls to ML whitespace/uncurried form.
    name(atom) -> name atom ; name(expr) -> name (expr) ; name(x,y) -> name (x, y) ;
    name() -> name (). `fn(...)` (lambda head) is skipped. Quote- and nesting-aware."""
    res, i = [], 0
    while i < len(s):
        m = re.match(r'([A-Za-z_]\w*)\(', s[i:])
        # only treat as a call if the identifier isn't 'fn' (lambda) and the char
        # before it isn't part of a larger token
        if m and m.group(1) != "fn":
            name = m.group(1)
            open_idx = i + len(name)
            close = _find_match(s, open_idx)
            if close != -1:
                inner = paren_call_to_ws(s[open_idx+1:close])
                inner_s = inner.strip()
                if inner_s == "":
                    res.append(f"{name} ()")               # name() -> name () (unit)
                elif has_toplevel_comma(inner_s):
                    res.append(f"{name} ({inner_s})")      # name (a, b)
                elif _is_atom(inner_s):
                    res.append(f"{name} {inner_s}")        # name x  (single atom)
                else:
                    res.append(f"{name} ({inner_s})")      # name (0 - 5)  (compound)
                i = close + 1
                continue
        # copy char, skipping over quoted strings verbatim
        c = s[i]
        if c in '"\'':
            q = c; res.append(c); i += 1
            while i < len(s):
                res.append(s[i])
                if s[i] == q and s[i-1] != "\\":
                    i += 1; break
                i += 1
            continue
        res.append(c); i += 1
    return "".join(res)


def _lam_head(params):
    params = params.strip()
    if has_toplevel_comma(params):
        return f"\\({', '.join(split_top_commas(params))}) =>"
    return f"\\{params} =>" if params else "\\() =>"


# Channel builtins are curried in ML (whitespace application, no tuple parens):
#   send(ch, v) -> send ch v ; recv(ch) -> recv ch . [FLAVOR-ML-CALL]
CURRIED_BUILTINS = {"send", "recv"}


def map_literal_to_ml(s):
    """Default map literal `{ "k": v, ... }` -> ML `["k" => v, ...]` ([FLAVOR-ML-MAP]).
    Empty `{}` -> `[=>]`. Quote-aware; only fires on brace groups whose top-level
    separators are `key : value` pairs (string/ident keys), never record/block braces."""
    out, i = [], 0
    while i < len(s):
        if s[i] == "{":
            close = _find_brace(s, i)
            if close != -1:
                inner = s[i+1:close].strip()
                if inner == "":
                    out.append("[=>]"); i = close + 1; continue
                pairs = split_top_commas(inner)
                if pairs and all(":" in p and p.lstrip()[:1] in '"\'' for p in pairs):
                    conv = ", ".join(
                        f'{k.strip()} => {translate_expr(v.strip())}'
                        for k, _, v in (p.partition(":") for p in pairs))
                    out.append(f"[{conv}]"); i = close + 1; continue
        out.append(s[i]); i += 1
    return "".join(out)


def _find_brace(s, open_idx):
    depth, i, quote = 0, open_idx, None
    while i < len(s):
        c = s[i]
        if quote:
            if c == quote and s[i-1] != "\\":
                quote = None
        elif c in '"\'':
            quote = c
        elif c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1


def curried_builtin_call(s):
    """send(ch, v) -> send ch v ; recv(ch) -> recv ch. Whitespace-applied, uncurried
    tuple form is invalid for these channel builtins in ML."""
    for name in CURRIED_BUILTINS:
        m = re.search(rf'\b{name}\(', s)
        while m:
            open_idx = m.end() - 1
            close = _find_match(s, open_idx)
            if close == -1:
                break
            args = split_top_commas(s[open_idx+1:close])
            ws = " ".join(translate_expr(a.strip()) for a in args if a.strip())
            s = s[:m.start()] + f"{name} {ws}" + s[close+1:]
            m = re.search(rf'\b{name}\(', s)
    return s


def translate_expr(expr):
    e = expr
    # brace-body lambda: fn(x) { body }  -> \x => body
    def lam_brace(m):
        return f"{_lam_head(m.group(1))} {m.group(2).strip()}"
    e = re.sub(r'fn\(([^)]*)\)\s*\{([^{}]*)\}', lam_brace, e)
    # arrow lambda: fn(x) => e
    e = re.sub(r'fn\(([^)]*)\)\s*=>', lambda m: _lam_head(m.group(1)), e)
    e = map_literal_to_ml(e)
    e = curried_builtin_call(e)
    e = paren_call_to_ws(e)
    return e


# ---- line-block translation ---------------------------------------------

def translate_records(code):
    """Convert record construction `Name { f: v, ... }` and record type decls
    `type Name = { f: T, ... }` to offside ML layout:
        Name              |   type Name =
            f = v         |       f : T
    Non-record content is preserved. Single-line and multi-line handled."""
    # `type Name = { body }`  -> keep header, fields use `:`
    def repl_type(m):
        header, body = m.group(1), m.group(2)
        fields = [f.strip() for f in split_top_commas(body.strip()) if f.strip()]
        rows = []
        for fld in fields:
            key, _, typ = fld.partition(":")
            rows.append(f"    {key.strip()} : {typ.strip()}")
        return f"{header}\n" + "\n".join(rows)

    out = re.sub(r'(type\s+[A-Za-z_][\w<>]*\s*=)\s*\{(.*?)\}', repl_type, code, flags=re.S)

    # bare construction `Name { body }` (not preceded by `type ... =`) -> fields use `=`
    def repl_ctor(m):
        name, body = m.group(1), m.group(2)
        fields = [f.strip() for f in split_top_commas(body.strip()) if f.strip()]
        rows = []
        for fld in fields:
            key, _, val = fld.partition(":")
            rows.append(f"    {key.strip()} = {translate_expr(val.strip())}")
        return f"{name}\n" + "\n".join(rows)

    out = re.sub(r'\b([A-Z][\w<>]*)\s*\{(.*?)\}', repl_ctor, out, flags=re.S)
    return out


HARD = []  # blocks the script refuses -> hand-treatment list


def is_complex(code):
    """True if the block has a shape the line/record/match rules can't safely do."""
    if code.count("match ") + code.count("match\n") > 1:
        return True  # multiple match blocks
    # fn with brace body spanning lines:  fn name(...) {  ... }
    if re.search(r'\bfn\s+\w+\([^)]*\)\s*(?:->\s*\w+\s*)?\{', code):
        return True
    # standalone braced match whose arms our converter can't parse cleanly is caught later
    return False


def translate_block(code):
    """Translate a whole Default osprey code block body to ML. Returns None if the
    block is byte-identical (no surface difference) so caller can skip. Records
    complex blocks in HARD and returns None so they get hand-treatment."""
    if is_complex(code):
        HARD.append(code)
        return None
    lines = code.split("\n")
    # Single braced match -> offside layout.
    if any(re.search(r'\bmatch\b.*\{', l) for l in lines):
        ml = translate_match_block(lines)
        if ml is None:
            HARD.append(code)
            return None
        return "\n".join(ml)
    # Inline record construction / type-record -> offside layout. Only when the
    # record syntax is in real CODE (a `//` comment's braces stay verbatim).
    if re.search(r'[A-Za-z_][\w<>]*\s*\{', _strip_comments(code)) and ":" in _strip_comments(code):
        rec = translate_records(code)
        if rec != code:
            return rec
    out = [translate_line(l) for l in lines]
    ml = "\n".join(out)
    return None if ml == code else ml


def _split_comment(line):
    """Split a line into (code, comment) at the first `//` outside a string."""
    quote, i = None, 0
    while i < len(line):
        c = line[i]
        if quote:
            if c == quote and line[i-1] != "\\":
                quote = None
        elif c in '"\'':
            quote = c
        elif c == "/" and i + 1 < len(line) and line[i+1] == "/":
            return line[:i], line[i:]
        i += 1
    return line, ""


def _strip_comments(code):
    return "\n".join(_split_comment(l)[0] for l in code.split("\n"))


def translate_line(line):
    if line.strip() == "" or line.lstrip().startswith("//"):
        return line
    # trailing comment: transform only the code, keep the comment verbatim
    code, comment = _split_comment(line)
    if comment and code.strip():
        ml_code = translate_line(code.rstrip())
        pad = code[len(code.rstrip()):] or "  "
        return f"{ml_code}{pad}{comment}"
    got = strip_ann_in_let(line)
    if got is not None:
        return got
    # mut c = 0  (keep) ; reassignment  c = c + 1 -> c := c + 1
    m = re.match(r'^(\s*)mut\s+(\w+)\s*=\s*(.*)$', line)
    if m:
        i, n, v = m.groups()
        return f"{i}mut {n} = {translate_expr(v)}"
    # extern fn name(p: T) -> R
    m = re.match(r'^(\s*)extern\s+fn\s+(\w+)\((.*)\)\s*->\s*(.*)$', line)
    if m:
        i, n, params, ret = m.groups()
        p = params.strip()
        if p == "":
            return f"{i}extern {n} -> {ret}"
        parts = [pp.strip().replace(":", " :") for pp in p.split(",")]
        return f"{i}extern {n} ({', '.join(parts)}) -> {ret}"
    # named fn:  fn f(args) = body    or   fn f(args) { body }
    m = re.match(r'^(\s*)fn\s+(\w+)\((.*)\)\s*(?:->\s*\w+\s*)?=\s*(.*)$', line)
    if m:
        i, n, params, body = m.groups()
        p = params.strip()
        # strip param type annotations (HM infers)
        names = [pp.split(":")[0].strip() for pp in p.split(",")] if p else []
        if len(names) == 0:
            head = f"{i}{n} ="
        elif len(names) == 1:
            head = f"{i}{n} {names[0]} ="
        else:
            head = f"{i}{n} ({', '.join(names)}) ="
        return f"{head} {translate_expr(body)}"
    # plain expression / statement line
    return translate_expr(line)


def translate_match_block(lines):
    """Convert a single `match scrut { arm* }` (arms use -> or =>) to offside ML.
    Returns list of ml lines or None if the shape is unrecognised."""
    text = "\n".join(lines)
    m = re.search(r'match\s+(.+?)\s*\{(.*)\}', text, re.S)
    if not m:
        return None
    scrut, body = m.group(1).strip(), m.group(2)
    arms = []
    for raw in body.split("\n"):
        s = raw.strip()
        if not s:
            continue
        a = re.match(r'^(.*?)\s*(?:=>|->)\s*(.*)$', s)
        if not a:
            return None
        pat, res = a.group(1).strip(), a.group(2).strip()
        # constructor payload: Some(x) -> Some x ; None stays
        pat = re.sub(r'(\b[A-Z]\w*)\(([^)]*)\)', lambda mm: f"{mm.group(1)} {mm.group(2)}" if mm.group(2) else mm.group(1), pat)
        # brace payload: Success { value } -> Success value ; fields juxtaposed
        pat = re.sub(r'(\b[A-Z]\w*)\s*\{([^}]*)\}', lambda mm: (f"{mm.group(1)} " + " ".join(f.strip() for f in mm.group(2).split(",") if f.strip())).strip(), pat)
        arms.append(f"    {pat} => {translate_expr(res)}")
    return [f"match {translate_expr(scrut)}"] + arms


# ---- file rewriting ------------------------------------------------------

BLOCK = re.compile(r'```osprey\n(.*?)\n```', re.S)


def strip_twins(txt):
    """Remove any existing osprey-ml blocks so the pass is idempotent."""
    return re.sub(r'\n*```osprey-ml\n.*?\n```', '', txt, flags=re.S)


def process(path, hand_ml=None):
    """Insert an osprey-ml twin after the FIRST osprey block. If hand_ml is given,
    use it verbatim (override); otherwise auto-translate. Idempotent."""
    txt = strip_twins(path.read_text())
    m = BLOCK.search(txt)
    if not m:
        path.write_text(txt)
        return False, False
    ml = hand_ml if hand_ml is not None else translate_block(m.group(1))
    if ml is None:
        path.write_text(txt)  # identical-in-both-flavors block: no twin
        return False, False
    end = m.end()
    new = txt[:end] + f"\n\n```osprey-ml\n{ml}\n```" + txt[end:]
    path.write_text(new)
    return True, False


def main():
    files = sorted(DOCS.rglob("*.md"))
    n = 0
    for f in files:
        source = f.read_text()
        if re.search(r'^mlTwins:\s*false\s*$', source, re.M):
            without_twins = strip_twins(source)
            if without_twins != source:
                f.write_text(without_twins)
            continue
        rel = f.relative_to(DOCS).as_posix()
        hand = HAND_TWINS.get(rel)
        changed, _ = process(f, hand_ml=hand)
        if changed:
            n += 1
    total = sum(1 for f in files if "```osprey-ml" in f.read_text())
    print(f"ML twins present in {total} files (added/refreshed {n}).")


if __name__ == "__main__":
    main()
