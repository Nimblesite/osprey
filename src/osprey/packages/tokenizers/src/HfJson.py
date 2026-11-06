# osprey: flavor=python
# osprey: namespace=tokenizers
# osprey: module=HfJson
"""The `tokenizer.json` boundary.

`Json` values never leave this module. It reads the pipeline a document declares
and refuses one this port does not implement, so a silently different token
stream is impossible. Implements [PORT-SCOPE]: a data format is read at the edge.

This module checks the declared pipeline and stops there: the vocabulary and
merge arrays are supplied to `Models.build_bpe` as data (the oracle reads them
from the pinned tokenizer.json), so a 248,320-token vocabulary is never
materialised as Osprey lists. The divergence is recorded in the README.
"""
from tokenizers.PreTokenizers import QWEN_PATTERN
from tokenizers.Tokenizer import QwenTokenizer

__all__ = ["load_qwen_tokenizer"]

QWEN_PATTERN_MARK = "the-split-pattern"
"""Stands in the fact table for the split pattern itself, which is long."""


def load_qwen_tokenizer(text) -> Result[QwenTokenizer, str]:
    """The tokenizer a `tokenizer.json` document declares."""
    match jsonDecode(text):
        case Success(value=doc):
            return load_document(doc)
        case Error(message=message):
            return Error(message="tokenizer.json is not JSON: " + message)


def load_document(doc) -> Result[QwenTokenizer, str]:
    match check_pipeline(doc):
        case Success(value=_checked):
            return Error(message="tokenizer.json: this port reads the vocabulary and merges from supplied data, not from the document (see this module's docstring)")
        case Error(message=message):
            return Error(message=message)


def check_pipeline(doc) -> Result[bool, str]:
    """Refuse a document whose pipeline differs from the one this port implements."""
    wrong = filterList(pipeline_facts(doc), lambda row: at_str(row, 2) != at_str(row, 1))
    if len(wrong) == 0:
        return Success(value=True)
    else:
        return Error(message="tokenizer.json declares a pipeline this port does not implement: " + describe(wrong))


def pipeline_facts(doc) -> list[list[str]]:
    """Each declared pipeline fact as [what, required, declared], as JSON text."""
    pre = field(doc, "pre_tokenizer")
    split_pt = unwrap_or(jsonIndex(field(pre, "pretokenizers"), 0), JsonNull())
    byte_pt = unwrap_or(jsonIndex(field(pre, "pretokenizers"), 1), JsonNull())
    model = field(doc, "model")
    decoder = field(doc, "decoder")
    post = field(doc, "post_processor")
    return [
        ["normalizer.type", q("NFC"), flatten(field(field(doc, "normalizer"), "type"))],
        ["pre_tokenizer.type", q("Sequence"), flatten(field(pre, "type"))],
        ["pre_tokenizer.pretokenizers[0].type", q("Split"), flatten(field(split_pt, "type"))],
        ["pre_tokenizer.pretokenizers[0].behavior", q("Isolated"), flatten(field(split_pt, "behavior"))],
        ["pre_tokenizer.pretokenizers[0].invert", "false", flatten(field(split_pt, "invert"))],
        ["pre_tokenizer.pretokenizers[0].pattern.Regex", q(QWEN_PATTERN), flatten(field(field(split_pt, "pattern"), "Regex"))],
        ["pre_tokenizer.pretokenizers[1].type", q("ByteLevel"), flatten(field(byte_pt, "type"))],
        ["pre_tokenizer.pretokenizers[1].add_prefix_space", "false", flatten(field(byte_pt, "add_prefix_space"))],
        ["pre_tokenizer.pretokenizers[1].use_regex", "false", flatten(field(byte_pt, "use_regex"))],
        ["decoder.type", q("ByteLevel"), flatten(field(decoder, "type"))],
        ["decoder.add_prefix_space", "false", flatten(field(decoder, "add_prefix_space"))],
        ["decoder.use_regex", "false", flatten(field(decoder, "use_regex"))],
        ["post_processor.type", q("ByteLevel"), flatten(field(post, "type"))],
        ["post_processor.add_prefix_space", "false", flatten(field(post, "add_prefix_space"))],
        ["post_processor.trim_offsets", "false", flatten(field(post, "trim_offsets"))],
        ["model.type", q("BPE"), flatten(field(model, "type"))],
        ["model.ignore_merges", "false", flatten(field(model, "ignore_merges"))],
        ["model.byte_fallback", "false", flatten(field(model, "byte_fallback"))],
        ["model.fuse_unk", "false", flatten(field(model, "fuse_unk"))],
        ["model.unk_token", "null", flatten(field(model, "unk_token"))],
        ["model.continuing_subword_prefix", q(""), flatten(field(model, "continuing_subword_prefix"))],
        ["model.end_of_word_suffix", q(""), flatten(field(model, "end_of_word_suffix"))],
    ]


def q(text):
    """A string's JSON text, which is how every declared value is compared."""
    return unwrap_or(jsonQuote(text), "")


def describe(rows):
    return join(mapList(rows, lambda row: at_str(row, 0) + " is " + at_str(row, 2) + ", want " + at_str(row, 1)), "; ")


def at_str(row, i):
    return unwrap_or(listGet(row, i), "")


def flatten(value) -> string:
    """A JSON value as its canonical text, so a declared shape compares as a string."""
    return unwrap_or(jsonEncode(value), "")


def field(value, name):
    """The member called name, or JsonNull."""
    return unwrap_or(jsonField(value, name), JsonNull())
