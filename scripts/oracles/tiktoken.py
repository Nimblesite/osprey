#!/usr/bin/env python3
"""CPython oracle for @osprey/tiktoken.

Imports the real tiktoken (and its educational module) plus the `regex`
module, computes every case the port's suite asserts, and writes:

  packages/tiktoken/src/tables.py      the Unicode class range tables
  packages/tiktoken/tests/expected.py  the expected values as a constants module

Deterministic: no randomness, fixed corpora. The real gpt2/cl100k_base
vocabularies are downloaded once through tiktoken's own cache (network on the
first run only). Run from the repository root:

    scripts/oracles/tiktoken.py
"""

from __future__ import annotations

import sys
import unicodedata
from pathlib import Path

import regex
import tiktoken
from tiktoken._educational import SimpleBytePairEncoding, bpe_encode, bpe_train

REPO = Path(__file__).resolve().parents[2]
PKG = REPO / "packages" / "tiktoken"

CLASSIC_GPT2_PAT = r"'s|'t|'re|'ve|'m|'ll|'d| ?\p{L}+| ?\p{N}+| ?[^\s\p{L}\p{N}]+|\s+(?!\S)|\s+"
CLASSIC_CL100K_PAT = (
    r"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}{1,3}"
    r"| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+"
)

# The code point blocks the port's class tables cover. Outside them the port
# classifies a code point as punctuation; the suite's corpus stays inside and
# the battery pins the divergence for a few outsiders.
BLOCKS = [
    (0x0000, 0x2FFF),
    (0x3000, 0x33FF),
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xA000, 0xA48C),
    (0xA960, 0xA97F),
    (0xAC00, 0xD7FB),
    (0xF900, 0xFA6D),
    (0xFB00, 0xFB17),
    (0xFB1D, 0xFB4F),
    (0xFB50, 0xFDFF),
    (0xFE70, 0xFEFC),
    (0xFF00, 0xFFEF),
    (0x1D400, 0x1D7FF),
    (0x20000, 0x2A6DF),
    (0x2A700, 0x2CEAF),
    (0x2F800, 0x2FA1D),
]

CORPUS = """The quick brown fox jumps over the lazy dog. The dog barks; the fox runs.
It's a fine day, isn't it? We'll see. They're here and I've got it. He'd know. I'm sure.
She said: "Hello, world!" — and the world said hello back. Don't stop; keep going!
Numbers: 42, 3.14159, 2024-01-15, 7,500,000 and 0x1F plus 100% of nothing.
Tokyo 東京 is huge; 平仮名 and カタカナ mix with kanji. 日本語のテキストです。
Ελληνικά κείμενα και Κυριλλικά: Привет мир, greetings from Αθήνα and Москва.
שלום עולם and مرحبا بالعworld नमस्ते दुनिया สวัสดีชาวโลก 안녕하세요 세계.
Emoji: 🚀🚀🚀 launch! 🌍 fires 😀 and 🎉 parties; also ☕ time. Mixed rocket 🚀fox!
Fullwidth: ＡＢＣ１２３ and ｶﾀｶﾅ. Superscripts x² and y³ plus ½ ¼ ¾ of a cake.
Ideographic space　here and NBSP here and a run of three spaces   done.
Tabs	and	newlines
appear too; so does a trailing space
Devanagari digits ०१२३४५६७८९ and Arabic-Indic ٠١٢٣٤٥٦٧٨٩ and Thai ๐๑๒๓๔๕.
The fox and the dog. The dog and the fox. quick quick brown brown fox fox.
world hello world hello world. The end."""

TRAIN_VOCAB = 480
TRAIN_VOCAB_CL100K = 420
SPECIALS = {"<|endoftext|>": None, "<|fim_prefix|>": None, "<|fim_middle|>": None}

SPLIT_INPUTS = [
    "",
    " ",
    "  ",
    "   ",
    "hello",
    "hello world",
    " hello world",
    "hello  world",
    "hello   world",
    "a",
    " a",
    "a ",
    "a  ",
    "a   b",
    "a    b",
    "  leading",
    "trailing  ",
    "don't",
    "it's we'll they've I'm he'd she'll 's 't 're 've 'm 'll 'd",
    "'S 'T 'RE 'VE 'M 'LL 'D",
    "'ſa",
    "'x 'r '' ''s",
    "42 3.14159 2024 1234567 100%",
    " 123",
    "x²y³ ½¾ ⅠⅡ ①② Ⓐ",
    "٠١٢٣ ०१२३ ๐๑๒๓ １２３４",
    "Привет мир",
    " Ελληνικά ",
    "日本語 テキスト 東京",
    "カタカナー and ひらがなー mixed",
    "한국어 텍스트",
    "שלום עולם",
    "مرحبا بالعالم",
    "नमस्ते दुनिया",
    "สวัสดีชาวโลก",
    "🚀launch 🌍fires 😀party",
    "🚀🚀🚀",
    "a\tb",
    "\ta",
    "a\nb",
    "a\r\nb",
    "a \nb",
    "  \n\n  x",
    "\n\n\n",
    "a  \n",
    "x y",
    "x　y",
    "　abc",
    " abc",
    "a  b",
    "xy",
    "the QUICK Brown fOX; 12,345.67!",
    "e.g. i.e. etc. -- isn't it?!"
]

# Extra code points for the class battery beyond the table boundaries: emoji,
# symbols, fold sources, unusual numbers and whitespace variants.
EXTRA_CLASS_CPS = [
    0x17F, 0x212A, 0x212B, 0x1F600, 0x1F680, 0x2600, 0x2603, 0x2708, 0x00A9,
    0x00AE, 0x00B0, 0x00D7, 0x00F7, 0x2018, 0x2019, 0x201C, 0x2013, 0x2014,
    0x2026, 0x00A1, 0x00BF, 0x00B7, 0x2039, 0x20AC, 0x2116, 0x2460, 0x2468,
    0x24B6, 0x24EA, 0x2160, 0x2168, 0x2170, 0x3021, 0x3029, 0x3038, 0x303A,
    0x3007, 0x3005, 0x30FC, 0x30A0, 0x309B, 0x309C, 0x0F33, 0x0F20, 0x0F2F,
    0x2189, 0x3248, 0x3251, 0x3280, 0x32C0, 0x1011, 0x1015, 0x1018, 0x1032,
    0x1039, 0x108F, 0x1090, 0x10A0, 0x10FD, 0x16EE, 0x16F0, 0x10107, 0x1018A,
    0x1018B, 0x1018C, 0x1D7CE, 0x1D7D8, 0x1D408, 0x1F10D, 0x1F12F, 0x1FB00,
    0x20000, 0x2A700, 0x2FA1D, 0xE0001, 0x100000,
]


def fail(message: str) -> None:
    print(f"oracle: {message}", file=sys.stderr)
    sys.exit(1)


# ---------------------------------------------------------------------------
# Unicode class tables
# ---------------------------------------------------------------------------


def ranges_of(predicate) -> list[list[int]]:
    """Maximal [lo, hi] runs of code points satisfying predicate, per block."""
    ranges = []
    for lo, hi in BLOCKS:
        run_lo = None
        for cp in range(lo, hi + 1):
            if predicate(chr(cp)):
                if run_lo is None:
                    run_lo = cp
            elif run_lo is not None:
                ranges.append([run_lo, cp - 1])
                run_lo = None
        if run_lo is not None:
            ranges.append([run_lo, hi])
    return ranges


def flatten(ranges: list[list[int]]) -> set[int]:
    out = set()
    for lo, hi in ranges:
        for cp in range(lo, hi + 1):
            out.add(cp)
    return out


def write_tables() -> dict[str, list[list[int]]]:
    letters = ranges_of(lambda ch: regex.match(r"\p{L}", ch) is not None)
    numbers = ranges_of(lambda ch: regex.match(r"\p{N}", ch) is not None)
    spaces = ranges_of(lambda ch: regex.match(r"\s", ch) is not None)
    tables = {
        "LETTER_RANGES": letters,
        "NUMBER_RANGES": numbers,
        "SPACE_RANGES": spaces,
    }
    # Sanity: the regex module's \s must agree with White_Space on our blocks
    # (it does: unlike the stdlib re module it follows the Unicode property).
    for cp in sorted(flatten(spaces)):
        if not unicodedata.category(chr(cp)) == "Zs" and cp not in (
            0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x85, 0x2028, 0x2029,
        ):
            fail(f"unexpected whitespace {cp:#x}")
    text = TABLES_TEMPLATE.format(
        letters=letters,
        numbers=numbers,
        spaces=spaces,
        unicode_version=unicodedata.unidata_version,
        regex_version=regex.__version__,
        count_l=len(letters),
        count_n=len(numbers),
        count_s=len(spaces),
    )
    path = PKG / "src" / "tables.py"
    if path.exists() and path.read_text() == text:
        print("tables.py unchanged")
    else:
        path.write_text(text)
        print(f"wrote {path.relative_to(REPO)}")
    return tables


TABLES_TEMPLATE = '''# osprey: flavor=python
# Derived Unicode data; required attribution and terms: ../NOTICE.md.
# osprey: namespace=tiktoken
# osprey: module=Tables
"""The Unicode character class range tables tiktoken's pre-tokenizers need.

Generated by scripts/oracles/tiktoken.py from the `regex` module (version
{regex_version}, Unicode {unicode_version}) — the same module tiktoken itself
uses — by sweeping the covered blocks and keeping every maximal run of code
points whose class is \\p{{L}}, \\p{{N}} or \\s (White_Space). Code points
outside the covered blocks classify as punctuation, which the package README
documents; the suite's corpus stays inside. Regenerate with the oracle.
"""

__all__ = ["LETTER_RANGES", "NUMBER_RANGES", "SPACE_RANGES"]

# {count_l} inclusive ranges of general category L* code points.
LETTER_RANGES = {letters}

# {count_n} inclusive ranges of general category N* code points.
NUMBER_RANGES = {numbers}

# {count_s} inclusive ranges of White_Space code points (what \\s matches).
SPACE_RANGES = {spaces}
'''


# ---------------------------------------------------------------------------
# Expected-value rendering
# ---------------------------------------------------------------------------


def flavor_str(s: str) -> str:
    """A Python-flavor string literal for s (never contains U+0000)."""
    out = ['"']
    for ch in s:
        cp = ord(ch)
        if ch == '"':
            out.append('\\"')
        elif ch == "\\":
            out.append("\\\\")
        elif ch == "\n":
            out.append("\\n")
        elif ch == "\r":
            out.append("\\r")
        elif ch == "\t":
            out.append("\\t")
        elif 32 <= cp < 127:
            out.append(ch)
        elif cp < 0x10000:
            out.append(f"\\u{cp:04x}")
        else:
            out.append(f"\\U{cp:08x}")
    out.append('"')
    return "".join(out)


def flavor_list(values) -> str:
    return "[" + ", ".join(flavor_value(v) for v in values) + "]"


# Constants that can render as a completely empty list need an annotation: an
# empty literal has no context for the checker to infer its element type.
EMPTY_ANNOTATIONS = {"TRAIN_EDGE_MERGES": "list[list[int]]"}


def flavor_nested(values) -> str:
    return "[" + ", ".join(flavor_value(v) for v in values) + "]"


def flavor_value(v) -> str:
    """One constant rendered for the Python flavor: booleans keep Python
    spelling, ints are literals, nested lists render recursively."""
    if isinstance(v, bool):
        return "True" if v else "False"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, str):
        return flavor_str(v)
    if isinstance(v, list):
        return "[" + ", ".join(flavor_value(x) for x in v) + "]"
    raise TypeError(f"cannot render {v!r}")


def flat_pieces(pieces) -> list[int]:
    """Piece byte lists flattened with 256 as the separator; string pieces
    (regex matches) are encoded to UTF-8 first."""
    out: list[int] = []
    for i, piece in enumerate(pieces):
        if i:
            out.append(256)
        if isinstance(piece, str):
            out.extend(piece.encode("utf-8"))
        else:
            out.extend(piece)
    return out


def cps_of(s: str) -> list[int]:
    return [ord(ch) for ch in s]


# ---------------------------------------------------------------------------
# Case computation
# ---------------------------------------------------------------------------


def check_patterns(split_inputs: list[str]) -> None:
    gpt2 = tiktoken.get_encoding("gpt2")
    cl100k = tiktoken.get_encoding("cl100k_base")
    classic_gpt2 = regex.compile(CLASSIC_GPT2_PAT)
    classic_cl100k = regex.compile(CLASSIC_CL100K_PAT)
    real_gpt2 = regex.compile(gpt2._pat_str)
    real_cl100k = regex.compile(cl100k._pat_str)
    for s in split_inputs + [CORPUS]:
        if classic_gpt2.findall(s) != real_gpt2.findall(s):
            fail(f"classic gpt2 pattern disagrees with real one on {s!r}")
        if classic_cl100k.findall(s) != real_cl100k.findall(s):
            fail(f"classic cl100k pattern disagrees with real one on {s!r}")


def split_cases() -> dict[str, list]:
    classic_gpt2 = regex.compile(CLASSIC_GPT2_PAT)
    classic_cl100k = regex.compile(CLASSIC_CL100K_PAT)
    return {
        "SPLIT_INPUTS": [cps_of(s) for s in SPLIT_INPUTS],
        "SPLIT_GPT2": [flat_pieces(classic_gpt2.findall(s)) for s in SPLIT_INPUTS],
        "SPLIT_CL100K": [flat_pieces(classic_cl100k.findall(s)) for s in SPLIT_INPUTS],
    }


def class_cases(tables: dict[str, list[list[int]]]) -> dict[str, list]:
    letters = flatten(tables["LETTER_RANGES"])
    numbers = flatten(tables["NUMBER_RANGES"])
    spaces = flatten(tables["SPACE_RANGES"])
    covered = set()
    for lo, hi in BLOCKS:
        covered.update(range(lo, hi + 1))
    # Every table boundary and its neighbours, a stride through each range,
    # and the curated extras. In-block expectations are the regex module's
    # verdict; outside the blocks the port says punctuation (kind 3), which
    # the battery pins as the documented divergence.
    cps: list[int] = []
    for table in tables.values():
        for lo, hi in table:
            cps.extend([lo - 1, lo, lo + 1, hi - 1, hi, hi + 1])
            width = hi - lo + 1
            stride = max(1, width // 4)
            cps.extend(range(lo, hi + 1, stride))
    cps.extend(EXTRA_CLASS_CPS)
    seen: set[int] = set()
    unique: list[int] = []
    for cp in cps:
        if 0 <= cp <= 0x10FFFF and cp not in seen and not (0xD800 <= cp <= 0xDFFF):
            seen.add(cp)
            unique.append(cp)
    unique.sort()
    kinds = []
    for cp in unique:
        if cp not in covered:
            kinds.append(3)
            continue
        ch = chr(cp)
        is_l = regex.match(r"\p{L}", ch) is not None
        is_n = regex.match(r"\p{N}", ch) is not None
        is_s = regex.match(r"\s", ch) is not None
        if sum(1 for x in (is_l, is_n, is_s) if x) > 1:
            fail(f"overlapping classes at {cp:#x}")
        kinds.append(0 if is_l else 1 if is_n else 2 if is_s else 3)
    return {"CLASS_CPS": unique, "CLASS_KINDS": kinds}


def train_cases() -> dict[str, list]:
    ranks_gpt2 = bpe_train(data=CORPUS, vocab_size=TRAIN_VOCAB, pat_str=CLASSIC_GPT2_PAT, visualise=None)
    ranks_cl = bpe_train(data=CORPUS, vocab_size=TRAIN_VOCAB_CL100K, pat_str=CLASSIC_CL100K_PAT, visualise=None)
    ranks_edge = bpe_train(data=CORPUS, vocab_size=256, pat_str=CLASSIC_GPT2_PAT, visualise=None)
    again = bpe_train(data=CORPUS, vocab_size=TRAIN_VOCAB, pat_str=CLASSIC_GPT2_PAT, visualise=None)
    if again != ranks_gpt2:
        fail("bpe_train is not deterministic")
    for name, ranks, vocab in (
        ("gpt2", ranks_gpt2, TRAIN_VOCAB),
        ("cl100k", ranks_cl, TRAIN_VOCAB_CL100K),
        ("edge", ranks_edge, 256),
    ):
        if sorted(ranks.values()) != list(range(len(ranks))):
            fail(f"{name} training produced non-contiguous ranks")
        if len(ranks) != vocab:
            fail(f"{name} training produced {len(ranks)} ranks, want {vocab}")
        for i in range(256):
            if ranks.get(bytes([i])) != i:
                fail(f"{name} training base ranks wrong at {i}")
    # A merge whose concatenation already exists would break contiguity; the
    # corpus avoids it, and this pins that.
    merges_gpt2 = [r for r, v in sorted(ranks_gpt2.items(), key=lambda kv: kv[1]) if v >= 256]
    merges_cl = [r for r, v in sorted(ranks_cl.items(), key=lambda kv: kv[1]) if v >= 256]
    return {
        "TRAIN_VOCAB": TRAIN_VOCAB,
        "TRAIN_MERGES": [list(m) for m in merges_gpt2],
        "TRAIN_VOCAB_CL100K": TRAIN_VOCAB_CL100K,
        "TRAIN_MERGES_CL100K": [list(m) for m in merges_cl],
        "TRAIN_EDGE_MERGES": [],
        "TRAIN_SMALL_VOCAB": 255,
    }


def encode_texts() -> list[str]:
    return [
        "",
        "hello world",
        "The quick brown fox jumps over the lazy dog.",
        "It's fine, isn't it? We'll see!",
        "12345 678 9",
        "🚀launch",
        "Ελληνικά and Привет and 東京",
        "  spaces  everywhere  ",
        "don't we'll they've I'm he'd 'll 've",
        "x²y³½¾",
        "naïve café résumé",
        "　ideographic and nbsp",
    ]


def special_texts() -> list[str]:
    return [
        "<|endoftext|>",
        "hello <|endoftext|> world",
        "<|endoftext|><|endoftext|>",
        "a<|fim_prefix|>b<|fim_middle|>c",
        "<|endoftext|> tail",
        "head <|fim_prefix|>",
        "hello <|endoftext",
        "no specials here",
        "<|endoftext|><|fim_prefix|><|fim_middle|>",
    ]


def decode_token_cases() -> list[list[int]]:
    return [
        [],
        [104, 101, 108, 108, 111],
        [255],
        [128],
        [192],
        [193],
        [245],
        [255, 255],
        [195, 169, 128],
        [224, 128, 128],
        [237, 160, 128],
        [240, 128, 128, 128],
        [240, 159, 152],
        [244, 144, 128, 128],
        [224, 160, 128],
        [194],
        [240, 159, 152, 128, 255],
        [239, 191, 189],
        [226, 130, 172],
        [227, 130, 164],
        [239, 191, 191],
        [244, 143, 191, 191],
        [237, 159, 191],
        [226, 128, 139],
        [206, 169],
        [1, 2, 3],
    ]


def trained_encoding_cases(ranks: dict[bytes, int]) -> dict[str, list]:
    specials = {
        "<|endoftext|>": TRAIN_VOCAB,
        "<|fim_prefix|>": TRAIN_VOCAB + 1,
        "<|fim_middle|>": TRAIN_VOCAB + 2,
    }
    enc = tiktoken.Encoding(
        name="trained", pat_str=CLASSIC_GPT2_PAT,
        mergeable_ranks=ranks, special_tokens=specials,
    )
    all_specials = set(specials)
    out: dict[str, list] = {}
    texts = encode_texts() + special_texts()
    out["ENC_TEXTS"] = [cps_of(s) for s in texts]
    out["ENC_ORDINARY"] = [enc.encode_ordinary(s) for s in texts]
    out["ENC_ALL_TEXTS"] = [cps_of(s) for s in special_texts()]
    out["ENC_ALL_IDS"] = [
        list(enc.encode(s, allowed_special=all_specials)) for s in special_texts()
    ]
    one = {"<|endoftext|>"}
    out["ENC_ONE_TEXTS"] = [cps_of(s) for s in special_texts()]
    out["ENC_ONE_IDS"] = []
    out["ENC_ONE_ERROR"] = []
    for s in special_texts():
        try:
            out["ENC_ONE_IDS"].append(list(enc.encode(s, allowed_special=one)))
            out["ENC_ONE_ERROR"].append(False)
        except ValueError:
            out["ENC_ONE_IDS"].append([])
            out["ENC_ONE_ERROR"].append(True)
    out["ENC_NONE_TEXTS"] = [cps_of(s) for s in special_texts()]
    out["ENC_NONE_ERROR"] = []
    out["ENC_NONE_TOKEN"] = []
    for s in special_texts():
        try:
            enc.encode(s, allowed_special=set())
            out["ENC_NONE_ERROR"].append(False)
            out["ENC_NONE_TOKEN"].append("")
        except ValueError as e:
            out["ENC_NONE_ERROR"].append(True)
            out["ENC_NONE_TOKEN"].append(e.args[0].split("'")[1])
    # Decoding, including invalid UTF-8, unknown tokens and specials. Byte 0
    # sequences decode to U+0000 in CPython; the port faults on them because
    # the runtime's NUL-terminated strings cannot hold it, so they are their
    # own constant.
    out["DECODE_NUL_SEQS"] = [[0], [0, 65], [255, 0, 65]]
    for seq in out["DECODE_NUL_SEQS"]:
        if "\x00" not in enc.decode(seq):
            fail("expected NUL in decode")
    seqs = decode_token_cases()
    special_seq = [TRAIN_VOCAB, 104, 105, TRAIN_VOCAB + 1]
    seqs.append(special_seq)
    out["DECODE_SEQS"] = seqs
    out["DECODE_REPLACE"] = [enc.decode(t) for t in seqs]
    out["DECODE_IGNORE"] = [enc.decode(t, errors="ignore") for t in seqs]
    out["DECODE_STRICT_OK"] = []
    for t in seqs:
        try:
            enc.decode(t, errors="strict")
            out["DECODE_STRICT_OK"].append(True)
        except UnicodeDecodeError:
            out["DECODE_STRICT_OK"].append(False)
    out["DECODE_UNKNOWN_SEQS"] = [[999999], [TRAIN_VOCAB + 99]]
    out["DECODE_BYTES_CASES"] = [[104, 101], [TRAIN_VOCAB], [240, 159, 152, 128]]
    out["DECODE_BYTES"] = [list(enc.decode_bytes(t)) for t in out["DECODE_BYTES_CASES"]]
    out["SINGLE_TOKEN_TEXTS"] = ["hello", " world", "<|endoftext|>", "zzqq", "<|nope|>"]
    out["SINGLE_TOKEN_IDS"] = []
    out["SINGLE_TOKEN_ERROR"] = []
    for s in out["SINGLE_TOKEN_TEXTS"]:
        try:
            out["SINGLE_TOKEN_IDS"].append(enc.encode_single_token(s))
            out["SINGLE_TOKEN_ERROR"].append(False)
        except KeyError:
            out["SINGLE_TOKEN_IDS"].append(-1)
            out["SINGLE_TOKEN_ERROR"].append(True)
    out["SINGLE_BYTES_TOKENS"] = [0, 65, 255, 256, TRAIN_VOCAB, TRAIN_VOCAB + 2, 999999]
    out["SINGLE_BYTES"] = []
    out["SINGLE_BYTES_ERROR"] = []
    for t in out["SINGLE_BYTES_TOKENS"]:
        try:
            out["SINGLE_BYTES"].append(list(enc.decode_single_token_bytes(t)))
            out["SINGLE_BYTES_ERROR"].append(False)
        except KeyError:
            out["SINGLE_BYTES"].append([])
            out["SINGLE_BYTES_ERROR"].append(True)
    out["TOKENS_BYTES_SEQS"] = [[104, 101, 108, 108, 111], [TRAIN_VOCAB, 256 + 5]]
    out["TOKENS_BYTES"] = [
        [list(b) for b in enc.decode_tokens_bytes(t)] for t in out["TOKENS_BYTES_SEQS"]
    ]
    out["TBV"] = flat_pieces(enc.token_byte_values())
    out["TBV_COUNT"] = len(enc.token_byte_values())
    out["N_VOCAB"] = enc.n_vocab
    out["MAX_TOKEN_VALUE"] = enc.max_token_value
    out["IS_SPECIAL"] = [enc.is_special_token(t) for t in [0, 255, TRAIN_VOCAB - 1, TRAIN_VOCAB, TRAIN_VOCAB + 2, TRAIN_VOCAB + 50]]
    out["SPECIAL_NAMES"] = sorted(specials)
    out["SPECIAL_IDS"] = [specials[n] for n in sorted(specials)]
    return out


def overlap_cases() -> dict[str, list]:
    """A synthetic encoding whose specials overlap by prefix, pinning the
    Rust core's alphabetically-sorted special alternation."""
    enc = tiktoken.Encoding(
        name="overlap", pat_str=CLASSIC_GPT2_PAT,
        mergeable_ranks={bytes([i]): i for i in range(256)},
        special_tokens={"<|a|>": 300, "<|a|>x": 301, "<|b|>": 302},
    )
    both = {"<|a|>", "<|a|>x", "<|b|>"}
    texts = ["<|a|>x", "<|a|>", "<|a|>xy", "<|b|><|a|>x", "a<|a|>xb"]
    out: dict[str, list] = {}
    out["OVERLAP_TEXTS"] = [cps_of(s) for s in texts]
    out["OVERLAP_IDS"] = [list(enc.encode(s, allowed_special=both)) for s in texts]
    return out


def base_ranks_of(by_rank) -> list[int]:
    """The 256 single-byte tokens' ranks, indexed by byte value."""
    base = [0] * 256
    for k, v in by_rank[:256]:
        base[k[0]] = v
    return base


def trunc_gpt2_cases() -> dict[str, list]:
    full = tiktoken.get_encoding("gpt2")
    n_merges = 1000
    by_rank = sorted(full._mergeable_ranks.items(), key=lambda kv: kv[1])
    if not all(len(k) == 1 for k, _ in by_rank[:256]):
        fail("gpt2 base ranks are not the single bytes")
    ranks = dict(by_rank[: 256 + n_merges])
    special = {"<|endoftext|>": 256 + n_merges}
    enc = tiktoken.Encoding(
        name="gpt2-trunc", pat_str=CLASSIC_GPT2_PAT,
        mergeable_ranks=ranks, special_tokens=special,
    )
    texts = [
        "the and of to in",
        "is on at as be",
        "the the the the",
        "one two three",
        "The of and to",
        "the quick brown fox",
        "hello world",
        "Hello, World!",
        "don't",
        "12345",
        "🚀",
        "É",
        "the <|endoftext|> end",
        "<|endoftext|>",
    ]
    out: dict[str, list] = {}
    out["GPT2_TRUNC_BASE"] = base_ranks_of(by_rank)
    out["GPT2_TRUNC_MERGES"] = [list(k) for k, v in by_rank[256 : 256 + n_merges]]
    out["GPT2_TRUNC_TEXTS"] = [cps_of(s) for s in texts]
    out["GPT2_TRUNC_IDS"] = []
    out["GPT2_TRUNC_SPECIAL_IDS"] = []
    for s in texts:
        out["GPT2_TRUNC_IDS"].append(list(enc.encode_ordinary(s)))
        out["GPT2_TRUNC_SPECIAL_IDS"].append(
            list(enc.encode(s, allowed_special=set(special)))
        )
    # Every case whose full-encoding leaves all fit the truncation must match
    # the truncated encoding: all intermediate merges rank below their leaves,
    # so the merge sequences coincide.
    checked = 0
    for s, ids in zip(texts, out["GPT2_TRUNC_IDS"]):
        full_ids = full.encode_ordinary(s)
        if all(t < 256 + n_merges for t in full_ids):
            if ids != full_ids:
                fail(f"truncated gpt2 disagrees with full gpt2 on {s!r}")
            checked += 1
    print(f"gpt2 truncation: {checked}/{len(texts)} cases match the full encoding")
    return out


def trunc_cl100k_cases() -> dict[str, list]:
    full = tiktoken.get_encoding("cl100k_base")
    # cl100k's ranks 0..255 are its 256 single-byte tokens, so truncation at
    # 256 + n_merges keeps every byte token and the first n_merges merges.
    n_merges = 1200
    by_rank = sorted(full._mergeable_ranks.items(), key=lambda kv: kv[1])
    if not all(len(k) == 1 for k, _ in by_rank[:256]):
        fail("cl100k base ranks are not the single bytes")
    ranks = dict(by_rank[: 256 + n_merges])
    special = {"<|endoftext|>": 100257}
    enc = tiktoken.Encoding(
        name="cl100k-trunc", pat_str=CLASSIC_CL100K_PAT,
        mergeable_ranks=ranks, special_tokens=special,
    )
    texts = [
        " the and of to in",
        "   x",
        "a  b",
        " for you and me",
        "hello world",
        "Hello, World!",
        "1234567 89",
        "don't we'll",
        "🚀launch",
        " Greek and Cyrillic",
        "日本語",
        "a \nb",
        "  \n\n  x",
        "the <|endoftext|> end",
    ]
    out: dict[str, list] = {}
    out["CL100K_TRUNC_BASE"] = base_ranks_of(by_rank)
    out["CL100K_TRUNC_MERGES"] = [
        list(k) for k, v in by_rank[256 : 256 + n_merges]
    ]
    out["CL100K_TRUNC_TEXTS"] = [cps_of(s) for s in texts]
    out["CL100K_TRUNC_IDS"] = [list(enc.encode_ordinary(s)) for s in texts]
    out["CL100K_TRUNC_SPECIAL_IDS"] = [
        list(enc.encode(s, allowed_special=set(special))) for s in texts
    ]
    present = set(ranks.values())
    checked = 0
    for s, ids in zip(texts, out["CL100K_TRUNC_IDS"]):
        full_ids = full.encode_ordinary(s)
        if set(full_ids) <= present:
            if ids != full_ids:
                fail(f"truncated cl100k disagrees with full cl100k on {s!r}")
            checked += 1
    print(f"cl100k truncation: {checked}/{len(texts)} cases match the full encoding")
    return out


def simple_cases(ranks: dict[bytes, int]) -> dict[str, list]:
    simple = SimpleBytePairEncoding(pat_str=CLASSIC_GPT2_PAT, mergeable_ranks=ranks)
    texts = encode_texts()
    out: dict[str, list] = {}
    out["SIMPLE_TEXTS"] = [cps_of(s) for s in texts]
    out["SIMPLE_IDS"] = [simple.encode(s, visualise=None) for s in texts]
    roundtrips = [simple.decode(simple.encode(s, visualise=None)) for s in texts]
    if roundtrips != texts:
        fail("SimpleBytePairEncoding round trip failed")
    out["SIMPLE_DECODE_TEXTS"] = ["hello world", "🚀launch", "naïve café"]
    out["SIMPLE_DECODE_IDS"] = [
        simple.encode(s, visualise=None) for s in out["SIMPLE_DECODE_TEXTS"]
    ]
    out["SIMPLE_DECODE"] = [
        simple.decode(simple.encode(s, visualise=None)) for s in out["SIMPLE_DECODE_TEXTS"]
    ]
    out["SIMPLE_DECODE_BYTES"] = [
        list(simple.decode_bytes(simple.encode(s, visualise=None)))
        for s in out["SIMPLE_DECODE_TEXTS"]
    ]
    out["SIMPLE_TOKENS_BYTES"] = [
        [list(b) for b in simple.decode_tokens_bytes(simple.encode(s, visualise=None))]
        for s in out["SIMPLE_DECODE_TEXTS"]
    ]
    # Module-level bpe_encode on raw byte pieces.
    pieces = [
        b"hello",
        b" hello",
        b"\xe2\x82\xac",
        b"aaa",
        b"abab",
        b"",
        bytes(range(256)),
        b"the quick",
    ]
    out["BPE_PIECES"] = [list(p) for p in pieces]
    out["BPE_IDS"] = [bpe_encode(ranks, p, visualise=None) for p in pieces]
    return out


def render_cases(cases: dict[str, list]) -> list[str]:
    """The expected.py lines: header, __all__, then one constant per name."""
    # Texts become code point lists (the flavor's literals cannot hold every
    # code point the way CPython's can); plain string constants stay strings.
    cp_lists = {
        "SPLIT_INPUTS", "ENC_TEXTS", "ENC_ALL_TEXTS", "ENC_ONE_TEXTS",
        "ENC_NONE_TEXTS", "OVERLAP_TEXTS", "GPT2_TRUNC_TEXTS",
        "CL100K_TRUNC_TEXTS", "SIMPLE_TEXTS", "SIMPLE_DECODE_TEXTS",
    }
    string_lists = {"SPECIAL_NAMES", "ENC_NONE_TOKEN", "DECODE_REPLACE", "DECODE_IGNORE"}
    lines = [
        "# osprey: flavor=python",
        "# osprey: namespace=tiktoken_expected",
        "# osprey: module=Expected",
        '"""Expected values for the @osprey/tiktoken suite, computed by the real',
        "tiktoken.",
        "",
        f"CPython {sys.version.split()[0]}, tiktoken {tiktoken.__version__}, regex",
        f"{regex.__version__}, Unicode {unicodedata.unidata_version}. Generated by",
        "scripts/oracles/tiktoken.py; never hand-edit. Piece byte lists are",
        "flattened with 256 as the separator (bytes are 0..255). Class kinds:",
        "0 letter, 1 number, 2 space, 3 other. Text constants are code point",
        "lists; the corpus is the one plain string.",
        '"""',
        "",
    ]
    lines.append("__all__ = " + flavor_list(sorted(cases)))
    lines.append("")
    for name in sorted(cases):
        if name == "CORPUS":
            lines.append(f"{name} = {flavor_str(cases[name])}")
        elif name in cp_lists:
            # The case builders already produced code point lists.
            lines.append(f"{name} = {flavor_nested(cases[name])}")
        elif name in string_lists:
            lines.append(
                f"{name} = [{', '.join(flavor_str(v) for v in cases[name])}]"
            )
        elif cases[name] == []:
            lines.append(f"{name}: {EMPTY_ANNOTATIONS.get(name, 'list[int]')} = []")
        else:
            lines.append(f"{name} = {flavor_value(cases[name])}")
        lines.append("")
    return lines


def main() -> None:
    tables = write_tables()
    check_patterns(SPLIT_INPUTS)
    cases: dict[str, list] = {}
    cases.update(split_cases())
    cases.update(class_cases(tables))
    cases.update(train_cases())
    ranks = bpe_train(data=CORPUS, vocab_size=TRAIN_VOCAB, pat_str=CLASSIC_GPT2_PAT, visualise=None)
    cases.update(trained_encoding_cases(ranks))
    cases.update(overlap_cases())
    cases.update(trunc_gpt2_cases())
    cases.update(trunc_cl100k_cases())
    cases.update(simple_cases(ranks))
    cases["CORPUS"] = CORPUS

    path = PKG / "tests" / "expected.py"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(render_cases(cases)))
    print(f"wrote {path.relative_to(REPO)}")


if __name__ == "__main__":
    main()
