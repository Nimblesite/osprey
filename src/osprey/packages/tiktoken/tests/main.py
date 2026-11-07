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

def checked(name, result, compare):
    """A `Result` case: `compare name value` when it answered, the reported failure otherwise."""
    match result:
        case Success(value=value):
            return compare(name, value)
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

def split_case(label, pattern, want, i):
    """Pattern.split of split input i, flattened and compared against want."""
    return Check.ints_equal(f"{label}[{i}]", flatten(Pattern.split(pattern, text(atl(Expected.SPLIT_INPUTS, i)))), atl(want, i))

def class_case(i):
    cp = ati(Expected.CLASS_CPS, i)
    return Check.equal(f"class[{cp}]", Unicode.class_of(cp), ati(Expected.CLASS_KINDS, i))

# Training

def train_case(label, vocab, pattern, want):
    """bpe_train of the corpus, its merge list compared against want."""
    return checked(f"{label}.merges", Educational.bpe_train(Expected.CORPUS, vocab, pattern), lambda name, value: nested_equal(name, Bpe.merges_of(value), want))

# Encoding (ordinary and special-token aware)

def ordinary_ids_case(label, enc, texts, want, i):
    """encode_ordinary of text i, compared against want ids."""
    return result_case(f"{label}[{i}]", Core.encode_ordinary(enc, text(atl(texts, i))), Check.ints_equal, atl(want, i))

def special_ids_case(label, enc, texts, specials, want, i):
    """encode with specials of text i, compared against want ids."""
    return result_case(f"{label}[{i}]", Core.encode(enc, text(atl(texts, i)), specials), Check.ints_equal, atl(want, i))

def encode_ordinary_case(enc, i):
    return ordinary_ids_case("encode.ordinary", enc, Expected.ENC_TEXTS, Expected.ENC_ORDINARY, i)

def encode_all_case(enc, i):
    return special_ids_case("encode.all", enc, Expected.ENC_ALL_TEXTS, ALL_SPECIALS, Expected.ENC_ALL_IDS, i)

def encode_one_case(enc, i):
    return truth_case(f"encode.one[{i}]", Core.encode(enc, text(atl(Expected.ENC_ONE_TEXTS, i)), ["<|endoftext|>"]), atb(Expected.ENC_ONE_ERROR, i), lambda value: structuralEquals(value, atl(Expected.ENC_ONE_IDS, i)), lambda value: f"got {Unicode.ints_text(value)}")

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
    return result_case(f"decode.replace[{i}]", Core.decode(enc, atl(Expected.DECODE_SEQS, i)), Check.equal, ats(Expected.DECODE_REPLACE, i))

def decode_ignore_case(enc, i):
    return result_case(f"decode.ignore[{i}]", Core.decode_errors(enc, atl(Expected.DECODE_SEQS, i), "ignore"), Check.equal, ats(Expected.DECODE_IGNORE, i))

def decode_strict_case(enc, i):
    want_ok = atb(Expected.DECODE_STRICT_OK, i)
    return truth_case(f"decode.strict[{i}]", Core.decode_errors(enc, atl(Expected.DECODE_SEQS, i), "strict"), not want_ok, lambda value: True, lambda value: "expected a strict error")

def decode_nul_case(enc, i):
    return refused(f"decode.nul[{i}]", "expected an error for NUL", Core.decode(enc, atl(Expected.DECODE_NUL_SEQS, i)))

def decode_unknown_case(enc, i):
    return refused(f"decode.unknown[{i}]", "expected an unknown-token error", Core.decode(enc, atl(Expected.DECODE_UNKNOWN_SEQS, i)))

def decode_bytes_case(enc, i):
    return result_case(f"decode.bytes[{i}]", Core.decode_bytes(enc, atl(Expected.DECODE_BYTES_CASES, i)), Check.ints_equal, atl(Expected.DECODE_BYTES, i))

# Single tokens and token byte values

def single_token_case(enc, i):
    return truth_case(f"single.token[{i}]", Core.encode_single_token(enc, ats(Expected.SINGLE_TOKEN_TEXTS, i)), atb(Expected.SINGLE_TOKEN_ERROR, i), lambda value: value == ati(Expected.SINGLE_TOKEN_IDS, i), lambda value: f"got {value}")

def single_bytes_case(enc, i):
    return truth_case(f"single.bytes[{i}]", Core.decode_single_token_bytes(enc, ati(Expected.SINGLE_BYTES_TOKENS, i)), atb(Expected.SINGLE_BYTES_ERROR, i), lambda value: structuralEquals(value, atl(Expected.SINGLE_BYTES, i)), lambda value: f"got {Unicode.ints_text(value)}")

def tokens_bytes_case(enc, i):
    return result_case(f"tokens.bytes[{i}]", Core.decode_tokens_bytes(enc, atl(Expected.TOKENS_BYTES_SEQS, i)), nested_equal, atl(Expected.TOKENS_BYTES, i))

def special_probe(i):
    """The token the oracle probed for is_special_token at index i."""
    return ati([0, 255, Expected.TRAIN_VOCAB - 1, Expected.TRAIN_VOCAB, Expected.TRAIN_VOCAB + 2], i) if i < 5 else Expected.TRAIN_VOCAB + 50

def is_special_case(enc, i):
    return Check.equal(f"is_special[{i}]", Core.is_special_token(enc, special_probe(i)), atb(Expected.IS_SPECIAL, i))

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

# Overlap and truncation encodings

def overlap_case(i):
    return special_ids_case("overlap", overlap_enc(), Expected.OVERLAP_TEXTS, ["<|a|>", "<|a|>x", "<|b|>"], Expected.OVERLAP_IDS, i)

def gpt2_trunc_ordinary_case(i):
    return ordinary_ids_case("gpt2.trunc", gpt2_trunc_enc(), Expected.GPT2_TRUNC_TEXTS, Expected.GPT2_TRUNC_IDS, i)

def gpt2_trunc_special_case(i):
    return special_ids_case("gpt2.trunc.special", gpt2_trunc_enc(), Expected.GPT2_TRUNC_TEXTS, ["<|endoftext|>"], Expected.GPT2_TRUNC_SPECIAL_IDS, i)

def cl100k_trunc_ordinary_case(i):
    return ordinary_ids_case("cl100k.trunc", cl100k_trunc_enc(), Expected.CL100K_TRUNC_TEXTS, Expected.CL100K_TRUNC_IDS, i)

def cl100k_trunc_special_case(i):
    return special_ids_case("cl100k.trunc.special", cl100k_trunc_enc(), Expected.CL100K_TRUNC_TEXTS, ["<|endoftext|>"], Expected.CL100K_TRUNC_SPECIAL_IDS, i)

# Educational codec and module-level bpe_encode

def simple_ids_case(i):
    return result_case(f"simple.ids[{i}]", Educational.encode(simple_enc(), text(atl(Expected.SIMPLE_TEXTS, i))), Check.ints_equal, atl(Expected.SIMPLE_IDS, i))

def encode_ids(s):
    """The educational codec's ids for a string, [] on error."""
    match Educational.encode(simple_enc(), s):
        case Success(value=value):
            return value
        case Error(message=message):
            return []

def simple_decode_ids_case(i):
    return Check.ints_equal(f"simple.decode.ids[{i}]", simple_text_ids(i), atl(Expected.SIMPLE_DECODE_IDS, i))

def simple_text_ids(i):
    """Text i of the decode inputs with its simple-codec ids."""
    return encode_ids(ats(Expected.SIMPLE_DECODE_TEXTS, i))

def simple_decode_case(i):
    return result_case(f"simple.decode[{i}]", Educational.decode(simple_enc(), simple_text_ids(i)), Check.equal, ats(Expected.SIMPLE_DECODE, i))

def simple_decode_bytes_case(i):
    return result_case(f"simple.decode.bytes[{i}]", Educational.decode_bytes(simple_enc(), simple_text_ids(i)), Check.ints_equal, atl(Expected.SIMPLE_DECODE_BYTES, i))

def simple_tokens_bytes_case(i):
    return result_case(f"simple.tokens.bytes[{i}]", Educational.decode_tokens_bytes(simple_enc(), simple_text_ids(i)), nested_equal, atl(Expected.SIMPLE_TOKENS_BYTES, i))

def bpe_case(i):
    return result_case(f"bpe[{i}]", Educational.bpe_encode(trained_ranks(), atl(Expected.BPE_PIECES, i)), Check.ints_equal, atl(Expected.BPE_IDS, i))

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

def main():
    return Check.run(suite)
