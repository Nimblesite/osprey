# osprey: flavor=python
"""@osprey/tokenizers' suite: every encode, decode, pre-tokenization and
normalization compared with the real `tokenizers` package's output
(scripts/oracles/tokenizers.py generates tests/expected.py)."""
from testing import Check
from tokenizers_expected import Expected
from tokenizers.Decoders import byte_fallback_decode_chain, metaspace_decode_chain, wordpiece_decode
from tokenizers.Models import (
    Token, build_unigram, build_wordlevel, build_wordpiece, char_starts,
    id_to_token, unigram_encode, unigram_tokenize, wordlevel_tokenize, wordpiece_tokenize,
)
from tokenizers.Normalizers import Norm, bert_normalize, norm_of, norm_text, prepend_cp, replace_space
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
from tokenizers.Unicode import (
    is_chinese_char, is_control, is_numeric, is_punctuation, is_whitespace,
    is_word_char, lowercase, strip_base,
)


def offs_list(enc):
    offs = enc.offsets
    return toList(map(range(len(offs)), lambda i: pair(unwrap_or(listGet(offs, i), Offset(start=0, end=0)))))


def pair(o):
    return [o.start, o.end]


def concat_all(groups):
    """One flat list of cases from a list of case groups."""
    return fold(range(len(groups)), [], lambda acc, i: listConcat(acc, unwrap_or(listGet(groups, i), [])))


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
    text = unwrap_or(listGet(Expected.WP_INPUTS, i), "")
    enc = wordpiece_encode(wp, text, cls_id, sep_id)
    return listConcat(
        wp_enc_cases(f"wp.{i}", enc, unwrap_or(listGet(Expected.WP_IDS, i), []), unwrap_or(listGet(Expected.WP_TOKENS, i), []), unwrap_or(listGet(Expected.WP_OFFSETS, i), []), unwrap_or(listGet(Expected.WP_TYPE_IDS, i), [])),
        [Check.equal(f"wp.{i}.decode", wordpiece_decode_pipeline(wp, enc.ids), unwrap_or(listGet(Expected.WP_DECODES, i), ""))],
    )


def wp_pair_case(wp, cls_id, sep_id, i):
    a = unwrap_or(listGet(Expected.WP_PAIR_A, i), "")
    b = unwrap_or(listGet(Expected.WP_PAIR_B, i), "")
    enc = wordpiece_encode_pair(wp, a, b, cls_id, sep_id)
    return wp_enc_cases(f"wp.pair.{i}", enc, unwrap_or(listGet(Expected.WP_PAIR_IDS, i), []), unwrap_or(listGet(Expected.WP_PAIR_TOKENS, i), []), unwrap_or(listGet(Expected.WP_PAIR_OFFSETS, i), []), unwrap_or(listGet(Expected.WP_PAIR_TYPE_IDS, i), []))


def uni_case(m, tag, byte_fallback_mode, i, ids_c, tokens_c, offsets_c, decodes_c):
    text = unwrap_or(listGet(Expected.UNI_INPUTS, i), "")
    enc = unigram_encode_pipeline(m, text)
    return listConcat(
        core_cases(f"{tag}.{i}", enc, unwrap_or(listGet(ids_c, i), []), unwrap_or(listGet(tokens_c, i), []), unwrap_or(listGet(offsets_c, i), [])),
        [Check.equal(f"{tag}.{i}.decode", unigram_decode_pipeline(m, enc.ids, byte_fallback_mode), unwrap_or(listGet(decodes_c, i), ""))],
    )


def uni_cases(m, tag, byte_fallback_mode, ids_c, tokens_c, offsets_c, decodes_c):
    return fold(range(len(Expected.UNI_INPUTS)), [], lambda acc, i: listConcat(acc, uni_case(m, tag, byte_fallback_mode, i, ids_c, tokens_c, offsets_c, decodes_c)))


def wl_case(wl, i):
    text = unwrap_or(listGet(Expected.WL_INPUTS, i), "")
    enc = wordlevel_encode_pipeline(wl, text)
    return listConcat(
        core_cases(f"wl.{i}", enc, unwrap_or(listGet(Expected.WL_IDS, i), []), unwrap_or(listGet(Expected.WL_TOKENS, i), []), unwrap_or(listGet(Expected.WL_OFFSETS, i), [])),
        [Check.equal(f"wl.{i}.decode", wordlevel_decode_pipeline(wl, enc.ids), unwrap_or(listGet(Expected.WL_DECODES, i), ""))],
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
    text = unwrap_or(listGet(Expected.PRE_INPUTS, i), "")
    splits = split_fn(norm_of(text))
    toks = toList(map(range(len(splits)), lambda k: norm_text(unwrap_or(listGet(splits, k), Norm(chars=[], starts=[], ends=[])))))
    return [
        strs_equal(f"{name}.{i}.tokens", toks, unwrap_or(listGet(want_tokens_c, i), [])),
        offs_equal(f"{name}.{i}.offsets", pretok_offsets(text, splits), unwrap_or(listGet(want_offsets_c, i), [])),
    ]


def pretok_cases(name, split_fn, want_tokens_c, want_offsets_c):
    return fold(range(len(Expected.PRE_INPUTS)), [], lambda acc, i: listConcat(acc, pretok_case(name, split_fn, want_tokens_c, want_offsets_c, i)))


def norm_case(i):
    text = unwrap_or(listGet(Expected.NORM_INPUTS, i), "")
    return [
        Check.equal(f"norm.{i}.default", norm_text(bert_normalize(text, True, True, True, True)), unwrap_or(listGet(Expected.NORM_DEFAULT, i), "")),
        Check.equal(f"norm.{i}.clean", norm_text(bert_normalize(text, True, False, False, False)), unwrap_or(listGet(Expected.NORM_CLEAN_ONLY, i), "")),
        Check.equal(f"norm.{i}.lower", norm_text(bert_normalize(text, False, False, False, True)), unwrap_or(listGet(Expected.NORM_LOWER_ONLY, i), "")),
        Check.equal(f"norm.{i}.strip", norm_text(bert_normalize(text, False, False, True, False)), unwrap_or(listGet(Expected.NORM_STRIP_ONLY, i), "")),
        Check.equal(f"norm.{i}.chinese", norm_text(bert_normalize(text, False, True, False, False)), unwrap_or(listGet(Expected.NORM_CHINESE_ONLY, i), "")),
    ]


def uni_pred_case(name, pred, want_c):
    return fold(range(len(Expected.UNI_CPS)), [], lambda acc, i: listAppend(acc, Check.equal(f"{name}.{i}", pred(unwrap_or(listGet(Expected.UNI_CPS, i), 0)), unwrap_or(listGet(want_c, i), False))))


def uni_list_case(name, f, want_c):
    return fold(range(len(Expected.UNI_CPS)), [], lambda acc, i: listAppend(acc, Check.ints_equal(f"{name}.{i}", f(unwrap_or(listGet(Expected.UNI_CPS, i), 0)), unwrap_or(listGet(want_c, i), []))))


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


def token_ids(toks):
    return toList(map(range(len(toks)), lambda k: unwrap_or(listGet(toks, k), Token(id=0, value="", start=0, end=0)).id))


def token_values(toks):
    return toList(map(range(len(toks)), lambda k: unwrap_or(listGet(toks, k), Token(id=0, value="", start=0, end=0)).value))


def token_starts(toks):
    return toList(map(range(len(toks)), lambda k: unwrap_or(listGet(toks, k), Token(id=0, value="", start=0, end=0)).start))


def token_ends(toks):
    return toList(map(range(len(toks)), lambda k: unwrap_or(listGet(toks, k), Token(id=0, value="", start=0, end=0)).end))


def model_row_case(name, toks, want_ids, want_values, want_starts, want_ends):
    return [
        Check.ints_equal(name + ".ids", token_ids(toks), want_ids),
        strs_equal(name + ".values", token_values(toks), want_values),
        Check.ints_equal(name + ".starts", token_starts(toks), want_starts),
        Check.ints_equal(name + ".ends", token_ends(toks), want_ends),
    ]


def model_rows_case(name, rows_fn, texts_c, ids_c, values_c, starts_c, ends_c):
    return fold(range(len(ids_c)), [], lambda acc, i: listConcat(acc, model_row_case(f"{name}.{i}", rows_fn(unwrap_or(listGet(texts_c, i), "")), unwrap_or(listGet(ids_c, i), []), unwrap_or(listGet(values_c, i), []), unwrap_or(listGet(starts_c, i), []), unwrap_or(listGet(ends_c, i), []))))


def wp_model_cases(wp):
    return model_rows_case("mwp", lambda w: wordpiece_tokenize(wp, w), Expected.MODEL_WP_WORDS, Expected.MODEL_WP_TOK_IDS, Expected.MODEL_WP_TOK_VALUES, Expected.MODEL_WP_TOK_STARTS, Expected.MODEL_WP_TOK_ENDS)


def uni_model_cases(uni_nobf):
    return model_rows_case("muni", lambda s: unigram_tokenize(uni_nobf, s), Expected.MODEL_UNI_TEXTS, Expected.MODEL_UNI_IDS, Expected.MODEL_UNI_VALUES, Expected.MODEL_UNI_STARTS, Expected.MODEL_UNI_ENDS)


def wl_model_cases(wl):
    return model_rows_case("mwl", lambda s: wordlevel_tokenize(wl, s), Expected.MODEL_WL_TEXTS, Expected.MODEL_WL_IDS, Expected.MODEL_WL_VALUES, Expected.MODEL_WL_STARTS, Expected.MODEL_WL_ENDS)


def uni_encode_case(uni_nobf, i):
    s = unwrap_or(listGet(Expected.MODEL_UNI_TEXTS, i), "")
    return strs_equal(f"muni.encode.{i}", unigram_encode(uni_nobf, s), unwrap_or(listGet(Expected.MODEL_UNI_VALUES, i), []))


def uni_encode_cases(uni_nobf):
    return fold(range(len(Expected.MODEL_UNI_TEXTS)), [], lambda acc, i: listAppend(acc, uni_encode_case(uni_nobf, i)))


def id_to_token_case(wp, i):
    id = unwrap_or(listGet(Expected.MODEL_WP_IDS, i), 0)
    return Check.equal(f"mwp.id_to_token.{i}", id_to_token(wp.tokens, id), unwrap_or(listGet(Expected.MODEL_WP_ID_TO_TOKEN, i), ""))


def id_to_token_cases(wp):
    return fold(range(len(Expected.MODEL_WP_IDS)), [], lambda acc, i: listAppend(acc, id_to_token_case(wp, i)))


def decode_case(name, dec_fn, toks_c, want_c, i):
    toks = unwrap_or(listGet(toks_c, i), [])
    return Check.equal(f"{name}.{i}", dec_fn(toks), unwrap_or(listGet(want_c, i), ""))


def decoder_cases():
    """WordPiece, Metaspace and ByteFallback decoders on token lists."""
    wp_cases = fold(range(len(Expected.DECODE_WP)), [], lambda acc, i: listAppend(acc, decode_case("dec.wp", lambda t: wordpiece_decode(t, "##", True), Expected.DECODE_WP_TOKENS, Expected.DECODE_WP, i)))
    meta_cases = fold(range(len(Expected.DECODE_META)), [], lambda acc, i: listAppend(acc, decode_case("dec.meta", lambda t: join(metaspace_decode_chain(t, META_CP, 0), ""), Expected.DECODE_META_TOKENS, Expected.DECODE_META, i)))
    bf_cases = fold(range(len(Expected.DECODE_BF)), [], lambda acc, i: listAppend(acc, decode_case("dec.bf", lambda t: join(byte_fallback_decode_chain(t), ""), Expected.DECODE_BF_TOKENS, Expected.DECODE_BF, i)))
    return concat_all([wp_cases, meta_cases, bf_cases])


def meta_helper_cases():
    """Metaspace's replacement and prepend primitives on a Norm."""
    repl = fold(range(len(Expected.META_HELPER_INPUTS)), [], lambda acc, i: listAppend(acc, Check.equal(f"norm.replace.{i}", norm_text(replace_space(norm_of(unwrap_or(listGet(Expected.META_HELPER_INPUTS, i), "")), META_CP)), unwrap_or(listGet(Expected.META_REPLACE, i), ""))))
    prep = fold(range(len(Expected.META_PREPEND_INPUTS)), [], lambda acc, i: listAppend(acc, Check.equal(f"norm.prepend.{i}", norm_text(prepend_cp(norm_of(unwrap_or(listGet(Expected.META_PREPEND_INPUTS, i), "")), META_CP)), unwrap_or(listGet(Expected.META_PREPEND, i), ""))))
    return listConcat(repl, prep)


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


def all_cases():
    """Every suite case, grouped by pipeline layer."""
    return concat_all([encode_cases(), pretokenizer_cases(), normalizer_cases(), direct_cases()])


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
