# osprey: flavor=python
"""@osprey/tokenizers' suite: every encode, decode, pre-tokenization and
normalization compared with the real `tokenizers` package's output
(src/osprey/packages/tokenizers/oracle/tokenizers.cpython generates tests/expected.py)."""
from testing import Check
from tokenizers_expected import Expected
from tokenizers.Decoders import byte_fallback_decode_chain, metaspace_decode_chain, utf8_decode, wordpiece_decode
from tokenizers.Models import (
    Token, build_unigram, build_wordlevel, build_wordpiece, char_starts,
    id_to_token, unigram_encode, unigram_tokenize, wordlevel_tokenize, wordpiece_tokenize,
)
from tokenizers.Normalizers import Norm, append_norm, bert_normalize, char_at, empty_norm, end_at, norm_of, norm_text, prepend_cp, replace_space, start_at
from tokenizers.PreTokenizers import (
    META_CP, bert_pretokenize, char_delimiter_split, digits_pretokenize,
    metaspace_pretokenize, punctuation_pretokenize, whitespace_pretokenize, whitespace_split,
)
from tokenizers.Processors import Encoding, Offset, bert_template, concat_two, empty_encoding
from tokenizers.Tokenizer import (
    byte_char_map, char_of, unigram_decode_pipeline, unigram_encode_pipeline,
    wordlevel_decode_pipeline, wordlevel_encode_pipeline, wordpiece_decode_pipeline,
    wordpiece_encode, wordpiece_encode_pair,
)
from tokenizers.Category import LETTER_RANGES, MARK_RANGES, NUMBER_RANGES, is_letter, is_mark, is_number
from tokenizers.HfJson import load_qwen_tokenizer
from tokenizers import GgufVocab, NfcData
from tokenizers.Models import BpeModel, build_bpe, bpe_tokenize, Token as ModelToken
from tokenizers.Nfc import HANGUL_SBASE, HANGUL_SCOUNT, canonical_order, combining_class, compose, composite, decompose, decompose_cp, nfc_cps, nfc_text, nfd_cps, nfd_text
from tokenizers.Normalizers import nfc_normalize
from tokenizers.PreTokenizers import BYTE_CHARS, QWEN_PATTERN, byte_of_char, bytelevel_piece, bytelevel_raw, bytelevel_text, qwen_piece_len, qwen_pretokenize, qwen_split
from tokenizers.Decoders import bytelevel_decode
from tokenizers.Tokenizer import AddedToken, QwenTokenizer, qwen_decode, qwen_encode
from tokenizers.Unicode import (
    cps_text, in_ranges, is_chinese_char, is_control, is_numeric, is_punctuation,
    is_whitespace, is_word_char, lowercase, strip_base, text_cp_starts, text_cps,
)


def atl(xs, i):
    """The list at `i`, [] past the end."""
    return unwrap_or(listGet(xs, i), [])


def ats(xs, i):
    """The string at `i`, "" past the end."""
    return unwrap_or(listGet(xs, i), "")


def atb(xs, i):
    """The bool at `i`, false past the end."""
    return unwrap_or(listGet(xs, i), False)


def ati(xs, i):
    """The int at `i`, 0 past the end."""
    return unwrap_or(listGet(xs, i), 0)


def offs_list(enc):
    offs = enc.offsets
    return toList(map(range(len(offs)), lambda i: pair(unwrap_or(listGet(offs, i), Offset(start=0, end=0)))))


def pair(o):
    return [o.start, o.end]


def concat_all(groups):
    """One flat list of cases from a list of case groups."""
    return fold(range(len(groups)), [], lambda acc, i: listConcat(acc, atl(groups, i)))


def strs_text(xs):
    return "[" + join(toList(map(range(len(xs)), lambda i: unwrap_or(listGet(xs, i), "?"))), ", ") + "]"


def off_text(o):
    return "[" + toString(unwrap_or(listGet(o, 0), 0)) + "," + toString(unwrap_or(listGet(o, 1), 0)) + "]"


def offs_text(xs):
    return "[" + join(toList(map(range(len(xs)), lambda i: off_text(unwrap_or(listGet(xs, i), [0, 0])))), ", ") + "]"


def strs_equal(name, got, want):
    return Check.truth(name, structuralEquals(got, want), f"got {strs_text(got)}, want {strs_text(want)}")


def offs_equal(name, got, want):
    return Check.truth(name, structuralEquals(got, want), f"got {offs_text(got)}, want {offs_text(want)}")


def core_cases(name, enc, want_ids, want_tokens, want_offsets):
    return [
        Check.ints_equal(name + ".ids", enc.ids, want_ids),
        strs_equal(name + ".tokens", enc.tokens, want_tokens),
        offs_equal(name + ".offsets", offs_list(enc), want_offsets),
    ]


def wp_enc_cases(name, enc, want_ids, want_tokens, want_offsets, want_type_ids):
    return listConcat(core_cases(name, enc, want_ids, want_tokens, want_offsets), [Check.ints_equal(name + ".type_ids", enc.type_ids, want_type_ids)])


def wp_case(wp, cls_id, sep_id, i):
    text = ats(Expected.WP_INPUTS, i)
    enc = wordpiece_encode(wp, text, cls_id, sep_id)
    return listConcat(
        wp_enc_cases(f"wp.{i}", enc, atl(Expected.WP_IDS, i), atl(Expected.WP_TOKENS, i), atl(Expected.WP_OFFSETS, i), atl(Expected.WP_TYPE_IDS, i)),
        [Check.equal(f"wp.{i}.decode", wordpiece_decode_pipeline(wp, enc.ids), ats(Expected.WP_DECODES, i))],
    )


def wp_pair_case(wp, cls_id, sep_id, i):
    a = ats(Expected.WP_PAIR_A, i)
    b = ats(Expected.WP_PAIR_B, i)
    enc = wordpiece_encode_pair(wp, a, b, cls_id, sep_id)
    return wp_enc_cases(f"wp.pair.{i}", enc, atl(Expected.WP_PAIR_IDS, i), atl(Expected.WP_PAIR_TOKENS, i), atl(Expected.WP_PAIR_OFFSETS, i), atl(Expected.WP_PAIR_TYPE_IDS, i))


def uni_case(m, tag, byte_fallback_mode, i, ids_c, tokens_c, offsets_c, decodes_c):
    text = ats(Expected.UNI_INPUTS, i)
    enc = unigram_encode_pipeline(m, text)
    return listConcat(
        core_cases(f"{tag}.{i}", enc, atl(ids_c, i), atl(tokens_c, i), atl(offsets_c, i)),
        [Check.equal(f"{tag}.{i}.decode", unigram_decode_pipeline(m, enc.ids, byte_fallback_mode), ats(decodes_c, i))],
    )


def uni_cases(m, tag, byte_fallback_mode, ids_c, tokens_c, offsets_c, decodes_c):
    return fold(range(len(Expected.UNI_INPUTS)), [], lambda acc, i: listConcat(acc, uni_case(m, tag, byte_fallback_mode, i, ids_c, tokens_c, offsets_c, decodes_c)))


def wl_case(wl, i):
    text = ats(Expected.WL_INPUTS, i)
    enc = wordlevel_encode_pipeline(wl, text)
    return listConcat(
        core_cases(f"wl.{i}", enc, atl(Expected.WL_IDS, i), atl(Expected.WL_TOKENS, i), atl(Expected.WL_OFFSETS, i)),
        [Check.equal(f"wl.{i}.decode", wordlevel_decode_pipeline(wl, enc.ids), ats(Expected.WL_DECODES, i))],
    )


def pretok_offsets(text, splits):
    byte_char = byte_char_map(text)
    nchars = len(char_starts(text))
    return toList(map(range(len(splits)), lambda k: split_off(unwrap_or(listGet(splits, k), Norm(chars=[], starts=[], ends=[])), byte_char, nchars)))


def split_off(n, byte_char, nchars):
    s = unwrap_or(listGet(n.starts, 0), 0)
    e = unwrap_or(listGet(n.ends, len(n.chars) - 1), 0)
    return [char_of(byte_char, nchars, s), char_of(byte_char, nchars, e)]


def pretok_case(name, split_fn, want_tokens_c, want_offsets_c, i):
    text = ats(Expected.PRE_INPUTS, i)
    splits = split_fn(norm_of(text))
    toks = toList(map(range(len(splits)), lambda k: norm_text(unwrap_or(listGet(splits, k), Norm(chars=[], starts=[], ends=[])))))
    return [
        strs_equal(f"{name}.{i}.tokens", toks, atl(want_tokens_c, i)),
        offs_equal(f"{name}.{i}.offsets", pretok_offsets(text, splits), atl(want_offsets_c, i)),
    ]


def pretok_cases(name, split_fn, want_tokens_c, want_offsets_c):
    return fold(range(len(Expected.PRE_INPUTS)), [], lambda acc, i: listConcat(acc, pretok_case(name, split_fn, want_tokens_c, want_offsets_c, i)))


BERT_MODES = ["default", "clean", "lower", "strip", "chinese"]
"""The `bert_normalize` flag combinations, each named after the pass it leaves on."""
BERT_FLAGS = [[True, True, True, True], [True, False, False, False], [False, False, False, True], [False, False, True, False], [False, True, False, False]]
"""One flag row per mode: clean, chinese, strip-accents and lowercase, in that order."""
BERT_WANTS = [Expected.NORM_DEFAULT, Expected.NORM_CLEAN_ONLY, Expected.NORM_LOWER_ONLY, Expected.NORM_STRIP_ONLY, Expected.NORM_CHINESE_ONLY]


def norm_case(i):
    """Every `bert_normalize` mode's output for one input."""
    text = ats(Expected.NORM_INPUTS, i)
    return toList(map(range(len(BERT_MODES)), lambda k: Check.equal(f"norm.{i}.{ats(BERT_MODES, k)}", norm_mode(text, atl(BERT_FLAGS, k)), unwrap_or(listGet(atl(BERT_WANTS, k), i), ""))))


def norm_mode(text, flags):
    """`bert_normalize` under one row of `BERT_FLAGS`."""
    return norm_text(bert_normalize(text, unwrap_or(listGet(flags, 0), False), unwrap_or(listGet(flags, 1), False), unwrap_or(listGet(flags, 2), False), unwrap_or(listGet(flags, 3), False)))


def uni_pred_case(name, pred, want_c):
    return fold(range(len(Expected.UNI_CPS)), [], lambda acc, i: listAppend(acc, Check.equal(f"{name}.{i}", pred(ati(Expected.UNI_CPS, i)), atb(want_c, i))))


def uni_list_case(name, f, want_c):
    return fold(range(len(Expected.UNI_CPS)), [], lambda acc, i: listAppend(acc, Check.ints_equal(f"{name}.{i}", f(ati(Expected.UNI_CPS, i)), atl(want_c, i))))


def unicode_cases():
    """The Unicode character classes and case/accent tables, per codepoint."""
    return concat_all([
        uni_pred_case("uni.ws", is_whitespace, Expected.UNI_IS_WS),
        uni_pred_case("uni.punct", is_punctuation, Expected.UNI_IS_PUNCT),
        uni_pred_case("uni.num", is_numeric, Expected.UNI_IS_NUMERIC),
        uni_pred_case("uni.word", is_word_char, Expected.UNI_IS_WORD),
        uni_pred_case("uni.ctrl", is_control, Expected.UNI_IS_CONTROL),
        uni_pred_case("uni.cjk", is_chinese_char, Expected.UNI_IS_CHINESE),
        uni_list_case("uni.lower", lowercase, Expected.UNI_LOWER),
        uni_list_case("uni.strip", strip_base, Expected.UNI_STRIP_BASE),
    ])


def empty_token():
    """The token the accessors report for an index past the end."""
    return Token(id=0, value="", start=0, end=0)


def token_field(toks, get):
    """One field of every token, `get` picking it, the empty token past the end."""
    return toList(map(range(len(toks)), lambda k: get(unwrap_or(listGet(toks, k), empty_token()))))


def token_ids(toks):
    return token_field(toks, lambda token: token.id)


def token_values(toks):
    return token_field(toks, lambda token: token.value)


def token_starts(toks):
    return token_field(toks, lambda token: token.start)


def token_ends(toks):
    return token_field(toks, lambda token: token.end)


def model_row_case(name, toks, want_ids, want_values, want_starts, want_ends):
    return [
        Check.ints_equal(name + ".ids", token_ids(toks), want_ids),
        strs_equal(name + ".values", token_values(toks), want_values),
        Check.ints_equal(name + ".starts", token_starts(toks), want_starts),
        Check.ints_equal(name + ".ends", token_ends(toks), want_ends),
    ]


def model_rows_case(name, rows_fn, texts_c, ids_c, values_c, starts_c, ends_c):
    return fold(range(len(ids_c)), [], lambda acc, i: listConcat(acc, model_row_case(f"{name}.{i}", rows_fn(ats(texts_c, i)), atl(ids_c, i), atl(values_c, i), atl(starts_c, i), atl(ends_c, i))))


def wp_model_cases(wp):
    return model_rows_case("mwp", lambda w: wordpiece_tokenize(wp, w), Expected.MODEL_WP_WORDS, Expected.MODEL_WP_TOK_IDS, Expected.MODEL_WP_TOK_VALUES, Expected.MODEL_WP_TOK_STARTS, Expected.MODEL_WP_TOK_ENDS)


def uni_model_cases(uni_nobf):
    return model_rows_case("muni", lambda s: unigram_tokenize(uni_nobf, s), Expected.MODEL_UNI_TEXTS, Expected.MODEL_UNI_IDS, Expected.MODEL_UNI_VALUES, Expected.MODEL_UNI_STARTS, Expected.MODEL_UNI_ENDS)


def wl_model_cases(wl):
    return model_rows_case("mwl", lambda s: wordlevel_tokenize(wl, s), Expected.MODEL_WL_TEXTS, Expected.MODEL_WL_IDS, Expected.MODEL_WL_VALUES, Expected.MODEL_WL_STARTS, Expected.MODEL_WL_ENDS)


def uni_encode_case(uni_nobf, i):
    s = ats(Expected.MODEL_UNI_TEXTS, i)
    return strs_equal(f"muni.encode.{i}", unigram_encode(uni_nobf, s), atl(Expected.MODEL_UNI_VALUES, i))


def uni_encode_cases(uni_nobf):
    return fold(range(len(Expected.MODEL_UNI_TEXTS)), [], lambda acc, i: listAppend(acc, uni_encode_case(uni_nobf, i)))


def id_to_token_case(wp, i):
    id = ati(Expected.MODEL_WP_IDS, i)
    return Check.equal(f"mwp.id_to_token.{i}", id_to_token(wp.tokens, id), ats(Expected.MODEL_WP_ID_TO_TOKEN, i))


def id_to_token_cases(wp):
    return fold(range(len(Expected.MODEL_WP_IDS)), [], lambda acc, i: listAppend(acc, id_to_token_case(wp, i)))


def decode_case(name, dec_fn, toks_c, want_c, i):
    toks = atl(toks_c, i)
    return Check.equal(f"{name}.{i}", dec_fn(toks), ats(want_c, i))


def decoder_cases():
    """WordPiece, Metaspace and ByteFallback decoders on token lists."""
    wp_cases = fold(range(len(Expected.DECODE_WP)), [], lambda acc, i: listAppend(acc, decode_case("dec.wp", lambda t: wordpiece_decode(t, "##", True), Expected.DECODE_WP_TOKENS, Expected.DECODE_WP, i)))
    meta_cases = fold(range(len(Expected.DECODE_META)), [], lambda acc, i: listAppend(acc, decode_case("dec.meta", lambda t: join(metaspace_decode_chain(t, META_CP, 0), ""), Expected.DECODE_META_TOKENS, Expected.DECODE_META, i)))
    bf_cases = fold(range(len(Expected.DECODE_BF)), [], lambda acc, i: listAppend(acc, decode_case("dec.bf", lambda t: join(byte_fallback_decode_chain(t), ""), Expected.DECODE_BF_TOKENS, Expected.DECODE_BF, i)))
    return concat_all([wp_cases, meta_cases, bf_cases])


def meta_helper_cases():
    """Metaspace's replacement and prepend primitives on a Norm."""
    repl = fold(range(len(Expected.META_HELPER_INPUTS)), [], lambda acc, i: listAppend(acc, Check.equal(f"norm.replace.{i}", norm_text(replace_space(norm_of(ats(Expected.META_HELPER_INPUTS, i)), META_CP)), ats(Expected.META_REPLACE, i))))
    prep = fold(range(len(Expected.META_PREPEND_INPUTS)), [], lambda acc, i: listAppend(acc, Check.equal(f"norm.prepend.{i}", norm_text(prepend_cp(norm_of(ats(Expected.META_PREPEND_INPUTS, i)), META_CP)), ats(Expected.META_PREPEND, i))))
    return concat_all([repl, prep, alignment_cases()])


def alignment_cases():
    n = append_norm(empty_norm(), 233, 7, 9)
    return [
        Check.equal("norm.align.text", norm_text(n), "é"),
        Check.equal("norm.align.char", char_at(n, 0), 233),
        Check.equal("norm.align.start", start_at(n, 0), 7),
        Check.equal("norm.align.end", end_at(n, 0), 9),
        Check.equal("norm.align.empty.char", char_at(empty_norm(), 0), 0),
        Check.equal("norm.align.empty.start", start_at(empty_norm(), 0), 0),
        Check.equal("norm.align.empty.end", end_at(empty_norm(), 0), 0),
    ]


def encoding_of(ids, tokens, offsets):
    return Encoding(
        ids=ids,
        tokens=tokens,
        offsets=toList(map(range(len(offsets)), lambda i: Offset(start=unwrap_or(listGet(unwrap_or(listGet(offsets, i), [0, 0]), 0), 0), end=unwrap_or(listGet(unwrap_or(listGet(offsets, i), [0, 0]), 1), 0)))),
        type_ids=toList(map(range(len(ids)), lambda _: 0)),
    )


def processor_cases(cls_id, sep_id):
    """The BERT TemplateProcessing on raw single and pair encodings."""
    a = encoding_of(Expected.PROC_RAW_A_IDS, Expected.PROC_RAW_A_TOKENS, Expected.PROC_RAW_A_OFFSETS)
    b = encoding_of(Expected.PROC_RAW_B_IDS, Expected.PROC_RAW_B_TOKENS, Expected.PROC_RAW_B_OFFSETS)
    single = bert_template([a], cls_id, sep_id)
    pair = bert_template([a, b], cls_id, sep_id)
    return concat_all([
        wp_enc_cases("proc.single", single, Expected.PROC_SINGLE_IDS, Expected.PROC_SINGLE_TOKENS, Expected.PROC_SINGLE_OFFSETS, Expected.PROC_SINGLE_TYPE_IDS),
        wp_enc_cases("proc.pair", pair, Expected.PROC_PAIR_IDS, Expected.PROC_PAIR_TOKENS, Expected.PROC_PAIR_OFFSETS, Expected.PROC_PAIR_TYPE_IDS),
        [
            Check.truth("proc.empty.ids", structuralEquals(empty_encoding().ids, []), "empty ids"),
            Check.truth("proc.concat", structuralEquals(concat_two(empty_encoding(), empty_encoding()).tokens, []), "concat empties"),
        ],
    ])


def wordpiece_model():
    return build_wordpiece(Expected.WP_VOCAB, "[UNK]", "##", 100)


def unigram_model():
    return build_unigram(Expected.UNI_TOKENS, Expected.UNI_SCORES, Expected.UNI_UNK_ID, True)


def unigram_nobf_model():
    return build_unigram(Expected.UNI_NOBF_VOCAB, Expected.UNI_NOBF_SCORES, Expected.UNI_UNK_ID, False)


def wordlevel_model():
    return build_wordlevel(Expected.WL_VOCAB, "<unk>")


def cls_id_of(wp):
    return unwrap_or(mapGet(wp.token_to_id, "[CLS]"), 1)


def sep_id_of(wp):
    return unwrap_or(mapGet(wp.token_to_id, "[SEP]"), 2)


def encode_cases():
    wp = wordpiece_model()
    cls_id = cls_id_of(wp)
    sep_id = sep_id_of(wp)
    uni = unigram_model()
    uni_nobf = unigram_nobf_model()
    wl = wordlevel_model()
    wp_cases = fold(range(len(Expected.WP_INPUTS)), [], lambda acc, i: listConcat(acc, wp_case(wp, cls_id, sep_id, i)))
    pair_cases = fold(range(len(Expected.WP_PAIR_IDS)), [], lambda acc, i: listConcat(acc, wp_pair_case(wp, cls_id, sep_id, i)))
    uni_bf = uni_cases(uni, "uni", True, Expected.UNI_IDS, Expected.UNI_TOKENS_OUT, Expected.UNI_OFFSETS, Expected.UNI_DECODES)
    uni_no = uni_cases(uni_nobf, "uni.nobf", False, Expected.UNI_NOBF_IDS, Expected.UNI_NOBF_TOKENS, Expected.UNI_NOBF_OFFSETS, Expected.UNI_NOBF_DECODES)
    wl_cases = fold(range(len(Expected.WL_INPUTS)), [], lambda acc, i: listConcat(acc, wl_case(wl, i)))
    return concat_all([wp_cases, pair_cases, uni_bf, uni_no, wl_cases])


def pretokenizer_cases():
    pre_bert = pretok_cases("pre.bert", bert_pretokenize, Expected.PRE_BERT_TOKENS, Expected.PRE_BERT_OFFSETS)
    pre_ws = pretok_cases("pre.ws", whitespace_pretokenize, Expected.PRE_WS_TOKENS, Expected.PRE_WS_OFFSETS)
    pre_wss = pretok_cases("pre.wss", whitespace_split, Expected.PRE_WSS_TOKENS, Expected.PRE_WSS_OFFSETS)
    pre_punc = pretok_cases("pre.punc", punctuation_pretokenize, Expected.PRE_PUNC_TOKENS, Expected.PRE_PUNC_OFFSETS)
    pre_digits = pretok_cases("pre.digits", lambda n: digits_pretokenize(n, False), Expected.PRE_DIGITS_TOKENS, Expected.PRE_DIGITS_OFFSETS)
    pre_digits_ind = pretok_cases("pre.digits.ind", lambda n: digits_pretokenize(n, True), Expected.PRE_DIGITS_IND_TOKENS, Expected.PRE_DIGITS_IND_OFFSETS)
    pre_delim = pretok_cases("pre.delim", lambda n: char_delimiter_split(n, 44), Expected.PRE_DELIM_TOKENS, Expected.PRE_DELIM_OFFSETS)
    pre_meta = pretok_cases("pre.meta", lambda n: metaspace_pretokenize(n, META_CP, 0, True), Expected.PRE_META_TOKENS, Expected.PRE_META_OFFSETS)
    return concat_all([pre_bert, pre_ws, pre_wss, pre_punc, pre_digits, pre_digits_ind, pre_delim, pre_meta])


def normalizer_cases():
    return fold(range(len(Expected.NORM_INPUTS)), [], lambda acc, i: listConcat(acc, norm_case(i)))


def direct_cases():
    """Every other export: char classes, model.tokenize, decoders, primitives."""
    wp = wordpiece_model()
    cls_id = cls_id_of(wp)
    sep_id = sep_id_of(wp)
    uni_nobf = unigram_nobf_model()
    wl = wordlevel_model()
    return concat_all([
        unicode_cases(),
        wp_model_cases(wp),
        uni_model_cases(uni_nobf),
        wl_model_cases(wl),
        uni_encode_cases(uni_nobf),
        id_to_token_cases(wp),
        decoder_cases(),
        meta_helper_cases(),
        processor_cases(cls_id, sep_id),
    ])


def utf8_cases():
    """The UTF-8 decoder over long text and every malformed shape.

    A per-codepoint recursion overflows the stack here, so the long case is the
    regression: 150,000 three-byte characters in one byte list.
    """
    long_cps = toList(map(range(450000), wide_byte))
    long_text = cps_text(toList(map(range(150000), lambda i: 26085)))
    names = ["utf8.empty", "utf8.ascii", "utf8.two", "utf8.three", "utf8.four", "utf8.long"]
    byte_rows = [[], [104, 105], [195, 169], [230, 151, 165], [240, 159, 152, 128], long_cps]
    text_rows = ["", "hi", cps_text([233]), cps_text([26085]), cps_text([128512]), long_text]
    cases = fold(range(len(names)), [], lambda acc, i: listAppend(acc, utf8_text(ats(names, i), atl(byte_rows, i), ats(text_rows, i))))
    return listConcat(cases, [
        Check.truth("utf8.lone continuation", is_error(utf8_decode([128], 0, "")), "a lone continuation byte decoded"),
        Check.truth("utf8.truncated", is_error(utf8_decode([230, 151], 0, "")), "a truncated sequence decoded"),
        Check.truth("utf8.bad lead", is_error(utf8_decode([245], 0, "")), "0xF5 decoded"),
        Check.truth("utf8.overlong lead", is_error(utf8_decode([192, 128], 0, "")), "an overlong pair decoded"),
        Check.truth("utf8.surrogate", is_error(utf8_decode([237, 160, 128], 0, "")), "a surrogate decoded"),
        Check.truth("utf8.bad continuation", is_error(utf8_decode([230, 65, 165], 0, "")), "a bad continuation byte decoded"),
    ])


def wide_byte(i):
    """The i-th byte of a repeating three-byte sequence: U+65E5, 150,000 times."""
    return 230 if i % 3 == 0 else 151 if i % 3 == 1 else 165


def utf8_text(name, bytes_, want):
    match utf8_decode(bytes_, 0, ""):
        case Success(value=value):
            return Check.equal(name, value, want)
        case Error(message=message):
            return Check.failed(name, "Error: " + message)


def is_error(r):
    match r:
        case Success(value=_value):
            return False
        case Error(message=_message):
            return True



# ---------------------------------------------------------------------------
# [LLM-TOKENIZER]: NFC, the Qwen split, ByteLevel and byte-level BPE
# ---------------------------------------------------------------------------

def qwen_model():
    """The byte-level BPE model the oracle's subset vocabulary describes."""
    return unwrap_or(build_bpe(Expected.QWEN_BYTE_IDS, Expected.QWEN_MERGES_LEFT, Expected.QWEN_MERGES_RIGHT, Expected.QWEN_MERGE_IDS), empty_bpe())


def empty_bpe():
    return BpeModel(ranks={}, entries=[])


def qwen_added():
    return toList(map(range(len(Expected.QWEN_ADDED_CONTENT)), lambda i: AddedToken(
        content=at_str(Expected.QWEN_ADDED_CONTENT, i),
        id=at_int(Expected.QWEN_ADDED_IDS, i),
        special=at_int(Expected.QWEN_ADDED_SPECIAL, i) == 1,
        lstrip=False,
        rstrip=False,
        single_word=False,
    )))


def at_str(row, i):
    return ats(row, i)


def at_int(row, i):
    return unwrap_or(listGet(row, i), -1)


def qwen_tokenizer():
    return QwenTokenizer(model=qwen_model(), added=qwen_added())


def nfc_cases():
    """The reference's NFC and NFD for decomposed, precomposed, Hangul, Vietnamese,
    Devanagari, Thai, Hebrew and Arabic text, plus the combining-class ordering."""
    return fold(range(len(Expected.NFC_INPUTS)), unicode_table_cases(), lambda acc, i: listConcat(acc, nfc_row(i)))


def unicode_table_cases():
    return [
        Check.ints_equal("unicode.letter.range", [at_int(LETTER_RANGES, 0), at_int(LETTER_RANGES, 1)], [65, 90]),
        Check.ints_equal("unicode.mark.range", [at_int(MARK_RANGES, 0), at_int(MARK_RANGES, 1)], [768, 879]),
        Check.ints_equal("unicode.number.range", [at_int(NUMBER_RANGES, 0), at_int(NUMBER_RANGES, 1)], [48, 57]),
        Check.equal("unicode.ccc.aligned", len(NfcData.CCC_KEYS), len(NfcData.CCC_VALUES)),
        Check.equal("unicode.comp.aligned", len(NfcData.COMP_KEYS), len(NfcData.COMP_VALUES)),
        Check.equal("unicode.decomp.offsets", len(NfcData.DECOMP_OFFSETS), len(NfcData.DECOMP_KEYS) + 1),
        Check.equal("unicode.decomp.extent", at_int(NfcData.DECOMP_OFFSETS, len(NfcData.DECOMP_KEYS)), len(NfcData.DECOMP_CHARS)),
        Check.ints_equal("unicode.hangul.first", decompose_cp(HANGUL_SBASE), [4352, 4449]),
        Check.ints_equal("unicode.hangul.last", decompose_cp(HANGUL_SBASE + HANGUL_SCOUNT - 1), [4370, 4469, 4546]),
        Check.ints_equal("unicode.decompose", decompose([192, 98]), [65, 768, 98]),
        Check.ints_equal("unicode.canonical_order", canonical_order([65, 768, 807]), [65, 807, 768]),
        Check.equal("unicode.composite", composite(65, 768), 192),
        Check.ints_equal("unicode.compose", compose([65, 768, 98]), [192, 98]),
        Check.equal("gguf.kind.unused", GgufVocab.UNUSED, 5),
    ]


def nfc_row(i):
    text = ats(Expected.NFC_INPUTS, i)
    return [
        Check.equal(f"nfc.{i}", nfc_text(text), ats(Expected.NFC_EXPECTED, i)),
        Check.equal(f"nfd.{i}", nfd_text(text), ats(Expected.NFD_EXPECTED, i)),
        Check.equal(f"nfc.cps.{i}", cps_text(nfc_cps(text_cps(text))), ats(Expected.NFC_EXPECTED, i)),
        Check.equal(f"nfd.cps.{i}", cps_text(nfd_cps(text_cps(text))), ats(Expected.NFD_EXPECTED, i)),
        Check.truth(f"nfc.stable.{i}", nfc_text(nfc_text(text)) == nfc_text(text), "NFC is not idempotent here"),
    ]


def qwen_cases():
    """The whole Qwen pipeline, byte-identical to the reference package, from the
    `tokenizer.json` tables and again from the same vocabulary in GGUF form."""
    tok = qwen_tokenizer()
    return concat_all([
        fold(range(len(Expected.QWEN_INPUTS)), [], lambda acc, i: listConcat(acc, qwen_row(tok, i, "qwen"))),
        qwen_refusal_cases(),
        gguf_cases(),
    ])


def gguf_cases():
    """[LLM-TOKENIZER] The same encodings and decodes from `tokenizer.ggml.tokens`,
    `token_type` and `merges`, plus the refusals: a merge whose spelling is not a
    token, and a vocabulary missing a byte."""
    tables = gguf_tables()
    match GgufVocab.qwen_from_vocabulary(tables.tokens, tables.types, tables.merges):
        case Success(value=tok):
            return concat_all([
                fold(range(len(Expected.QWEN_INPUTS)), [], lambda acc, i: listConcat(acc, qwen_row(tok, i, "gguf"))),
                [Check.equal("gguf.added.count", len(tok.added), len(Expected.QWEN_ADDED_IDS))],
                gguf_refusals(tables),
            ])
        case Error(message=message):
            return [Check.failed("gguf.build", message)]


class GgufTables:
    """The three GGUF vocabulary tables, rebuilt from the oracle's subset."""
    tokens: list[str]
    types: list[int]
    merges: list[str]


def gguf_tables():
    """`tokens[id]` for every byte, merge and added id the oracle recorded, `""` and
    type 0 elsewhere, and each merge as `left right`."""
    n = Lists_max(Expected.QWEN_ADDED_IDS) + 1
    spelled = fold(range(len(Expected.QWEN_MERGE_IDS)), fold(range(256), {}, lambda acc, b: mapSet(acc, toString(at_int(Expected.QWEN_BYTE_IDS, b)), bytelevel_text([b]))), lambda acc, i: mapSet(acc, toString(at_int(Expected.QWEN_MERGE_IDS, i)), at_str(Expected.QWEN_MERGES_LEFT, i) + at_str(Expected.QWEN_MERGES_RIGHT, i)))
    named = fold(range(len(Expected.QWEN_ADDED_IDS)), spelled, lambda acc, i: mapSet(acc, toString(at_int(Expected.QWEN_ADDED_IDS, i)), at_str(Expected.QWEN_ADDED_CONTENT, i)))
    kinds = fold(range(len(Expected.QWEN_ADDED_IDS)), {}, lambda acc, i: mapSet(acc, toString(at_int(Expected.QWEN_ADDED_IDS, i)), GgufVocab.CONTROL if at_int(Expected.QWEN_ADDED_SPECIAL, i) == 1 else GgufVocab.USER_DEFINED))
    return GgufTables(
        tokens=toList(map(range(n), lambda i: unwrap_or(mapGet(named, toString(i)), ""))),
        types=toList(map(range(n), lambda i: unwrap_or(mapGet(kinds, toString(i)), 1))),
        merges=toList(map(range(len(Expected.QWEN_MERGE_IDS)), lambda i: at_str(Expected.QWEN_MERGES_LEFT, i) + " " + at_str(Expected.QWEN_MERGES_RIGHT, i))),
    )


def Lists_max(xs):
    return fold(range(len(xs)), 0, lambda m, i: at_int(xs, i) if at_int(xs, i) > m else m)


def gguf_refusals(tables):
    return [
        refused("gguf.rejects.merge", GgufVocab.qwen_from_vocabulary(tables.tokens, tables.types, listAppend(tables.merges, "zz qq")), "merge " + toString(len(tables.merges)) + " names a spelling the vocabulary lacks"),
        refused("gguf.rejects.byte", GgufVocab.qwen_from_vocabulary(replace_at(tables.tokens, at_int(Expected.QWEN_BYTE_IDS, 65), ""), tables.types, tables.merges), "the vocabulary has no token for byte 65"),
    ]


def refused(name, result, want):
    match result:
        case Success(value=_tok):
            return Check.failed(name, "built a tokenizer from a broken table")
        case Error(message=message):
            return Check.equal(name, message, want)


def replace_at(xs, position, value):
    return toList(map(range(len(xs)), lambda i: value if i == position else ats(xs, i)))


def qwen_row(tok, i, label):
    text = ats(Expected.QWEN_INPUTS, i)
    match qwen_encode(tok, text):
        case Success(value=enc):
            return listConcat([
                Check.ints_equal(f"{label}.{i}.ids", enc.ids, atl(Expected.QWEN_IDS, i)),
                strs_equal(f"{label}.{i}.tokens", enc.tokens, atl(Expected.QWEN_TOKENS, i)),
            ], [
                Check.ints_equal(f"{label}.{i}.type_ids", enc.type_ids, atl(Expected.QWEN_TYPE_IDS, i)),
                offs_equal(f"{label}.{i}.offsets", offs_list(enc), atl(Expected.QWEN_OFFSETS, i)),
                Check.equal(f"{label}.{i}.decode", decoded(tok, enc.ids, False), ats(Expected.QWEN_DECODES, i)),
                Check.equal(f"{label}.{i}.skip", decoded(tok, enc.ids, True), ats(Expected.QWEN_SKIP_DECODES, i)),
                strs_equal(f"{label}.{i}.pretok", mapList(qwen_pretokenize(norm_of(nfc_text(text))), lambda p: cps_text(p.chars)), atl(Expected.QWEN_PRETOK_PIECES, i)),
            ])
        case Error(message=message):
            return [Check.failed(f"{label}.{i}", "Error: " + message)]


def decoded(tok, ids, skip):
    match qwen_decode(tok, ids, skip):
        case Success(value=text):
            return text
        case Error(message=message):
            return "Error: " + message


def qwen_refusal_cases():
    """A document declaring a pipeline this port does not implement is refused."""
    return [
        Check.truth("qwen.json.refused", not accepted(load_qwen_tokenizer("{\"normalizer\":{\"type\":\"NFD\"}}")), "an NFD pipeline was accepted"),
        Check.truth("qwen.json.notjson", not accepted(load_qwen_tokenizer("not json")), "bad JSON was accepted"),
        Check.truth("qwen.json.empty", not accepted(load_qwen_tokenizer("{}")), "an empty document was accepted"),
        Check.truth("qwen.bpe.rejects", not accepted(build_bpe([], [], [], [])), "an empty vocabulary was accepted"),
        Check.truth("qwen.bpe.rejects.merges", not accepted(build_bpe(empty_byte_ids_test(), ["a"], ["b"], [-1])), "a merge whose id is already past was accepted"),
    ]


def empty_byte_ids_test():
    return toList(map(range(256), lambda i: 1000 + i))


def accepted(r):
    """Whether a Result holds a value."""
    match r:
        case Success(value=_value):
            return True
        case Error(message=_message):
            return False


def split_oracle_case(i):
    text = nfc_text(at_str(Expected.SPLIT_INPUTS, i))
    pieces = qwen_pretokenize(norm_of(text))
    return [
        strs_equal(f"split.{i}.pieces", mapList(pieces, norm_text), atl(Expected.SPLIT_PIECES, i)),
        offs_equal(f"split.{i}.offsets", pretok_offsets(text, pieces), atl(Expected.SPLIT_OFFSETS, i)),
    ]


def byte_boundaries():
    return [
        refused("utf8.e0.overlong", utf8_decode([224, 159, 128], 0, ""), "bad utf8 at byte 0"),
        refused("utf8.f0.overlong", utf8_decode([240, 143, 128, 128], 0, ""), "bad utf8 at byte 0"),
        refused("utf8.f4.too_high", utf8_decode([65, 244, 144, 128, 128], 0, ""), "bad utf8 at byte 1"),
        Check.equal("utf8.offset", unwrap_or(utf8_decode([65, 195, 169], 1, "x"), ""), "xé"),
        refused("bytelevel.rejects", bytelevel_decode(["中", "a"]), "character 20013 is not a byte-level symbol"),
        Check.equal("bytelevel.split.char", unwrap_or(bytelevel_decode([bytelevel_text([195]), bytelevel_text([169])]), ""), "é"),
        Check.equal("bytelevel.lossy", unwrap_or(bytelevel_decode([bytelevel_text([255, 226, 150, 65])]), ""), "��A"),
        strs_equal("meta.first", mapList(metaspace_pretokenize(norm_of("a b"), META_CP, 1, True), norm_text), ["▁a", "▁b"]),
        strs_equal("meta.first.nonzero", mapList(metaspace_pretokenize(append_norm(empty_norm(), 97, 5, 6), META_CP, 1, True), norm_text), ["a"]),
        strs_equal("meta.never.unsplit", mapList(metaspace_pretokenize(norm_of("a b"), META_CP, 2, False), norm_text), ["a▁b"]),
    ]


def qwen_error_boundaries():
    known = at_int(Expected.QWEN_BYTE_IDS, 65)
    missing = 999999999
    tok = qwen_tokenizer()
    return [
        refused("bpe.merge.lengths", build_bpe(Expected.QWEN_BYTE_IDS, ["a"], [], []), "the merge lists disagree in length"),
        refused("bpe.merge.invalid", build_bpe(Expected.QWEN_BYTE_IDS, ["中", "b"], ["a", "c"], [999, 1000]), "merge 0 is unusable: its halves must be byte-level text and its id must exceed every earlier merge's"),
        refused("qwen.decode.missing.first", qwen_decode(tok, [missing, known], False), "no token with id 999999999"),
        refused("qwen.decode.missing.middle", qwen_decode(tok, [known, missing, known], False), "no token with id 999999999"),
    ]


def qwen_pipeline_document():
    pattern = unwrap_or(jsonQuote(QWEN_PATTERN), "")
    return ("{\"normalizer\":{\"type\":\"NFC\"},\"pre_tokenizer\":{\"type\":\"Sequence\",\"pretokenizers\":["
        + "{\"type\":\"Split\",\"behavior\":\"Isolated\",\"invert\":false,\"pattern\":{\"Regex\":" + pattern + "}},"
        + "{\"type\":\"ByteLevel\",\"add_prefix_space\":false,\"use_regex\":false}]},"
        + "\"decoder\":{\"type\":\"ByteLevel\",\"add_prefix_space\":false,\"use_regex\":false},"
        + "\"post_processor\":{\"type\":\"ByteLevel\",\"add_prefix_space\":false,\"trim_offsets\":false},"
        + "\"model\":{\"type\":\"BPE\",\"ignore_merges\":false,\"byte_fallback\":false,\"fuse_unk\":false,"
        + "\"unk_token\":null,\"continuing_subword_prefix\":\"\",\"end_of_word_suffix\":\"\"}}")


def qwen_added_boundaries():
    token = AddedToken(content="middle", id=999999998, special=False, lstrip=True, rstrip=True, single_word=True)
    broken = QwenTokenizer(model=empty_bpe(), added=[token])
    tok = QwenTokenizer(model=qwen_model(), added=[token])
    text = " \tmiddle\n "
    match qwen_encode(tok, text):
        case Error(message=message):
            return [Check.failed("qwen.added.strip", message)]
        case Success(value=enc):
            return listConcat(core_cases("qwen.added.strip", enc, [token.id], ["middle"], [[0, len(text)]]), [
                refused("qwen.encode.missing.byte", qwen_encode(broken, "a middle b"), "segment 0 could not be tokenized"),
                qwen_added_requires_boundaries(tok, token.id),
            ])


def qwen_added_requires_boundaries(tok, id):
    match qwen_encode(tok, "amiddle middleb"):
        case Success(value=enc):
            return Check.truth("qwen.added.single_word", not listContains(enc.ids, id), "matched inside a word")
        case Error(message=message):
            return Check.failed("qwen.added.single_word", message)


def qwen_public_boundaries():
    return [
        Check.equal("qwen.contraction.re", qwen_piece_len(norm_of("'RE"), 0), 3),
        Check.equal("qwen.contraction.ve", qwen_piece_len(norm_of("'ve"), 0), 3),
        Check.equal("qwen.contraction.ll", qwen_piece_len(norm_of("'Ll"), 0), 3),
        refused("qwen.json.external.tables", load_qwen_tokenizer(qwen_pipeline_document()), "tokenizer.json: this port reads the vocabulary and merges from supplied data, not from the document (see this module's docstring)"),
    ]


def all_cases():
    """Every suite case, grouped by pipeline layer."""
    splits = fold(range(len(Expected.SPLIT_INPUTS)), [], lambda acc, i: listConcat(acc, split_oracle_case(i)))
    return concat_all([encode_cases(), pretokenizer_cases(), normalizer_cases(), direct_cases(), utf8_cases(), nfc_cases(), qwen_cases(), splits, byte_boundaries(), qwen_error_boundaries(), qwen_added_boundaries(), qwen_public_boundaries()])


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
