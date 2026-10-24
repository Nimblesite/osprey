# osprey: flavor=python
# osprey: namespace=tiktoken_tests
# osprey: module=Contracts
"""Direct byte, Unicode and vocabulary contracts alongside the CPython oracle."""
from testing import Check
from tiktoken import Bpe, Core, Educational, Pattern, Tables, Unicode

__all__ = ["cases"]


def unicode_cases():
    return [
        Check.equal("helpers.at_str", Unicode.at_str(["é"], 0), "é"),
        Check.ints_equal("helpers.at_list", Unicode.at_list([[1, 2]], 0), [1, 2]),
        Check.equal("helpers.max_int", Unicode.max_int(-1, 2), 2),
        Check.equal("helpers.min_int", Unicode.min_int(-1, 2), -1),
        Check.ints_equal("helpers.code_points", Unicode.code_points("é😀"), [233, 128512]),
        Check.equal("helpers.cp_len", Unicode.cp_len("é😀"), 2),
        Check.equal("helpers.is_letter", Unicode.is_letter(233), True),
        Check.equal("helpers.is_number", Unicode.is_number(1633), True),
        Check.equal("helpers.is_space", Unicode.is_space(160), True),
        Check.equal("helpers.is_other", Unicode.is_other(33), True),
        Check.equal("helpers.cmp_text", Unicode.cmp_text("a", "b"), -1),
    ]


def utf8_cases():
    return [
        Check.ints_equal("helpers.utf8_of_cp", Unicode.utf8_of_cp(233, [65]), [65, 195, 169]),
        Check.ints_equal("helpers.utf8_range", Unicode.utf8_range([65, 233, 66], 1, 2), [195, 169]),
        Check.ints_equal("helpers.utf8_text", Unicode.utf8_text("é😀"), [195, 169, 240, 159, 152, 128]),
        Check.equal("helpers.decode_utf8", unwrap_or(Unicode.decode_utf8([195, 169], "strict"), ""), "é"),
        Check.equal("helpers.decode_replace", unwrap_or(Unicode.decode_utf8([255, 65], "replace"), ""), "�A"),
        Check.ints_equal("tables.LETTER_RANGES", Unicode.at_list(Tables.LETTER_RANGES, 0), [65, 90]),
        Check.ints_equal("tables.NUMBER_RANGES", Unicode.at_list(Tables.NUMBER_RANGES, 0), [48, 57]),
        Check.ints_equal("tables.SPACE_RANGES", Unicode.at_list(Tables.SPACE_RANGES, 0), [9, 13]),
    ]


def bpe_cases():
    a = Bpe.key([97])
    b = Bpe.key([98])
    pair = Bpe.Pair(first=a, second=b)
    entries = Bpe.entries_by_rank({a: 5, b: 2})
    best = Bpe.Best(rank=Bpe.NO_RANK, index=-1)
    return [
        Check.ints_equal("helpers.bytes_of_key", Bpe.bytes_of_key(Bpe.key([0, 97, 255])), [0, 97, 255]),
        Check.equal("helpers.pair_key", Bpe.pair_key(pair.first, pair.second), "bĀc"),
        Check.equal("helpers.SEP", Bpe.SEP, "Ā"),
        Check.equal("helpers.Best", best.rank, 2147483647),
        Check.equal("helpers.Best.index", best.index, -1),
        Check.equal("helpers.entries_by_rank", unwrap_or(listGet(entries, 0), Bpe.Ranked(key="", rank=-1)).rank, 2),
        Check.equal("helpers.key_of_rank", unwrap_or(Bpe.key_of_rank(entries, 5, 0, 1), ""), a),
        Check.equal("helpers.merge_all", join(Bpe.merge_all([a, b, a, b], a, b, 0, []), ","), "bc,bc"),
    ]


def special_case():
    want = Bpe.Hit(start=1, token="<x>", next=4)
    got = Bpe.earliest_special(["<x>"], Unicode.code_points("a<x>b"), 0)
    return [Check.equal("helpers.earliest_special.start", got.start, want.start), Check.equal("helpers.earliest_special.next", got.next, want.next), Check.equal("helpers.earliest_special.token", got.token, want.token)]


def simple_alias() -> Educational.SimpleBytePairEncoding:
    """The annotation tests resolution of the exported cross-module type alias."""
    return Educational.simple(Pattern.Gpt2(), Bpe.base_ranks())


def encoding_cases():
    base = simple_alias()
    pieces = Pattern.split_cps(Pattern.Gpt2(), Unicode.code_points("ab 12"), 0, 5)
    return [
        Check.ints_equal("helpers.SimpleBytePairEncoding", unwrap_or(Core.encode_ordinary(base, "ab"), []), [97, 98]),
        Check.ints_equal("helpers.encode_range", unwrap_or(Core.encode_range(base.ranks, base.pattern, [122, 97, 98], 1, 3), []), [97, 98]),
        Check.equal("helpers.split_cps", join(mapList(pieces, Unicode.ints_text), "|"), "[97, 98]|[32, 49, 50]"),
    ]


def cases():
    return foldList([unicode_cases(), utf8_cases(), bpe_cases(), special_case(), encoding_cases()], [], listConcat)
