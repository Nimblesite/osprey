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
        declared("normalizer.type", q("NFC"), field(field(doc, "normalizer"), "type")),
        declared("pre_tokenizer.type", q("Sequence"), field(pre, "type")),
        declared("pre_tokenizer.pretokenizers[0].type", q("Split"), field(split_pt, "type")),
        declared("pre_tokenizer.pretokenizers[0].behavior", q("Isolated"), field(split_pt, "behavior")),
        declared("pre_tokenizer.pretokenizers[0].invert", "false", field(split_pt, "invert")),
        declared("pre_tokenizer.pretokenizers[0].pattern.Regex", q(QWEN_PATTERN), field(field(split_pt, "pattern"), "Regex")),
        declared("pre_tokenizer.pretokenizers[1].type", q("ByteLevel"), field(byte_pt, "type")),
        declared("pre_tokenizer.pretokenizers[1].add_prefix_space", "false", field(byte_pt, "add_prefix_space")),
        declared("pre_tokenizer.pretokenizers[1].use_regex", "false", field(byte_pt, "use_regex")),
        declared("decoder.type", q("ByteLevel"), field(decoder, "type")),
        declared("decoder.add_prefix_space", "false", field(decoder, "add_prefix_space")),
        declared("decoder.use_regex", "false", field(decoder, "use_regex")),
        declared("post_processor.type", q("ByteLevel"), field(post, "type")),
        declared("post_processor.add_prefix_space", "false", field(post, "add_prefix_space")),
        declared("post_processor.trim_offsets", "false", field(post, "trim_offsets")),
        declared("model.type", q("BPE"), field(model, "type")),
        declared("model.ignore_merges", "false", field(model, "ignore_merges")),
        declared("model.byte_fallback", "false", field(model, "byte_fallback")),
        declared("model.fuse_unk", "false", field(model, "fuse_unk")),
        declared("model.unk_token", "null", field(model, "unk_token")),
        declared("model.continuing_subword_prefix", q(""), field(model, "continuing_subword_prefix")),
        declared("model.end_of_word_suffix", q(""), field(model, "end_of_word_suffix")),
    ]

def declared(path, want, value):
    """One pipeline fact: what it is, what this port requires, and what the
    document declares, all as JSON text."""
    return [path, want, flatten(value)]

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
