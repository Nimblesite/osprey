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

def nested_equal(name, got, want):
    """Exact equality of two lists of lists, naming the first differing index."""
    i = fold(range(len(want)), -1, lambda found, k: found if found >= 0 or structuralEquals(at(got, k, []), at(want, k, [])) else k)
    return length_failed(name, got, want) if len(got) != len(want) else Check.passed(name) if i < 0 else Check.failed(name, f"index {i}: got {Unicode.ints_text(at(got, i, []))}, want {Unicode.ints_text(at(want, i, []))}")

def strings_equal(name, got, want):
    """Exact equality of two string lists, naming the first differing index."""
    i = fold(range(len(want)), -1, lambda found, k: found if found >= 0 or at(got, k, "") == at(want, k, "") else k)
    return length_failed(name, got, want) if len(got) != len(want) else Check.passed(name) if i < 0 else Check.failed(name, f"index {i}: got \"{at(got, i, "")}\", want \"{at(want, i, "")}\"")

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

def ordinary_ids_case(label, enc, texts, want, i):
    """encode_ordinary of text i, compared against want ids."""
    return result_case(f"{label}[{i}]", Core.encode_ordinary(enc, text(at(texts, i, []))), Check.ints_equal, at(want, i, []))

def special_ids_case(label, enc, texts, specials, want, i):
    """encode with specials of text i, compared against want ids."""
    return result_case(f"{label}[{i}]", Core.encode(enc, text(at(texts, i, [])), specials), Check.ints_equal, at(want, i, []))

def encode_ordinary_case(enc, i):
    return ordinary_ids_case("encode.ordinary", enc, Expected.ENC_TEXTS, Expected.ENC_ORDINARY, i)

def encode_all_case(enc, i):
    return special_ids_case("encode.all", enc, Expected.ENC_ALL_TEXTS, ALL_SPECIALS, Expected.ENC_ALL_IDS, i)

def bytes_truth_case(name, result, want_error, want):
    """A `Result` byte-list case: success needs no wanted error and exactly `want`."""
    return truth_case(name, result, want_error, lambda value: structuralEquals(value, want), lambda value: f"got {Unicode.ints_text(value)}")

def encode_one_case(enc, i):
    return bytes_truth_case(f"encode.one[{i}]", Core.encode(enc, text(at(Expected.ENC_ONE_TEXTS, i, [])), ["<|endoftext|>"]), at(Expected.ENC_ONE_ERROR, i, False), at(Expected.ENC_ONE_IDS, i, []))

def encode_none_case(enc, i):
    want_error = at(Expected.ENC_NONE_ERROR, i, False)
    want_token = at(Expected.ENC_NONE_TOKEN, i, "")
    match Core.encode(enc, text(at(Expected.ENC_NONE_TEXTS, i, [])), []):
        case Success(value=value):
            return Check.truth(f"encode.none[{i}]", not want_error, f"got {Unicode.ints_text(value)}")
        case Error(message=message):
            return Check.truth(f"encode.none[{i}]", want_error and disallowed_token(message) == want_token, message)

# Decoding

def decode_replace_case(enc, i):
    return result_case(f"decode.replace[{i}]", Core.decode(enc, at(Expected.DECODE_SEQS, i, [])), Check.equal, at(Expected.DECODE_REPLACE, i, ""))

def decode_ignore_case(enc, i):
    return result_case(f"decode.ignore[{i}]", Core.decode_errors(enc, at(Expected.DECODE_SEQS, i, []), "ignore"), Check.equal, at(Expected.DECODE_IGNORE, i, ""))

def decode_strict_case(enc, i):
    want_ok = at(Expected.DECODE_STRICT_OK, i, False)
    return truth_case(f"decode.strict[{i}]", Core.decode_errors(enc, at(Expected.DECODE_SEQS, i, []), "strict"), not want_ok, lambda value: True, lambda value: "expected a strict error")

def refused_decode_case(label, why, seqs, enc, i):
    """Decoding sequence list i must fail, reported as `why` when it answers."""
    return refused(f"{label}[{i}]", why, Core.decode(enc, at(seqs, i, [])))

def decode_bytes_case(enc, i):
    return result_case(f"decode.bytes[{i}]", Core.decode_bytes(enc, at(Expected.DECODE_BYTES_CASES, i, [])), Check.ints_equal, at(Expected.DECODE_BYTES, i, []))

# Single tokens and token byte values

def single_token_case(enc, i):
    return truth_case(f"single.token[{i}]", Core.encode_single_token(enc, at(Expected.SINGLE_TOKEN_TEXTS, i, "")), at(Expected.SINGLE_TOKEN_ERROR, i, False), lambda value: value == at(Expected.SINGLE_TOKEN_IDS, i, 0), lambda value: f"got {value}")

def single_bytes_case(enc, i):
    return bytes_truth_case(f"single.bytes[{i}]", Core.decode_single_token_bytes(enc, at(Expected.SINGLE_BYTES_TOKENS, i, 0)), at(Expected.SINGLE_BYTES_ERROR, i, False), at(Expected.SINGLE_BYTES, i, []))

def tokens_bytes_case(enc, i):
    return result_case(f"tokens.bytes[{i}]", Core.decode_tokens_bytes(enc, at(Expected.TOKENS_BYTES_SEQS, i, [])), nested_equal, at(Expected.TOKENS_BYTES, i, []))

def special_probe(i):
    """The token the oracle probed for is_special_token at index i."""
    return at([0, 255, Expected.TRAIN_VOCAB - 1, Expected.TRAIN_VOCAB, Expected.TRAIN_VOCAB + 2], i, 0) if i < 5 else Expected.TRAIN_VOCAB + 50

def is_special_case(enc, i):
    return Check.equal(f"is_special[{i}]", Core.is_special_token(enc, special_probe(i)), at(Expected.IS_SPECIAL, i, False))

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

def trunc_encoding_cases(label, enc, texts, ids, special_ids):
    """One truncated encoding: its ordinary ids cases, then its special-token ids cases."""
    return concat([
        cases(len(texts), lambda i: ordinary_ids_case(label, enc, texts, ids, i)),
        cases(len(texts), lambda i: special_ids_case(f"{label}.special", enc, texts, ["<|endoftext|>"], special_ids, i)),
    ])

# Educational codec and module-level bpe_encode

def simple_ids_case(i):
    return result_case(f"simple.ids[{i}]", Educational.encode(simple_enc(), text(at(Expected.SIMPLE_TEXTS, i, []))), Check.ints_equal, at(Expected.SIMPLE_IDS, i, []))

def encode_ids(s):
    """The educational codec's ids for a string, [] on error."""
    match Educational.encode(simple_enc(), s):
        case Success(value=value):
            return value
        case Error(message=message):
            return []

def simple_decode_ids_case(i):
    return Check.ints_equal(f"simple.decode.ids[{i}]", simple_text_ids(i), at(Expected.SIMPLE_DECODE_IDS, i, []))

def simple_text_ids(i):
    """Text i of the decode inputs with its simple-codec ids."""
    return encode_ids(at(Expected.SIMPLE_DECODE_TEXTS, i, ""))

def simple_decode_case(i):
    return result_case(f"simple.decode[{i}]", Educational.decode(simple_enc(), simple_text_ids(i)), Check.equal, at(Expected.SIMPLE_DECODE, i, ""))

def simple_decode_bytes_case(i):
    return result_case(f"simple.decode.bytes[{i}]", Educational.decode_bytes(simple_enc(), simple_text_ids(i)), Check.ints_equal, at(Expected.SIMPLE_DECODE_BYTES, i, []))

def simple_tokens_bytes_case(i):
    return result_case(f"simple.tokens.bytes[{i}]", Educational.decode_tokens_bytes(simple_enc(), simple_text_ids(i)), nested_equal, at(Expected.SIMPLE_TOKENS_BYTES, i, []))

def bpe_case(i):
    return result_case(f"bpe[{i}]", Educational.bpe_encode(trained_ranks(), at(Expected.BPE_PIECES, i, [])), Check.ints_equal, at(Expected.BPE_IDS, i, []))

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
        cases(len(Expected.DECODE_NUL_SEQS), lambda i: refused_decode_case("decode.nul", "expected an error for NUL", Expected.DECODE_NUL_SEQS, enc, i)),
        cases(len(Expected.DECODE_UNKNOWN_SEQS), lambda i: refused_decode_case("decode.unknown", "expected an unknown-token error", Expected.DECODE_UNKNOWN_SEQS, enc, i)),
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
        trunc_encoding_cases("gpt2.trunc", gpt2_trunc_enc(), Expected.GPT2_TRUNC_TEXTS, Expected.GPT2_TRUNC_IDS, Expected.GPT2_TRUNC_SPECIAL_IDS),
        trunc_encoding_cases("cl100k.trunc", cl100k_trunc_enc(), Expected.CL100K_TRUNC_TEXTS, Expected.CL100K_TRUNC_IDS, Expected.CL100K_TRUNC_SPECIAL_IDS),
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
