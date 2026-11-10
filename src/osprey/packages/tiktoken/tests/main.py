# osprey: flavor=python
"""@osprey/tiktoken's suite: values computed by the real tiktoken in
src/osprey/packages/tiktoken/oracle/tiktoken.cpython, compared against the port through @osprey/testing.
Covers pre-tokenizer splits, Unicode classes, BPE training and merge order,
encode/decode round trips (invalid UTF-8, unknown tokens, NUL), special-token
overlap, truncated real vocabularies, the educational codec and bpe_encode."""
from testing import Check
from tiktoken import Bpe, Core, Educational, Pattern, Unicode
from tiktoken_expected import Expected
from tiktoken_tests import Contracts

ALL_SPECIALS = ["<|endoftext|>", "<|fim_prefix|>", "<|fim_middle|>"]

def at(xs, i, missing):
    """The element of a list at i, `missing` past the end."""
    return unwrap_or(listGet(xs, i), missing)

def text(cps):
    """The string a code point list spells."""
    return Unicode.text_of(cps)

def flatten(pieces):
    """Piece byte lists flattened with 256 as the separator."""
    return fold(range(len(pieces)), [], lambda acc, i: listConcat(listConcat(acc, [256]) if i > 0 else acc, at(pieces, i, [])))

def length_failed(name, got, want):
    """The failure `Check` for two lists whose lengths differ."""
    return Check.failed(name, f"length {len(got)}, want {len(want)}")

def first_diff(got, want, same, missing):
    """The first index at which got and want differ, -1 when none does."""
    return fold(range(len(want)), -1, lambda found, k: found if found >= 0 or same(at(got, k, missing), at(want, k, missing)) else k)

def list_equal(name, got, want, same, missing, show):
    """Length then first-difference comparison, `show` rendering an element for the message."""
    i = first_diff(got, want, same, missing)
    return length_failed(name, got, want) if len(got) != len(want) else Check.passed(name) if i < 0 else Check.failed(name, f"index {i}: got {show(at(got, i, missing))}, want {show(at(want, i, missing))}")

def nested_equal(name, got, want):
    """Exact equality of two lists of lists, naming the first differing index."""
    return list_equal(name, got, want, lambda a, b: structuralEquals(a, b), [], lambda v: Unicode.ints_text(v))

def strings_equal(name, got, want):
    """Exact equality of two string lists, naming the first differing index."""
    return list_equal(name, got, want, lambda a, b: a == b, "", lambda s: f"\"{s}\"")

def disallowed_token(message):
    """The special token named in an encode error message."""
    parts = unwrap_or(split(message, "'"), ["", "", ""])
    return unwrap_or(listGet(parts, 1), "")

def cases(n, f):
    """The outcomes of f(i) for i in 0..n-1."""
    return toList(map(range(n), f))

def concat(groups):
    """Flatten a list of outcome lists into one."""
    return foldList(groups, [], listConcat)

def checked(name, result, answer):
    """`answer name value` when result answered, the reported failure otherwise."""
    match result:
        case Success(value=value):
            return answer(name, value)
        case Error(message=message):
            return Check.failed(name, message)

def refused(name, why, result):
    """A `Result` case that must be an error; a success is the failure, reported as `why`."""
    match result:
        case Success(value=_value):
            return Check.failed(name, why)
        case Error(message=_message):
            return Check.passed(name)

def result_case(name, result, compare, want):
    """A `Result` case: `compare name value want` when it answered, the failure otherwise."""
    return checked(name, result, lambda name, value: compare(name, value, want))

def truth_case(name, result, want_error, value_ok, describe):
    """A `Result` truth case: success needs no wanted error and `value_ok value`, an error needs `want_error`."""
    match result:
        case Success(value=value):
            return Check.truth(name, not want_error and value_ok(value), describe(value))
        case Error(message=message):
            return Check.truth(name, want_error, message)

# The drivers every family below runs on: one case per index of an expected table.

def result_list(label, count, run, compare, want, missing):
    """`count` cases `label[i]`: run(i) compared against want[i]."""
    return cases(count, lambda i: result_case(f"{label}[{i}]", run(i), compare, at(want, i, missing)))

def truth_list(label, count, run, want_error, verdict, describe):
    """`count` truth cases `label[i]`; `verdict i value` and `describe i value` judge a success."""
    return cases(count, lambda i: truth_case(f"{label}[{i}]", run(i), at(want_error, i, False), lambda value: verdict(i, value), lambda value: describe(i, value)))

def bytes_list(label, count, run, want_error, want):
    """`count` byte-list cases `label[i]`, each equal to want[i]."""
    return truth_list(label, count, run, want_error, lambda i, value: structuralEquals(value, at(want, i, [])), lambda i, value: f"got {Unicode.ints_text(value)}")

def refused_list(label, why, seqs, enc):
    """Failure of every decode sequence list in seqs, reported as `why` when it answers."""
    return cases(len(seqs), lambda i: refused(f"{label}[{i}]", why, Core.decode(enc, at(seqs, i, []))))

# Encodings under test

def trained_ranks():
    """The trained 480-token vocabulary, rebuilt from the oracle's merge list."""
    return Bpe.ranks_with_merges(Expected.TRAIN_MERGES)

def trained_specials():
    """The three special tokens of the trained encoding."""
    return {
        "<|endoftext|>": Expected.TRAIN_VOCAB,
        "<|fim_prefix|>": Expected.TRAIN_VOCAB + 1,
        "<|fim_middle|>": Expected.TRAIN_VOCAB + 2,
    }

def trained_enc():
    return Core.build("trained", trained_ranks(), trained_specials(), Pattern.Gpt2())

def overlap_enc():
    return Core.build("overlap", Bpe.base_ranks(), {"<|a|>": 300, "<|a|>x": 301, "<|b|>": 302}, Pattern.Gpt2())

def ranks_from(base_ranks_list, merges):
    """A vocabulary from real base-256 ranks and a merge list (ranks 256+i)."""
    base = fold(range(256), {}, lambda acc, b: mapSet(acc, Bpe.key_char(b), at(base_ranks_list, b, 0)))
    return fold(range(len(merges)), base, lambda acc, i: mapSet(acc, Bpe.key(at(merges, i, [])), 256 + i))

def gpt2_trunc_enc():
    return Core.build("gpt2-trunc", ranks_from(Expected.GPT2_TRUNC_BASE, Expected.GPT2_TRUNC_MERGES), {"<|endoftext|>": 256 + len(Expected.GPT2_TRUNC_MERGES)}, Pattern.Gpt2())

def cl100k_trunc_enc():
    return Core.build("cl100k-trunc", ranks_from(Expected.CL100K_TRUNC_BASE, Expected.CL100K_TRUNC_MERGES), {"<|endoftext|>": 100257}, Pattern.Cl100kBase())

def simple_enc():
    return Educational.simple(Pattern.Gpt2(), trained_ranks())

# Pre-tokenizers and Unicode classes

def split_case(label, pattern, want, i):
    """Pattern.split of split input i, flattened and compared against want."""
    return Check.ints_equal(f"{label}[{i}]", flatten(Pattern.split(pattern, text(at(Expected.SPLIT_INPUTS, i, [])))), at(want, i, []))

def class_case(i):
    cp = at(Expected.CLASS_CPS, i, 0)
    return Check.equal(f"class[{cp}]", Unicode.class_of(cp), at(Expected.CLASS_KINDS, i, 0))

# Training

def train_case(label, vocab, pattern, want):
    """bpe_train of the corpus, its merge list compared against want."""
    return checked(f"{label}.merges", Educational.bpe_train(Expected.CORPUS, vocab, pattern), lambda name, value: nested_equal(name, Bpe.merges_of(value), want))

# Encoding (ordinary and special-token aware)

def encode_none_case(enc, i):
    """encode with no specials: an error must name the disallowed token."""
    want_error = at(Expected.ENC_NONE_ERROR, i, False)
    want_token = at(Expected.ENC_NONE_TOKEN, i, "")
    match Core.encode(enc, text(at(Expected.ENC_NONE_TEXTS, i, [])), []):
        case Success(value=value):
            return Check.truth(f"encode.none[{i}]", not want_error, f"got {Unicode.ints_text(value)}")
        case Error(message=message):
            return Check.truth(f"encode.none[{i}]", want_error and disallowed_token(message) == want_token, message)

def encode_cases(enc):
    return concat([
        result_list("encode.ordinary", len(Expected.ENC_TEXTS), lambda i: Core.encode_ordinary(enc, text(at(Expected.ENC_TEXTS, i, []))), Check.ints_equal, Expected.ENC_ORDINARY, []),
        result_list("encode.all", len(Expected.ENC_ALL_TEXTS), lambda i: Core.encode(enc, text(at(Expected.ENC_ALL_TEXTS, i, [])), ALL_SPECIALS), Check.ints_equal, Expected.ENC_ALL_IDS, []),
        bytes_list("encode.one", len(Expected.ENC_ONE_TEXTS), lambda i: Core.encode(enc, text(at(Expected.ENC_ONE_TEXTS, i, [])), ["<|endoftext|>"]), Expected.ENC_ONE_ERROR, Expected.ENC_ONE_IDS),
        cases(len(Expected.ENC_NONE_TEXTS), lambda i: encode_none_case(enc, i)),
    ])

# Decoding

def decode_cases(enc):
    seqs = len(Expected.DECODE_SEQS)
    return concat([
        result_list("decode.replace", seqs, lambda i: Core.decode(enc, at(Expected.DECODE_SEQS, i, [])), Check.equal, Expected.DECODE_REPLACE, ""),
        result_list("decode.ignore", seqs, lambda i: Core.decode_errors(enc, at(Expected.DECODE_SEQS, i, []), "ignore"), Check.equal, Expected.DECODE_IGNORE, ""),
        truth_list("decode.strict", seqs, lambda i: Core.decode_errors(enc, at(Expected.DECODE_SEQS, i, []), "strict"), mapList(Expected.DECODE_STRICT_OK, lambda ok: not ok), lambda i, value: True, lambda i, value: "expected a strict error"),
        refused_list("decode.nul", "expected an error for NUL", Expected.DECODE_NUL_SEQS, enc),
        refused_list("decode.unknown", "expected an unknown-token error", Expected.DECODE_UNKNOWN_SEQS, enc),
        result_list("decode.bytes", len(Expected.DECODE_BYTES_CASES), lambda i: Core.decode_bytes(enc, at(Expected.DECODE_BYTES_CASES, i, [])), Check.ints_equal, Expected.DECODE_BYTES, []),
    ])

# Single tokens and token byte values

def special_probe(i):
    """The token the oracle probed for is_special_token at index i."""
    return at([0, 255, Expected.TRAIN_VOCAB - 1, Expected.TRAIN_VOCAB, Expected.TRAIN_VOCAB + 2], i, 0) if i < 5 else Expected.TRAIN_VOCAB + 50

def single_cases(enc):
    return concat([
        truth_list("single.token", len(Expected.SINGLE_TOKEN_TEXTS), lambda i: Core.encode_single_token(enc, at(Expected.SINGLE_TOKEN_TEXTS, i, "")), Expected.SINGLE_TOKEN_ERROR, lambda i, value: value == at(Expected.SINGLE_TOKEN_IDS, i, 0), lambda i, value: f"got {value}"),
        bytes_list("single.bytes", len(Expected.SINGLE_BYTES_TOKENS), lambda i: Core.decode_single_token_bytes(enc, at(Expected.SINGLE_BYTES_TOKENS, i, 0)), Expected.SINGLE_BYTES_ERROR, Expected.SINGLE_BYTES),
        result_list("tokens.bytes", len(Expected.TOKENS_BYTES_SEQS), lambda i: Core.decode_tokens_bytes(enc, at(Expected.TOKENS_BYTES_SEQS, i, [])), nested_equal, Expected.TOKENS_BYTES, []),
    ])

def special_names_case(enc):
    names = Core.special_tokens_set(enc)
    ids = mapList(names, lambda n: unwrap_or(mapGet(enc.special_tokens, n), -1))
    return [
        strings_equal("special.names", names, Expected.SPECIAL_NAMES),
        Check.ints_equal("special.ids", ids, Expected.SPECIAL_IDS),
        Check.ints_equal("unicode.map_indexed", Unicode.map_indexed([9, 9, 9], lambda i: i - 1), [-1, 0, 1]),
    ]

def vocab_cases(enc):
    return [
        Check.ints_equal("token_byte_values", flatten(Core.token_byte_values(enc)), Expected.TBV),
        Check.equal("token_byte_values.count", len(Core.token_byte_values(enc)), Expected.TBV_COUNT),
        Check.equal("n_vocab", enc.n_vocab, Expected.N_VOCAB),
        Check.equal("max_token_value", enc.max_token_value, Expected.MAX_TOKEN_VALUE),
    ]

def enc_tail_cases(enc):
    return concat([
        vocab_cases(enc),
        cases(len(Expected.IS_SPECIAL), lambda i: Check.equal(f"is_special[{i}]", Core.is_special_token(enc, special_probe(i)), at(Expected.IS_SPECIAL, i, False))),
        special_names_case(enc),
    ])

# Overlap and truncation encodings

def overlap_cases():
    return result_list("overlap", len(Expected.OVERLAP_TEXTS), lambda i: Core.encode(overlap_enc(), text(at(Expected.OVERLAP_TEXTS, i, [])), ["<|a|>", "<|a|>x", "<|b|>"]), Check.ints_equal, Expected.OVERLAP_IDS, [])

def trunc_encoding_cases(label, enc, texts, ids, special_ids):
    """One truncated encoding: its ordinary ids cases, then its special-token ids cases."""
    return concat([
        result_list(label, len(texts), lambda i: Core.encode_ordinary(enc, text(at(texts, i, []))), Check.ints_equal, ids, []),
        result_list(f"{label}.special", len(texts), lambda i: Core.encode(enc, text(at(texts, i, [])), ["<|endoftext|>"]), Check.ints_equal, special_ids, []),
    ])

def trunc_cases():
    return concat([
        trunc_encoding_cases("gpt2.trunc", gpt2_trunc_enc(), Expected.GPT2_TRUNC_TEXTS, Expected.GPT2_TRUNC_IDS, Expected.GPT2_TRUNC_SPECIAL_IDS),
        trunc_encoding_cases("cl100k.trunc", cl100k_trunc_enc(), Expected.CL100K_TRUNC_TEXTS, Expected.CL100K_TRUNC_IDS, Expected.CL100K_TRUNC_SPECIAL_IDS),
    ])

# Educational codec and module-level bpe_encode

def encode_ids(s):
    """The educational codec's ids for a string, [] on error."""
    return unwrap_or(Educational.encode(simple_enc(), s), [])

def simple_text_ids(i):
    """Text i of the decode inputs with its simple-codec ids."""
    return encode_ids(at(Expected.SIMPLE_DECODE_TEXTS, i, ""))

def simple_cases():
    n = len(Expected.SIMPLE_DECODE_TEXTS)
    return concat([
        result_list("simple.ids", len(Expected.SIMPLE_TEXTS), lambda i: Educational.encode(simple_enc(), text(at(Expected.SIMPLE_TEXTS, i, []))), Check.ints_equal, Expected.SIMPLE_IDS, []),
        cases(n, lambda i: Check.ints_equal(f"simple.decode.ids[{i}]", simple_text_ids(i), at(Expected.SIMPLE_DECODE_IDS, i, []))),
        result_list("simple.decode", n, lambda i: Educational.decode(simple_enc(), simple_text_ids(i)), Check.equal, Expected.SIMPLE_DECODE, ""),
        result_list("simple.decode.bytes", n, lambda i: Educational.decode_bytes(simple_enc(), simple_text_ids(i)), Check.ints_equal, Expected.SIMPLE_DECODE_BYTES, []),
        result_list("simple.tokens.bytes", n, lambda i: Educational.decode_tokens_bytes(simple_enc(), simple_text_ids(i)), nested_equal, Expected.SIMPLE_TOKENS_BYTES, []),
    ])

def bpe_cases():
    return result_list("bpe", len(Expected.BPE_PIECES), lambda i: Educational.bpe_encode(trained_ranks(), at(Expected.BPE_PIECES, i, [])), Check.ints_equal, Expected.BPE_IDS, [])

# Suite assembly

def split_cases():
    return listConcat(
        cases(len(Expected.SPLIT_INPUTS), lambda i: split_case("split.gpt2", Pattern.Gpt2(), Expected.SPLIT_GPT2, i)),
        cases(len(Expected.SPLIT_INPUTS), lambda i: split_case("split.cl100k", Pattern.Cl100kBase(), Expected.SPLIT_CL100K, i)),
    )

def class_cases():
    return cases(len(Expected.CLASS_CPS), class_case)

def train_cases():
    return [
        train_case("train.gpt2", Expected.TRAIN_VOCAB, Pattern.Gpt2(), Expected.TRAIN_MERGES),
        train_case("train.cl100k", Expected.TRAIN_VOCAB_CL100K, Pattern.Cl100kBase(), Expected.TRAIN_MERGES_CL100K),
        train_case("train.edge", 256, Pattern.Gpt2(), Expected.TRAIN_EDGE_MERGES),
        refused("train.small", "expected an error for vocab_size < 256", Educational.bpe_train(Expected.CORPUS, Expected.TRAIN_SMALL_VOCAB, Pattern.Gpt2())),
    ]

def suite():
    enc = trained_enc()
    return concat([
        split_cases(), class_cases(), train_cases(), encode_cases(enc),
        decode_cases(enc), single_cases(enc), enc_tail_cases(enc),
        overlap_cases(), trunc_cases(), simple_cases(), bpe_cases(),
        Contracts.cases(),
    ])

def main():
    return Check.run(suite)
