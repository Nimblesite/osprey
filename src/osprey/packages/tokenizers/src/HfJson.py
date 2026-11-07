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
    split_pt = pt_at(pre, 0)
    head = [
        member_fact("normalizer", field(doc, "normalizer"), "type", q("NFC")),
        member_fact("pre_tokenizer", pre, "type", q("Sequence")),
        member_fact("pre_tokenizer.pretokenizers[0]", split_pt, "type", q("Split")),
        member_fact("pre_tokenizer.pretokenizers[0]", split_pt, "behavior", q("Isolated")),
        member_fact("pre_tokenizer.pretokenizers[0]", split_pt, "invert", "false"),
        declared("pre_tokenizer.pretokenizers[0].pattern.Regex", q(QWEN_PATTERN), field(field(split_pt, "pattern"), "Regex")),
    ]
    return concat_facts([
        head,
        bytelevel_facts("pre_tokenizer.pretokenizers[1]", pt_at(pre, 1), "use_regex"),
        bytelevel_facts("decoder", field(doc, "decoder"), "use_regex"),
        bytelevel_facts("post_processor", field(doc, "post_processor"), "trim_offsets"),
        model_facts(field(doc, "model")),
    ])

def pt_at(pre, i):
    """Pre-tokenizer i of the declared sequence, or JsonNull when absent."""
    return unwrap_or(jsonIndex(field(pre, "pretokenizers"), i), JsonNull())

def member_fact(path, obj, name, want):
    """One fact: obj's member `name` must be the JSON text `want`."""
    return declared(path + "." + name, want, field(obj, name))

def stage_facts(path, obj, wants):
    """The facts of one pipeline stage: each [member, required JSON text]."""
    return mapList(wants, lambda row: member_fact(path, obj, at_str(row, 0), at_str(row, 1)))

def bytelevel_facts(path, obj, last):
    """What every ByteLevel stage declares: its type, no prefix space, and `last` off."""
    return stage_facts(path, obj, [["type", q("ByteLevel")], ["add_prefix_space", "false"], [last, "false"]])

def model_facts(model):
    """What the BPE model declares: its type and every option this port pins."""
    return stage_facts("model", model, [
        ["type", q("BPE")],
        ["ignore_merges", "false"],
        ["byte_fallback", "false"],
        ["fuse_unk", "false"],
        ["unk_token", "null"],
        ["continuing_subword_prefix", q("")],
        ["end_of_word_suffix", q("")],
    ])

def concat_facts(groups):
    """Fact groups joined into one table, in order."""
    return fold(range(len(groups)), [], lambda acc, i: listConcat(acc, unwrap_or(listGet(groups, i), [])))

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
