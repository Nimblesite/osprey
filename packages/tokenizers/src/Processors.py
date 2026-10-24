# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=tokenizers
# osprey: module=Processors
"""The TemplateProcessing post-processor for the BERT template.

Implements tokenizers' TemplateProcessing semantics for the templates
single = `[CLS]:0 $A:0 [SEP]:0` and pair = `[CLS]:0 $A:0 [SEP]:0 $B:1 [SEP]:1`.
Special tokens get offset (0, 0); sequence A tokens get type id 0 and sequence
B tokens type id 1, with the final [SEP] also 1.
"""

__all__ = [
    "Offset", "Encoding", "empty_encoding", "concat_two", "bert_template",
]


class Offset:
    """A [start, end) span in the original input (char offsets in an Encoding)."""
    start: int
    end: int


class Encoding:
    """ids, tokens, offsets and type ids of one encoded sequence."""
    ids: list[int]
    tokens: list[str]
    offsets: list[Offset]
    type_ids: list[int]


def empty_encoding():
    """An Encoding with no tokens."""
    return Encoding(ids=[], tokens=[], offsets=[], type_ids=[])


def set_type_ids(enc, tid):
    """A copy of enc whose every token has type id `tid`."""
    return Encoding(ids=enc.ids, tokens=enc.tokens, offsets=enc.offsets, type_ids=toList(map(range(len(enc.ids)), lambda _: tid)))


def special_encoding(id, token, tid):
    """A one-token special Encoding at offset (0, 0)."""
    return Encoding(ids=[id], tokens=[token], offsets=[Offset(start=0, end=0)], type_ids=[tid])


def concat_encodings(encs):
    """One Encoding concatenating a list of Encodings."""
    return fold(range(len(encs)), empty_encoding(), lambda acc, i: concat_two(acc, unwrap_or(listGet(encs, i), empty_encoding())))


def concat_two(a, b):
    """One Encoding concatenating a and b."""
    return Encoding(
        ids=listConcat(a.ids, b.ids),
        tokens=listConcat(a.tokens, b.tokens),
        offsets=listConcat(a.offsets, b.offsets),
        type_ids=listConcat(a.type_ids, b.type_ids),
    )


def bert_template(encodings, cls_id, sep_id):
    """[CLS] $A [SEP] for one sequence, [CLS] $A [SEP] $B [SEP] for a pair,
    with type ids 0 for A and 1 for B."""
    if len(encodings) == 1:
        a = set_type_ids(unwrap_or(listGet(encodings, 0), empty_encoding()), 0)
        return concat_encodings([special_encoding(cls_id, "[CLS]", 0), a, special_encoding(sep_id, "[SEP]", 0)])
    else:
        a = set_type_ids(unwrap_or(listGet(encodings, 0), empty_encoding()), 0)
        b = set_type_ids(unwrap_or(listGet(encodings, 1), empty_encoding()), 1)
        return concat_encodings([
            special_encoding(cls_id, "[CLS]", 0),
            a,
            special_encoding(sep_id, "[SEP]", 0),
            b,
            special_encoding(sep_id, "[SEP]", 1),
        ])
