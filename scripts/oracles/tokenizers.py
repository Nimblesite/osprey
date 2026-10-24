#!/usr/bin/env python3
"""CPython oracle for @osprey/tokenizers.

Imports the real `tokenizers` package, trains the small WordPiece and Unigram
vocabularies on fixed corpora, and computes every id, token, offset, type id and
decode the port's suite asserts. Writes packages/tokenizers/tests/expected.py
through the shared writer ([PORT-TEST-ORACLE]). Deterministic: fixed corpora,
no randomness. Run with the oracle virtualenv, from the repository root:

    $ORACLE scripts/oracles/tokenizers.py
"""
import os
import sys

from expected_module import write

# The oracle script itself is named tokenizers.py; drop its own directory from
# sys.path so `import tokenizers` reaches the installed package, not this file.
_self_dir = os.path.dirname(os.path.abspath(__file__))
if sys.path and os.path.abspath(sys.path[0]) == _self_dir:
    sys.path.pop(0)

import tokenizers
from tokenizers import (
    Tokenizer,
    decoders,
    models,
    normalizers,
    pre_tokenizers,
    processors,
)

# ---------------------------------------------------------------------------
# Corpora
# ---------------------------------------------------------------------------

# The Unigram vocabulary below was trained ONCE by UnigramTrainer on this fixed
# corpus and frozen here, because UnigramTrainer iterates a Rust HashMap whose
# order varies per process: training again would produce a different vocabulary
# and make the oracle non-deterministic. WordPieceTrainer has the same problem,
# so WordPiece uses the hand-built fixed vocabulary further down.
_UNI_TRAINING_CORPUS = [
    "the quick brown fox jumps over the lazy dog",
    "hello world",
    "Hello, world!",
    "How are you doing?",
    "I am learning to tokenize text",
    "The Transformer architecture uses attention",
    "subword tokenization and unigram segmentation",
    "a b c d e f g h i j k l m n o p q r s t u v w x y z",
    "sentencepiece uses a unigram language model",
    "the unigram algorithm learns a vocabulary of pieces",
]

FROZEN_UNI_VOCAB = [
    ("<unk>", 0.0),
    (" ", -2.805836290862396),
    ("u", -3.1689585773606597),
    ("o", -3.405685313611367),
    ("r", -3.643574172809087),
    ("a", -3.695102963344599),
    ("g", -3.7482203732065296),
    ("c", -3.8012129849488927),
    ("t", -3.8470252447161646),
    ("e ", -3.8753936567527227),
    ("m", -3.88853934750073),
    ("h", -3.955610352703434),
    ("s", -4.000301160816647),
    ("the ", -4.1916143131327885),
    ("b", -4.211176692254328),
    ("d", -4.2160436186809225),
    ("la", -4.28464931315469),
    ("i", -4.354665994867647),
    ("y ", -4.446543920620071),
    ("l", -4.513832590008572),
    ("x", -4.635032506968461),
    ("z", -4.679409843197816),
    ("T", -4.679409843849405),
    ("H", -4.679409843849405),
    (" tokeniz", -4.67940984446103),
    ("m learn", -4.6794098465290395),
    ("ow", -4.679410020266411),
    ("f ", -4.679410184797021),
    ("fo", -4.679411028681346),
    (" j", -4.67941307373146),
    (" world", -4.67941347215578),
    ("piece", -4.679420580213729),
    ("ing", -4.686697776980718),
    ("ello", -4.6884850704242105),
    ("ation", -4.6905008587786625),
    (" v", -4.744075797854865),
    ("n ", -4.813552356633899),
    ("ten", -4.874374707229483),
    ("k ", -4.890645614813072),
    ("er ", -4.903874600341044),
    (" unigram ", -4.93099258162042),
    ("y", -4.980743964596372),
    (" do", -5.085719851227464),
    ("it", -5.144456246637407),
    ("s ", -5.1860609549701735),
    (" a", -5.187497769851593),
    ("se", -5.212279883460833),
    ("unigram ", -5.220171792271795),
    ("ent", -5.340312774705113),
    ("n", -5.46298755014978),
    (" ar", -5.4906245212612355),
    ("v", -5.529989975343616),
    ("p", -5.5443331700326794),
    (" b", -5.55619066728501),
    ("q", -5.608302810755047),
    ("d ", -5.620512212064269),
    ("te", -5.629040416826691),
    ("tion", -5.651670501860904),
    (" uses a", -5.662402098222058),
    ("wor", -5.679401053639453),
    ("I", -5.679409843849405),
    (",", -5.679409843849405),
    ("!", -5.679409843849405),
    ("?", -5.679409843849405),
    (" w", -5.679412395138826),
    ("e uses a", -5.6967068053660395),
    ("el", -5.720771599951426),
    (" to", -5.7221744787414215),
    ("ng", -5.747115454916061),
    (" q", -5.75531311719839),
    ("ce", -5.758945658415048),
    ("he ", -5.775911843539055),
    ("ar", -5.78107098001572),
    ("x ", -5.801420272964273),
    ("ns", -5.802586216853492),
    (" p", -5.832890382226194),
    ("or", -5.871118992036324),
    ("ec", -5.886206281030374),
    ("s a", -5.97237481056953),
    ("ra", -5.982825132244262),
    ("r ", -6.251405822782329),
    ("g ", -6.353477143228682),
    ("en", -6.5249760523256155),
    (" u", -6.664097901485659),
    ("j", -6.746487312090012),
    ("k", -6.7465873120900115),
    ("f", -6.746687312090011),
    ("w", -6.746787312090012),
    ("e", -6.746887312090012),
    ("an", -6.746887312090012),
]

WP_VOCAB = [
    "[UNK]", "[CLS]", "[SEP]", "[PAD]", "[MASK]",
    "hello", "world", "!", ",", "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog",
    "how", "are", "you", "doing", "?", "don", "'", "t",
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "u", "v", "w", "x", "y", "z",
    "##ing", "##ed", "##er", "##s", "##e", "##d", "##n", "##t", "##r", "##o", "##a", "##l", "##c", "##k", "##m", "##u", "##f", "##g", "##h", "##i", "##b", "##p", "##w", "##y", "##v", "##x", "##z", "##j", "##q",
]


# ---------------------------------------------------------------------------
# Pipelines
# ---------------------------------------------------------------------------

def build_wordpiece():
    vocab = {t: i for i, t in enumerate(WP_VOCAB)}
    tok = Tokenizer(models.WordPiece(vocab=vocab, unk_token="[UNK]", max_input_chars_per_word=100))
    tok.normalizer = normalizers.BertNormalizer(
        clean_text=True, handle_chinese_chars=True, strip_accents=None, lowercase=True
    )
    tok.pre_tokenizer = pre_tokenizers.BertPreTokenizer()
    tok.decoder = decoders.WordPiece(prefix="##", cleanup=True)
    tok.post_processor = processors.TemplateProcessing(
        single="[CLS]:0 $A:0 [SEP]:0",
        pair="[CLS]:0 $A:0 [SEP]:0 $B:1 [SEP]:1",
        special_tokens=[("[CLS]", 1), ("[SEP]", 2)],
    )
    return tok


WP_INPUTS = [
    "",
    "   ",
    "Hello world!",
    "hello,world",
    "don't",
    "The quick brown fox jumps over the lazy dog.",
    "How are you doing?",
    "Caf\u00e9 au lait",
    "\u00c5ngstr\u00f6m",
    "\u4e2d\u6587\u6d4b\u8bd5",
    "\u65e5\u672c\u8a9e\u306e\u30c6\u30b9\u30c8",
    "\U0001f600\U0001f389",
    "a" * 150,
    "zzzzunknownwordzzzz",
    "Hello, \u4e2d\u6587! Caf\u00e9 \U0001f600",
    "a.b.c",
    "  leading and trailing  ",
    "tab\there\nnewline",
    "\u00c6sir and \u00d8resund",
    "UPPERCASE and lowercase",
]

WP_PAIRS = [
    ("Hello world!", "How are you?"),
    ("", ""),
    ("\u4e2d\u6587", "\U0001f600"),
    ("The quick", "brown fox"),
]


def wp_vocab_list(tok):
    vocab = tok.get_vocab()
    items = sorted(vocab.items(), key=lambda kv: kv[1])
    return [t for t, _ in items]


def build_unigram(byte_fallback):
    vocab = list(FROZEN_UNI_VOCAB)
    unk_id = 0
    if byte_fallback:
        vocab = vocab + [(f"<0x{b:02X}>", -100.0) for b in range(256)]
    model = models.Unigram(vocab=vocab, unk_id=unk_id, byte_fallback=byte_fallback)
    tok = Tokenizer(model)
    tok.pre_tokenizer = pre_tokenizers.Metaspace(replacement="\u2581", prepend_scheme="always")
    if byte_fallback:
        tok.decoder = decoders.Sequence(
            [
                decoders.ByteFallback(),
                decoders.Metaspace(replacement="\u2581", prepend_scheme="always"),
            ]
        )
    else:
        tok.decoder = decoders.Metaspace(replacement="\u2581", prepend_scheme="always")
    return tok, vocab, unk_id


UNI_INPUTS = [
    "",
    "   ",
    "hello world",
    "The quick brown fox",
    "subword tokenization",
    "\u4e2d\u6587\u6d4b\u8bd5",
    "\U0001f600\U0001f389",
    "Caf\u00e9 au lait",
    "don't stop",
    "unigram segmentation",
    "  leading and trailing  ",
    "verylongwordthatisnotinvocabulary",
    "numbers 12345",
    "a",
    " ",
    "p\u00e2t\u00e9 \u00e9l\u00e8ve \U0001f680",
]

WL_VOCAB = ["<unk>", "hello", "world", "!", "the", "quick", "brown", "fox", "How", "are", "you", "?"]

WL_INPUTS = [
    "",
    "hello world",
    "the quick brown fox!",
    "Hello WORLD",
    "unknown words here",
    "  hello  world  ",
    "How are you?",
]


def build_wordlevel():
    tok = Tokenizer(models.WordLevel(vocab={t: i for i, t in enumerate(WL_VOCAB)}, unk_token="<unk>"))
    tok.pre_tokenizer = pre_tokenizers.Whitespace()
    return tok


PRE_INPUTS = [
    "Hey friend!     How are you?!?",
    "hello,world",
    "don't",
    "a.b.c",
    "abc123def",
    "\u4e2d\u6587hello",
    "He\u00dflo",
    "  spaces  and\ttabs  ",
    "price: $100.50!",
    "a\u2014b \u201cc\u201d",
    "",
]

NORM_INPUTS = [
    "Hello World!",
    "Caf\u00e9 au lait",
    "\u4e2d\u6587\u6d4b\u8bd5",
    "control\x01chars \x07here",
    "a\u00a0b \u3000c",
    "\u0394delta \u03a9mega",
    "\u00c0\u00e9\u00ee\u00f5\u00fc \u00c5ngstr\u00f6m",
    "A\u0300ccent",
    "\U0001f600 emoji",
    "\u0130stanbul \u1e9e",
    "\t\nwhitespace",
    "\u00c6sir \u00d8resund \u00deing",
    "",
]


# ---------------------------------------------------------------------------
# Direct fixtures: Unicode character classes, model.tokenize, decoders,
# TemplateProcessing, and the Metaspace replacement/prepend primitives.
# ---------------------------------------------------------------------------

UNI_CPS = [
    0, 9, 10, 13, 32, 33, 44, 46, 48, 65, 95, 97, 127, 128, 159, 160,
    170, 186, 192, 224, 255, 256, 304, 305, 376, 768, 769, 913, 916, 930, 937,
    7838, 8204, 8205, 8212, 8220, 9601, 12288, 19968, 40959, 65313, 65338,
    65533, 8364, 128512,
]


def _bert(**kwargs):
    return normalizers.BertNormalizer(**kwargs)


def _uni_rows():
    ws_pt = pre_tokenizers.WhitespaceSplit()
    punct_pt = pre_tokenizers.Punctuation()
    digits_pt = pre_tokenizers.Digits(individual_digits=True)
    word_pt = pre_tokenizers.Whitespace()
    clean_n = _bert(clean_text=True, handle_chinese_chars=False, lowercase=False, strip_accents=False)
    cjk_n = _bert(clean_text=False, handle_chinese_chars=True, lowercase=False, strip_accents=False)
    lower_n = _bert(clean_text=False, handle_chinese_chars=False, lowercase=True, strip_accents=False)
    strip_n = _bert(clean_text=False, handle_chinese_chars=False, lowercase=False, strip_accents=True)
    ws, punct, numeric, word, control, chinese, lower, strip = [], [], [], [], [], [], [], []
    for cp in UNI_CPS:
        s = chr(cp)
        ws.append(ws_pt.pre_tokenize_str(s) == [])
        punct.append(len(punct_pt.pre_tokenize_str("a" + s)) == 2)
        numeric.append(len(digits_pt.pre_tokenize_str("a" + s)) == 2)
        wr = word_pt.pre_tokenize_str("a" + s)
        word.append(len(wr) == 1 and len(wr[0][0]) == 2)
        control.append(clean_n.normalize_str(s) == "" and cp != 0xFFFD)
        chinese.append(cjk_n.normalize_str(s) == " " + s + " ")
        lower.append([ord(c) for c in lower_n.normalize_str(s)])
        strip.append([ord(c) for c in strip_n.normalize_str(s)])
    return ws, punct, numeric, word, control, chinese, lower, strip


MODEL_WP_WORDS = ["hello", "world", "unknowable", "playing", "doings", "zebra", "a", "a" * 150, ""]
MODEL_WP_IDS = [0, 1, 2, 4, 5, 25, 49, len(WP_VOCAB) - 1]
MODEL_UNI_TEXTS = ["\u2581hello", "\u2581world", "\u2581unknowable", "\u2581caf\u00e9", "\u2581", "abc", "\u2581p\u00e2t\u00e9", ""]
MODEL_WL_TEXTS = ["hello", "world", "unknown", ""]


def _model_token_rows(model, texts):
    ids, values, starts, ends = [], [], [], []
    for s in texts:
        toks = model.tokenize(s)
        ids.append([t.id for t in toks])
        values.append([t.value for t in toks])
        starts.append([t.offsets[0] for t in toks])
        ends.append([t.offsets[1] for t in toks])
    return ids, values, starts, ends


DECODE_WP_TOKENS = [["Hello", "##s"], ["don", "'", "t"], ["[CLS]", "hello", "[SEP]"], ["a", "##b", "##c"], []]
DECODE_META_TOKENS = [["\u2581hello", "\u2581world"], ["\u2581"], ["\u2581", "a"], ["plain"], []]
DECODE_BF_TOKENS = [["<0x48>", "<0x69>"], ["<0xC3>", "<0xA9>"], ["<0xE2>", "<0x96>", "<0x81>"], ["plain"], ["<0xFF>"], []]

META_HELPER_INPUTS = ["hello world", "  spaced  ", "tab\there", ""]
META_PREPEND_INPUTS = ["hello", "caf\u00e9", "a"]


def _meta_helper_rows():
    replace_pt = pre_tokenizers.Metaspace("\u2581", prepend_scheme="never", split=False)
    prepend_pt = pre_tokenizers.Metaspace("\u2581", prepend_scheme="always", split=False)
    replace, prepend = [], []
    for s in META_HELPER_INPUTS:
        r = replace_pt.pre_tokenize_str(s)
        replace.append(r[0][0] if r else "")
    for s in META_PREPEND_INPUTS:
        prepend.append(prepend_pt.pre_tokenize_str(s)[0][0])
    return replace, prepend


def _proc_rows():
    wp = build_wordpiece()
    raw = Tokenizer(wp.model)
    cls_id = wp.model.token_to_id("[CLS]")
    sep_id = wp.model.token_to_id("[SEP]")
    a = raw.encode("ab")
    b = raw.encode("a")
    single = wp.post_processor.process(a)
    pair = wp.post_processor.process(a, b)
    return cls_id, sep_id, a, b, single, pair


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def run_pre(pt):
    toks, offs = [], []
    for s in PRE_INPUTS:
        r = pt.pre_tokenize_str(s)
        toks.append([t for t, _ in r])
        offs.append([[a, b] for _, (a, b) in r])
    return toks, offs


def norm_flags(**kwargs):
    n = normalizers.BertNormalizer(**kwargs)
    return [n.normalize_str(s) for s in NORM_INPUTS]


def main():
    wp = build_wordpiece()
    wp_vocab = wp_vocab_list(wp)

    wp_ids, wp_tokens, wp_offsets, wp_type_ids, wp_decodes = [], [], [], [], []
    for s in WP_INPUTS:
        e = wp.encode(s)
        wp_ids.append(e.ids)
        wp_tokens.append(e.tokens)
        wp_offsets.append([list(o) for o in e.offsets])
        wp_type_ids.append(e.type_ids)
        wp_decodes.append(wp.decode(e.ids))

    wp_pair_ids, wp_pair_tokens, wp_pair_offsets, wp_pair_type_ids = [], [], [], []
    for a, b in WP_PAIRS:
        e = wp.encode(a, b)
        wp_pair_ids.append(e.ids)
        wp_pair_tokens.append(e.tokens)
        wp_pair_offsets.append([list(o) for o in e.offsets])
        wp_pair_type_ids.append(e.type_ids)

    uni, uni_vocab, uni_unk_id = build_unigram(byte_fallback=True)
    uni_nobf, nobf_vocab, _ = build_unigram(byte_fallback=False)

    def uni_rows(tok):
        ids, tokens, offsets, decodes = [], [], [], []
        for s in UNI_INPUTS:
            e = tok.encode(s)
            ids.append(e.ids)
            tokens.append(e.tokens)
            offsets.append([list(o) for o in e.offsets])
            decodes.append(tok.decode(e.ids))
        return ids, tokens, offsets, decodes

    uni_ids, uni_tokens_out, uni_offsets, uni_decodes = uni_rows(uni)
    nobf_ids, nobf_tokens, nobf_offsets, nobf_decodes = uni_rows(uni_nobf)

    wl = build_wordlevel()
    wl_ids, wl_tokens, wl_offsets, wl_decodes = [], [], [], []
    for s in WL_INPUTS:
        e = wl.encode(s)
        wl_ids.append(e.ids)
        wl_tokens.append(e.tokens)
        wl_offsets.append([list(o) for o in e.offsets])
        wl_decodes.append(wl.decode(e.ids))

    pre_bert_toks, pre_bert_offs = run_pre(pre_tokenizers.BertPreTokenizer())
    pre_ws_toks, pre_ws_offs = run_pre(pre_tokenizers.Whitespace())
    pre_wss_toks, pre_wss_offs = run_pre(pre_tokenizers.WhitespaceSplit())
    pre_punc_toks, pre_punc_offs = run_pre(pre_tokenizers.Punctuation())
    pre_digits_toks, pre_digits_offs = run_pre(pre_tokenizers.Digits(individual_digits=False))
    pre_digits_ind_toks, pre_digits_ind_offs = run_pre(pre_tokenizers.Digits(individual_digits=True))
    pre_delim_toks, pre_delim_offs = run_pre(pre_tokenizers.CharDelimiterSplit(delimiter=","))
    pre_meta_toks, pre_meta_offs = run_pre(pre_tokenizers.Metaspace(replacement="\u2581", prepend_scheme="always"))

    uni_ws, uni_punct, uni_numeric, uni_word, uni_control, uni_chinese, uni_lower, uni_strip = _uni_rows()

    wp_model_ids, wp_model_values, wp_model_starts, wp_model_ends = _model_token_rows(build_wordpiece().model, MODEL_WP_WORDS)
    uni_model_ids, uni_model_values, uni_model_starts, uni_model_ends = _model_token_rows(build_unigram(byte_fallback=False)[0].model, MODEL_UNI_TEXTS)
    wl_model_ids, wl_model_values, wl_model_starts, wl_model_ends = _model_token_rows(build_wordlevel().model, MODEL_WL_TEXTS)
    id_to_token_rows = [build_wordpiece().model.id_to_token(i) for i in MODEL_WP_IDS]

    decode_wp = [decoders.WordPiece(prefix="##", cleanup=True).decode(t) for t in DECODE_WP_TOKENS]
    decode_meta = [decoders.Metaspace(replacement="\u2581", prepend_scheme="always").decode(t) for t in DECODE_META_TOKENS]
    decode_bf = [decoders.ByteFallback().decode(t) for t in DECODE_BF_TOKENS]

    meta_replace, meta_prepend = _meta_helper_rows()
    proc_cls, proc_sep, raw_a, raw_b, proc_single, proc_pair = _proc_rows()

    constants = {
        "WP_VOCAB": wp_vocab,
        "WP_INPUTS": WP_INPUTS,
        "WP_IDS": wp_ids,
        "WP_TOKENS": wp_tokens,
        "WP_OFFSETS": wp_offsets,
        "WP_TYPE_IDS": wp_type_ids,
        "WP_DECODES": wp_decodes,
        "WP_PAIR_A": [a for a, _ in WP_PAIRS],
        "WP_PAIR_B": [b for _, b in WP_PAIRS],
        "WP_PAIR_IDS": wp_pair_ids,
        "WP_PAIR_TOKENS": wp_pair_tokens,
        "WP_PAIR_OFFSETS": wp_pair_offsets,
        "WP_PAIR_TYPE_IDS": wp_pair_type_ids,
        "UNI_TOKENS": [t for t, _ in uni_vocab],
        "UNI_SCORES": [s for _, s in uni_vocab],
        "UNI_UNK_ID": uni_unk_id,
        "UNI_BYTE_FALLBACK": True,
        "UNI_INPUTS": UNI_INPUTS,
        "UNI_IDS": uni_ids,
        "UNI_TOKENS_OUT": uni_tokens_out,
        "UNI_OFFSETS": uni_offsets,
        "UNI_DECODES": uni_decodes,
        "UNI_NOBF_VOCAB": [t for t, _ in nobf_vocab],
        "UNI_NOBF_SCORES": [s for _, s in nobf_vocab],
        "UNI_NOBF_IDS": nobf_ids,
        "UNI_NOBF_TOKENS": nobf_tokens,
        "UNI_NOBF_OFFSETS": nobf_offsets,
        "UNI_NOBF_DECODES": nobf_decodes,
        "WL_VOCAB": WL_VOCAB,
        "WL_INPUTS": WL_INPUTS,
        "WL_IDS": wl_ids,
        "WL_TOKENS": wl_tokens,
        "WL_OFFSETS": wl_offsets,
        "WL_DECODES": wl_decodes,
        "PRE_INPUTS": PRE_INPUTS,
        "PRE_BERT_TOKENS": pre_bert_toks,
        "PRE_BERT_OFFSETS": pre_bert_offs,
        "PRE_WS_TOKENS": pre_ws_toks,
        "PRE_WS_OFFSETS": pre_ws_offs,
        "PRE_WSS_TOKENS": pre_wss_toks,
        "PRE_WSS_OFFSETS": pre_wss_offs,
        "PRE_PUNC_TOKENS": pre_punc_toks,
        "PRE_PUNC_OFFSETS": pre_punc_offs,
        "PRE_DIGITS_TOKENS": pre_digits_toks,
        "PRE_DIGITS_OFFSETS": pre_digits_offs,
        "PRE_DIGITS_IND_TOKENS": pre_digits_ind_toks,
        "PRE_DIGITS_IND_OFFSETS": pre_digits_ind_offs,
        "PRE_DELIM_TOKENS": pre_delim_toks,
        "PRE_DELIM_OFFSETS": pre_delim_offs,
        "PRE_META_TOKENS": pre_meta_toks,
        "PRE_META_OFFSETS": pre_meta_offs,
        "NORM_INPUTS": NORM_INPUTS,
        "NORM_DEFAULT": norm_flags(),
        "NORM_CLEAN_ONLY": norm_flags(clean_text=True, handle_chinese_chars=False, lowercase=False, strip_accents=False),
        "NORM_LOWER_ONLY": norm_flags(clean_text=False, handle_chinese_chars=False, lowercase=True, strip_accents=False),
        "NORM_STRIP_ONLY": norm_flags(clean_text=False, handle_chinese_chars=False, lowercase=False, strip_accents=True),
        "NORM_CHINESE_ONLY": norm_flags(clean_text=False, handle_chinese_chars=True, lowercase=False, strip_accents=False),
        "UNI_CPS": UNI_CPS,
        "UNI_IS_WS": uni_ws,
        "UNI_IS_PUNCT": uni_punct,
        "UNI_IS_NUMERIC": uni_numeric,
        "UNI_IS_WORD": uni_word,
        "UNI_IS_CONTROL": uni_control,
        "UNI_IS_CHINESE": uni_chinese,
        "UNI_LOWER": uni_lower,
        "UNI_STRIP_BASE": uni_strip,
        "MODEL_WP_WORDS": MODEL_WP_WORDS,
        "MODEL_WP_IDS": MODEL_WP_IDS,
        "MODEL_WP_TOK_IDS": wp_model_ids,
        "MODEL_WP_TOK_VALUES": wp_model_values,
        "MODEL_WP_TOK_STARTS": wp_model_starts,
        "MODEL_WP_TOK_ENDS": wp_model_ends,
        "MODEL_WP_ID_TO_TOKEN": id_to_token_rows,
        "MODEL_UNI_TEXTS": MODEL_UNI_TEXTS,
        "MODEL_UNI_IDS": uni_model_ids,
        "MODEL_UNI_VALUES": uni_model_values,
        "MODEL_UNI_STARTS": uni_model_starts,
        "MODEL_UNI_ENDS": uni_model_ends,
        "MODEL_WL_TEXTS": MODEL_WL_TEXTS,
        "MODEL_WL_IDS": wl_model_ids,
        "MODEL_WL_VALUES": wl_model_values,
        "MODEL_WL_STARTS": wl_model_starts,
        "MODEL_WL_ENDS": wl_model_ends,
        "DECODE_WP_TOKENS": DECODE_WP_TOKENS,
        "DECODE_WP": decode_wp,
        "DECODE_META_TOKENS": DECODE_META_TOKENS,
        "DECODE_META": decode_meta,
        "DECODE_BF_TOKENS": DECODE_BF_TOKENS,
        "DECODE_BF": decode_bf,
        "META_HELPER_INPUTS": META_HELPER_INPUTS,
        "META_REPLACE": meta_replace,
        "META_PREPEND_INPUTS": META_PREPEND_INPUTS,
        "META_PREPEND": meta_prepend,
        "PROC_CLS": proc_cls,
        "PROC_SEP": proc_sep,
        "PROC_RAW_A_IDS": raw_a.ids,
        "PROC_RAW_A_TOKENS": raw_a.tokens,
        "PROC_RAW_A_OFFSETS": [list(o) for o in raw_a.offsets],
        "PROC_RAW_B_IDS": raw_b.ids,
        "PROC_RAW_B_TOKENS": raw_b.tokens,
        "PROC_RAW_B_OFFSETS": [list(o) for o in raw_b.offsets],
        "PROC_SINGLE_IDS": proc_single.ids,
        "PROC_SINGLE_TOKENS": proc_single.tokens,
        "PROC_SINGLE_OFFSETS": [list(o) for o in proc_single.offsets],
        "PROC_SINGLE_TYPE_IDS": proc_single.type_ids,
        "PROC_PAIR_IDS": proc_pair.ids,
        "PROC_PAIR_TOKENS": proc_pair.tokens,
        "PROC_PAIR_OFFSETS": [list(o) for o in proc_pair.offsets],
        "PROC_PAIR_TYPE_IDS": proc_pair.type_ids,
    }

    write(
        "tokenizers",
        "WordPiece, Unigram and WordLevel ids, tokens, offsets, type ids and decodes.",
        {"tokenizers": tokenizers.__version__},
        constants,
    )
    print(f"wrote packages/tokenizers/tests/expected.py ({len(constants)} constants)")


if __name__ == "__main__":
    main()
