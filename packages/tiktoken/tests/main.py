# osprey: flavor=python
"""@osprey/tiktoken's suite: values computed by the real tiktoken in
scripts/oracles/tiktoken.py, compared against the port through @osprey/testing.
Covers pre-tokenizer splits, Unicode classes, BPE training and merge order,
encode/decode round trips (invalid UTF-8, unknown tokens, NUL), special-token
overlap, truncated real vocabularies, the educational codec and bpe_encode."""
from testing import Check
from tiktoken import Bpe, Core, Educational, Pattern, Unicode
from tiktoken_expected import Expected
from tiktoken_tests import Contracts

ALL_SPECIALS = ["<|endoftext|>", "<|fim_prefix|>", "<|fim_middle|>"]

def ati(xs, i):
    """The int element of a list at i, 0 past the end."""
    return unwrap_or(listGet(xs, i), 0)

def atl(xs, i):
    """The list element of a list at i, [] past the end."""
    return unwrap_or(listGet(xs, i), [])

def ats(xs, i):
    """The string element of a list at i, "" past the end."""
    return unwrap_or(listGet(xs, i), "")

def atb(xs, i):
    """The bool element of a list at i, False past the end."""
    return unwrap_or(listGet(xs, i), False)

def text(cps):
    """The string a code point list spells."""
    return Unicode.text_of(cps)

def flatten(pieces):
    """Piece byte lists flattened with 256 as the separator."""
    return fold(range(len(pieces)), [], lambda acc, i: listConcat(listConcat(acc, [256]) if i > 0 else acc, atl(pieces, i)))

def nested_equal(name, got, want):
    """Exact equality of two lists of lists, naming the first differing index."""
    if len(got) != len(want):
        return Check.failed(name, f"length {len(got)}, want {len(want)}")
    else:
        i = fold(range(len(want)), -1, lambda found, k: found if found >= 0 or structuralEquals(atl(got, k), atl(want, k)) else k)
        return Check.passed(name) if i < 0 else Check.failed(name, f"index {i}: got {Unicode.ints_text(atl(got, i))}, want {Unicode.ints_text(atl(want, i))}")

def strings_equal(name, got, want):
    """Exact equality of two string lists, naming the first differing index."""
    if len(got) != len(want):
        return Check.failed(name, f"length {len(got)}, want {len(want)}")
    else:
        i = fold(range(len(want)), -1, lambda found, k: found if found >= 0 or ats(got, k) == ats(want, k) else k)
        return Check.passed(name) if i < 0 else Check.failed(name, f"index {i}: got \"{ats(got, i)}\", want \"{ats(want, i)}\"")

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
    base = fold(range(256), {}, lambda acc, b: mapSet(acc, Bpe.key_char(b), ati(base_ranks_list, b)))
    return fold(range(len(merges)), base, lambda acc, i: mapSet(acc, Bpe.key(atl(merges, i)), 256 + i))

def gpt2_trunc_enc():
    return Core.build("gpt2-trunc", ranks_from(Expected.GPT2_TRUNC_BASE, Expected.GPT2_TRUNC_MERGES), {"<|endoftext|>": 256 + len(Expected.GPT2_TRUNC_MERGES)}, Pattern.Gpt2())

def cl100k_trunc_enc():
    return Core.build("cl100k-trunc", ranks_from(Expected.CL100K_TRUNC_BASE, Expected.CL100K_TRUNC_MERGES), {"<|endoftext|>": 100257}, Pattern.Cl100kBase())

def simple_enc():
    return Educational.simple(Pattern.Gpt2(), trained_ranks())

# Pre-tokenizers and Unicode classes

def split_gpt2_case(i):
    return Check.ints_equal(f"split.gpt2[{i}]", flatten(Pattern.split(Pattern.Gpt2(), text(atl(Expected.SPLIT_INPUTS, i)))), atl(Expected.SPLIT_GPT2, i))

def split_cl100k_case(i):
    return Check.ints_equal(f"split.cl100k[{i}]", flatten(Pattern.split(Pattern.Cl100kBase(), text(atl(Expected.SPLIT_INPUTS, i)))), atl(Expected.SPLIT_CL100K, i))

def class_case(i):
    cp = ati(Expected.CLASS_CPS, i)
    return Check.equal(f"class[{cp}]", Unicode.class_of(cp), ati(Expected.CLASS_KINDS, i))

# Training

def train_gpt2_case():
    match Educational.bpe_train(Expected.CORPUS, Expected.TRAIN_VOCAB, Pattern.Gpt2()):
        case Success(value=value):
            return nested_equal("train.gpt2.merges", Bpe.merges_of(value), Expected.TRAIN_MERGES)
        case Error(message=message):
            return Check.failed("train.gpt2.merges", message)

def train_cl100k_case():
    match Educational.bpe_train(Expected.CORPUS, Expected.TRAIN_VOCAB_CL100K, Pattern.Cl100kBase()):
        case Success(value=value):
            return nested_equal("train.cl100k.merges", Bpe.merges_of(value), Expected.TRAIN_MERGES_CL100K)
        case Error(message=message):
            return Check.failed("train.cl100k.merges", message)

def train_edge_case():
    match Educational.bpe_train(Expected.CORPUS, 256, Pattern.Gpt2()):
        case Success(value=value):
            return nested_equal("train.edge.merges", Bpe.merges_of(value), Expected.TRAIN_EDGE_MERGES)
        case Error(message=message):
            return Check.failed("train.edge.merges", message)

def train_small_case():
    match Educational.bpe_train(Expected.CORPUS, Expected.TRAIN_SMALL_VOCAB, Pattern.Gpt2()):
        case Success(value=value):
            return Check.failed("train.small", "expected an error for vocab_size < 256")
        case Error(message=message):
            return Check.passed("train.small")

# Encoding (ordinary and special-token aware)

def encode_ordinary_case(enc, i):
    match Core.encode_ordinary(enc, text(atl(Expected.ENC_TEXTS, i))):
        case Success(value=value):
            return Check.ints_equal(f"encode.ordinary[{i}]", value, atl(Expected.ENC_ORDINARY, i))
        case Error(message=message):
            return Check.failed(f"encode.ordinary[{i}]", message)

def encode_all_case(enc, i):
    match Core.encode(enc, text(atl(Expected.ENC_ALL_TEXTS, i)), ALL_SPECIALS):
        case Success(value=value):
            return Check.ints_equal(f"encode.all[{i}]", value, atl(Expected.ENC_ALL_IDS, i))
        case Error(message=message):
            return Check.failed(f"encode.all[{i}]", message)

def encode_one_case(enc, i):
    want_error = atb(Expected.ENC_ONE_ERROR, i)
    match Core.encode(enc, text(atl(Expected.ENC_ONE_TEXTS, i)), ["<|endoftext|>"]):
        case Success(value=value):
            return Check.truth(f"encode.one[{i}]", not want_error and structuralEquals(value, atl(Expected.ENC_ONE_IDS, i)), f"got {Unicode.ints_text(value)}")
        case Error(message=message):
            return Check.truth(f"encode.one[{i}]", want_error, message)

def encode_none_case(enc, i):
    want_error = atb(Expected.ENC_NONE_ERROR, i)
    want_token = ats(Expected.ENC_NONE_TOKEN, i)
    match Core.encode(enc, text(atl(Expected.ENC_NONE_TEXTS, i)), []):
        case Success(value=value):
            return Check.truth(f"encode.none[{i}]", not want_error, f"got {Unicode.ints_text(value)}")
        case Error(message=message):
            return Check.truth(f"encode.none[{i}]", want_error and disallowed_token(message) == want_token, message)

# Decoding

def decode_replace_case(enc, i):
    match Core.decode(enc, atl(Expected.DECODE_SEQS, i)):
        case Success(value=value):
            return Check.equal(f"decode.replace[{i}]", value, ats(Expected.DECODE_REPLACE, i))
        case Error(message=message):
            return Check.failed(f"decode.replace[{i}]", message)

def decode_ignore_case(enc, i):
    match Core.decode_errors(enc, atl(Expected.DECODE_SEQS, i), "ignore"):
        case Success(value=value):
            return Check.equal(f"decode.ignore[{i}]", value, ats(Expected.DECODE_IGNORE, i))
        case Error(message=message):
            return Check.failed(f"decode.ignore[{i}]", message)

def decode_strict_case(enc, i):
    want_ok = atb(Expected.DECODE_STRICT_OK, i)
    match Core.decode_errors(enc, atl(Expected.DECODE_SEQS, i), "strict"):
        case Success(value=value):
            return Check.truth(f"decode.strict[{i}]", want_ok, "expected a strict error")
        case Error(message=message):
            return Check.truth(f"decode.strict[{i}]", not want_ok, message)

def decode_nul_case(enc, i):
    match Core.decode(enc, atl(Expected.DECODE_NUL_SEQS, i)):
        case Success(value=value):
            return Check.failed(f"decode.nul[{i}]", "expected an error for NUL")
        case Error(message=message):
            return Check.passed(f"decode.nul[{i}]")

def decode_unknown_case(enc, i):
    match Core.decode(enc, atl(Expected.DECODE_UNKNOWN_SEQS, i)):
        case Success(value=value):
            return Check.failed(f"decode.unknown[{i}]", "expected an unknown-token error")
        case Error(message=message):
            return Check.passed(f"decode.unknown[{i}]")

def decode_bytes_case(enc, i):
    match Core.decode_bytes(enc, atl(Expected.DECODE_BYTES_CASES, i)):
        case Success(value=value):
            return Check.ints_equal(f"decode.bytes[{i}]", value, atl(Expected.DECODE_BYTES, i))
        case Error(message=message):
            return Check.failed(f"decode.bytes[{i}]", message)

# Single tokens and token byte values

def single_token_case(enc, i):
    want_error = atb(Expected.SINGLE_TOKEN_ERROR, i)
    match Core.encode_single_token(enc, ats(Expected.SINGLE_TOKEN_TEXTS, i)):
        case Success(value=value):
            return Check.truth(f"single.token[{i}]", not want_error and value == ati(Expected.SINGLE_TOKEN_IDS, i), f"got {value}")
        case Error(message=message):
            return Check.truth(f"single.token[{i}]", want_error, message)

def single_bytes_case(enc, i):
    want_error = atb(Expected.SINGLE_BYTES_ERROR, i)
    match Core.decode_single_token_bytes(enc, ati(Expected.SINGLE_BYTES_TOKENS, i)):
        case Success(value=value):
            return Check.truth(f"single.bytes[{i}]", not want_error and structuralEquals(value, atl(Expected.SINGLE_BYTES, i)), f"got {Unicode.ints_text(value)}")
        case Error(message=message):
            return Check.truth(f"single.bytes[{i}]", want_error, message)

def tokens_bytes_case(enc, i):
    match Core.decode_tokens_bytes(enc, atl(Expected.TOKENS_BYTES_SEQS, i)):
        case Success(value=value):
            return nested_equal(f"tokens.bytes[{i}]", value, atl(Expected.TOKENS_BYTES, i))
        case Error(message=message):
            return Check.failed(f"tokens.bytes[{i}]", message)

def special_probe(i):
    """The token the oracle probed for is_special_token at index i."""
    if i == 0:
        return 0
    elif i == 1:
        return 255
    elif i == 2:
        return Expected.TRAIN_VOCAB - 1
    elif i == 3:
        return Expected.TRAIN_VOCAB
    elif i == 4:
        return Expected.TRAIN_VOCAB + 2
    else:
        return Expected.TRAIN_VOCAB + 50

def is_special_case(enc, i):
    return Check.equal(f"is_special[{i}]", Core.is_special_token(enc, special_probe(i)), atb(Expected.IS_SPECIAL, i))

def special_names_case(enc):
    names = Core.special_tokens_set(enc)
    ids = mapList(names, lambda n: unwrap_or(mapGet(enc.special_tokens, n), -1))
    return [
        strings_equal("special.names", names, Expected.SPECIAL_NAMES),
        Check.ints_equal("special.ids", ids, Expected.SPECIAL_IDS),
    ]

def vocab_cases(enc):
    return [
        Check.ints_equal("token_byte_values", flatten(Core.token_byte_values(enc)), Expected.TBV),
        Check.equal("token_byte_values.count", len(Core.token_byte_values(enc)), Expected.TBV_COUNT),
        Check.equal("n_vocab", enc.n_vocab, Expected.N_VOCAB),
        Check.equal("max_token_value", enc.max_token_value, Expected.MAX_TOKEN_VALUE),
    ]

# Overlap and truncation encodings

def overlap_case(i):
    match Core.encode(overlap_enc(), text(atl(Expected.OVERLAP_TEXTS, i)), ["<|a|>", "<|a|>x", "<|b|>"]):
        case Success(value=value):
            return Check.ints_equal(f"overlap[{i}]", value, atl(Expected.OVERLAP_IDS, i))
        case Error(message=message):
            return Check.failed(f"overlap[{i}]", message)

def gpt2_trunc_ordinary_case(i):
    match Core.encode_ordinary(gpt2_trunc_enc(), text(atl(Expected.GPT2_TRUNC_TEXTS, i))):
        case Success(value=value):
            return Check.ints_equal(f"gpt2.trunc[{i}]", value, atl(Expected.GPT2_TRUNC_IDS, i))
        case Error(message=message):
            return Check.failed(f"gpt2.trunc[{i}]", message)

def gpt2_trunc_special_case(i):
    match Core.encode(gpt2_trunc_enc(), text(atl(Expected.GPT2_TRUNC_TEXTS, i)), ["<|endoftext|>"]):
        case Success(value=value):
            return Check.ints_equal(f"gpt2.trunc.special[{i}]", value, atl(Expected.GPT2_TRUNC_SPECIAL_IDS, i))
        case Error(message=message):
            return Check.failed(f"gpt2.trunc.special[{i}]", message)

def cl100k_trunc_ordinary_case(i):
    match Core.encode_ordinary(cl100k_trunc_enc(), text(atl(Expected.CL100K_TRUNC_TEXTS, i))):
        case Success(value=value):
            return Check.ints_equal(f"cl100k.trunc[{i}]", value, atl(Expected.CL100K_TRUNC_IDS, i))
        case Error(message=message):
            return Check.failed(f"cl100k.trunc[{i}]", message)

def cl100k_trunc_special_case(i):
    match Core.encode(cl100k_trunc_enc(), text(atl(Expected.CL100K_TRUNC_TEXTS, i)), ["<|endoftext|>"]):
        case Success(value=value):
            return Check.ints_equal(f"cl100k.trunc.special[{i}]", value, atl(Expected.CL100K_TRUNC_SPECIAL_IDS, i))
        case Error(message=message):
            return Check.failed(f"cl100k.trunc.special[{i}]", message)

# Educational codec and module-level bpe_encode

def simple_ids_case(i):
    match Educational.encode(simple_enc(), text(atl(Expected.SIMPLE_TEXTS, i))):
        case Success(value=value):
            return Check.ints_equal(f"simple.ids[{i}]", value, atl(Expected.SIMPLE_IDS, i))
        case Error(message=message):
            return Check.failed(f"simple.ids[{i}]", message)

def encode_ids(s):
    """The educational codec's ids for a string, [] on error."""
    match Educational.encode(simple_enc(), s):
        case Success(value=value):
            return value
        case Error(message=message):
            return []

def simple_decode_ids_case(i):
    s = ats(Expected.SIMPLE_DECODE_TEXTS, i)
    return Check.ints_equal(f"simple.decode.ids[{i}]", encode_ids(s), atl(Expected.SIMPLE_DECODE_IDS, i))

def simple_decode_case(i):
    s = ats(Expected.SIMPLE_DECODE_TEXTS, i)
    match Educational.decode(simple_enc(), encode_ids(s)):
        case Success(value=value):
            return Check.equal(f"simple.decode[{i}]", value, ats(Expected.SIMPLE_DECODE, i))
        case Error(message=message):
            return Check.failed(f"simple.decode[{i}]", message)

def simple_decode_bytes_case(i):
    s = ats(Expected.SIMPLE_DECODE_TEXTS, i)
    match Educational.decode_bytes(simple_enc(), encode_ids(s)):
        case Success(value=value):
            return Check.ints_equal(f"simple.decode.bytes[{i}]", value, atl(Expected.SIMPLE_DECODE_BYTES, i))
        case Error(message=message):
            return Check.failed(f"simple.decode.bytes[{i}]", message)

def simple_tokens_bytes_case(i):
    s = ats(Expected.SIMPLE_DECODE_TEXTS, i)
    match Educational.decode_tokens_bytes(simple_enc(), encode_ids(s)):
        case Success(value=value):
            return nested_equal(f"simple.tokens.bytes[{i}]", value, atl(Expected.SIMPLE_TOKENS_BYTES, i))
        case Error(message=message):
            return Check.failed(f"simple.tokens.bytes[{i}]", message)

def bpe_case(i):
    match Educational.bpe_encode(trained_ranks(), atl(Expected.BPE_PIECES, i)):
        case Success(value=value):
            return Check.ints_equal(f"bpe[{i}]", value, atl(Expected.BPE_IDS, i))
        case Error(message=message):
            return Check.failed(f"bpe[{i}]", message)

# Suite assembly

def split_cases():
    return listConcat(cases(len(Expected.SPLIT_INPUTS), split_gpt2_case), cases(len(Expected.SPLIT_INPUTS), split_cl100k_case))

def class_cases():
    return cases(len(Expected.CLASS_CPS), class_case)

def train_cases():
    return [train_gpt2_case(), train_cl100k_case(), train_edge_case(), train_small_case()]

def encode_cases(enc):
    return concat([
        cases(len(Expected.ENC_TEXTS), lambda i: encode_ordinary_case(enc, i)),
        cases(len(Expected.ENC_ALL_TEXTS), lambda i: encode_all_case(enc, i)),
        cases(len(Expected.ENC_ONE_TEXTS), lambda i: encode_one_case(enc, i)),
        cases(len(Expected.ENC_NONE_TEXTS), lambda i: encode_none_case(enc, i)),
    ])

def decode_cases(enc):
    return concat([
        cases(len(Expected.DECODE_SEQS), lambda i: decode_replace_case(enc, i)),
        cases(len(Expected.DECODE_SEQS), lambda i: decode_ignore_case(enc, i)),
        cases(len(Expected.DECODE_SEQS), lambda i: decode_strict_case(enc, i)),
        cases(len(Expected.DECODE_NUL_SEQS), lambda i: decode_nul_case(enc, i)),
        cases(len(Expected.DECODE_UNKNOWN_SEQS), lambda i: decode_unknown_case(enc, i)),
        cases(len(Expected.DECODE_BYTES_CASES), lambda i: decode_bytes_case(enc, i)),
    ])

def single_cases(enc):
    return concat([
        cases(len(Expected.SINGLE_TOKEN_TEXTS), lambda i: single_token_case(enc, i)),
        cases(len(Expected.SINGLE_BYTES_TOKENS), lambda i: single_bytes_case(enc, i)),
        cases(len(Expected.TOKENS_BYTES_SEQS), lambda i: tokens_bytes_case(enc, i)),
    ])

def enc_tail_cases(enc):
    return concat([
        vocab_cases(enc),
        cases(len(Expected.IS_SPECIAL), lambda i: is_special_case(enc, i)),
        special_names_case(enc),
    ])

def trunc_cases():
    return concat([
        cases(len(Expected.GPT2_TRUNC_TEXTS), gpt2_trunc_ordinary_case),
        cases(len(Expected.GPT2_TRUNC_TEXTS), gpt2_trunc_special_case),
        cases(len(Expected.CL100K_TRUNC_TEXTS), cl100k_trunc_ordinary_case),
        cases(len(Expected.CL100K_TRUNC_TEXTS), cl100k_trunc_special_case),
    ])

def simple_cases():
    return concat([
        cases(len(Expected.SIMPLE_TEXTS), simple_ids_case),
        cases(len(Expected.SIMPLE_DECODE_TEXTS), simple_decode_ids_case),
        cases(len(Expected.SIMPLE_DECODE_TEXTS), simple_decode_case),
        cases(len(Expected.SIMPLE_DECODE_TEXTS), simple_decode_bytes_case),
        cases(len(Expected.SIMPLE_DECODE_TEXTS), simple_tokens_bytes_case),
    ])

def suite():
    enc = trained_enc()
    return concat([
        split_cases(), class_cases(), train_cases(), encode_cases(enc),
        decode_cases(enc), single_cases(enc), enc_tail_cases(enc),
        cases(len(Expected.OVERLAP_TEXTS), overlap_case),
        trunc_cases(), simple_cases(),
        cases(len(Expected.BPE_PIECES), bpe_case), Contracts.cases(),
    ])

def report_suite():
    return Check.report(suite())

def main():
    match Check.strictly(lambda: report_suite()):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
