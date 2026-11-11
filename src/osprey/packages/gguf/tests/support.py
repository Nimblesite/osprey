# osprey: flavor=python
# osprey: namespace=gguf_tests
# osprey: module=Support
"""The table of accepted GGUF fixtures and the comparisons the gguf suite reports
through. Every expected list here is a constant the CPython oracle wrote from the
real `gguf` 0.19.0 reader ([PORT-TEST-ORACLE]); nothing is retyped.
"""
from gguf import Reader
from gguf_expected import Expected
from testing import Check

__all__ = ["TIGHT", "ATOL", "Rejected", "Fixture", "QuantCase", "fixtures", "quant_types", "empty_file", "loaded", "str_list_equal", "int_rows", "float_rows", "str_rows", "bool_rows", "ats", "ati", "atf", "atb", "atl", "flag_int",
           "cell_case", "error_case", "swapped", "equal_case", "checked"]

TIGHT = 0.000000000001
"""The float tolerance: these are bit-exact decodes, so nothing downstream rounds."""
ATOL = 0.0
"""The float tolerance's absolute term, zero for the same reason."""


class Rejected:
    """One input gguf refuses: its name, its bytes and the message it recorded."""
    name: str
    data: list[int]
    message: str


class Fixture:
    """One accepted GGUF file with every value the oracle recorded for it."""
    name: str
    data: list[int]
    header: list[int]
    byte_order: str
    fields: list[str]
    offsets: list[int]
    type_offsets: list[int]
    value_offsets: list[int]
    kinds: list[str]
    item_kinds: list[str]
    ints: list[list[int]]
    floats: list[list[float]]
    bools: list[list[bool]]
    strings: list[list[str]]
    tensors: list[str]
    types: list[str]
    type_ids: list[int]
    dims: list[list[int]]
    elements: list[int]
    sizes: list[int]
    tensor_offsets: list[int]
    tensor_data: list[list[int]]
    distinct: list[str]
    tensor_info_offset: int
    tokenizer_keys: list[str]


def fixture(name, data, header, byte_order, fields, offsets, type_offsets, value_offsets, kinds, item_kinds,
            ints, floats, bools, strings, tensors, types, type_ids, dims, elements, sizes, tensor_offsets,
            tensor_data, distinct, tensor_info_offset, tokenizer_keys):
    """One fixture from the oracle constants its prefix names, so the table below reads
    as data instead of repeating the record's keywords nine times over."""
    return Fixture(name=name, data=data, header=header, byte_order=byte_order, fields=fields, offsets=offsets, type_offsets=type_offsets,
        value_offsets=value_offsets, kinds=kinds, item_kinds=item_kinds, ints=ints, floats=floats, bools=bools, strings=strings, tensors=tensors,
        types=types, type_ids=type_ids, dims=dims, elements=elements, sizes=sizes, tensor_offsets=tensor_offsets, tensor_data=tensor_data,
        distinct=distinct, tensor_info_offset=tensor_info_offset, tokenizer_keys=tokenizer_keys)


def fixtures():
    """Every accepted fixture the oracle wrote, in the order the suite reports them."""
    return [
        fixture("base", Expected.BASE_BYTES, Expected.BASE_HEADER, Expected.BASE_BYTE_ORDER, Expected.BASE_FIELDS, Expected.BASE_FIELD_OFFSETS, Expected.BASE_FIELD_TYPE_OFFSETS, Expected.BASE_FIELD_VALUE_OFFSETS, Expected.BASE_FIELD_KINDS, Expected.BASE_FIELD_ITEM_KINDS, Expected.BASE_FIELD_INTS, Expected.BASE_FIELD_FLOATS, Expected.BASE_FIELD_BOOLS, Expected.BASE_FIELD_STRINGS, Expected.BASE_TENSORS, Expected.BASE_TENSOR_TYPES, Expected.BASE_TENSOR_TYPE_IDS, Expected.BASE_TENSOR_DIMS, Expected.BASE_TENSOR_ELEMENTS, Expected.BASE_TENSOR_SIZES, Expected.BASE_TENSOR_OFFSETS, Expected.BASE_TENSOR_DATA, Expected.BASE_DISTINCT_TYPES, Expected.BASE_TENSOR_INFO_OFFSET, Expected.BASE_TOKENIZER_KEYS),
        fixture("align", Expected.ALIGN_BYTES, Expected.ALIGN_HEADER, Expected.ALIGN_BYTE_ORDER, Expected.ALIGN_FIELDS, Expected.ALIGN_FIELD_OFFSETS, Expected.ALIGN_FIELD_TYPE_OFFSETS, Expected.ALIGN_FIELD_VALUE_OFFSETS, Expected.ALIGN_FIELD_KINDS, Expected.ALIGN_FIELD_ITEM_KINDS, Expected.ALIGN_FIELD_INTS, Expected.ALIGN_FIELD_FLOATS, Expected.ALIGN_FIELD_BOOLS, Expected.ALIGN_FIELD_STRINGS, Expected.ALIGN_TENSORS, Expected.ALIGN_TENSOR_TYPES, Expected.ALIGN_TENSOR_TYPE_IDS, Expected.ALIGN_TENSOR_DIMS, Expected.ALIGN_TENSOR_ELEMENTS, Expected.ALIGN_TENSOR_SIZES, Expected.ALIGN_TENSOR_OFFSETS, Expected.ALIGN_TENSOR_DATA, Expected.ALIGN_DISTINCT_TYPES, Expected.ALIGN_TENSOR_INFO_OFFSET, Expected.ALIGN_TOKENIZER_KEYS),
        fixture("be", Expected.BE_BYTES, Expected.BE_HEADER, Expected.BE_BYTE_ORDER, Expected.BE_FIELDS, Expected.BE_FIELD_OFFSETS, Expected.BE_FIELD_TYPE_OFFSETS, Expected.BE_FIELD_VALUE_OFFSETS, Expected.BE_FIELD_KINDS, Expected.BE_FIELD_ITEM_KINDS, Expected.BE_FIELD_INTS, Expected.BE_FIELD_FLOATS, Expected.BE_FIELD_BOOLS, Expected.BE_FIELD_STRINGS, Expected.BE_TENSORS, Expected.BE_TENSOR_TYPES, Expected.BE_TENSOR_TYPE_IDS, Expected.BE_TENSOR_DIMS, Expected.BE_TENSOR_ELEMENTS, Expected.BE_TENSOR_SIZES, Expected.BE_TENSOR_OFFSETS, Expected.BE_TENSOR_DATA, Expected.BE_DISTINCT_TYPES, Expected.BE_TENSOR_INFO_OFFSET, Expected.BE_TOKENIZER_KEYS),
        fixture("quant", Expected.QUANT_BYTES, Expected.QUANT_HEADER, Expected.QUANT_BYTE_ORDER, Expected.QUANT_FIELDS, Expected.QUANT_FIELD_OFFSETS, Expected.QUANT_FIELD_TYPE_OFFSETS, Expected.QUANT_FIELD_VALUE_OFFSETS, Expected.QUANT_FIELD_KINDS, Expected.QUANT_FIELD_ITEM_KINDS, Expected.QUANT_FIELD_INTS, Expected.QUANT_FIELD_FLOATS, Expected.QUANT_FIELD_BOOLS, Expected.QUANT_FIELD_STRINGS, Expected.QUANT_TENSORS, Expected.QUANT_TENSOR_TYPES, Expected.QUANT_TENSOR_TYPE_IDS, Expected.QUANT_TENSOR_DIMS, Expected.QUANT_TENSOR_ELEMENTS, Expected.QUANT_TENSOR_SIZES, Expected.QUANT_TENSOR_OFFSETS, Expected.QUANT_TENSOR_DATA, Expected.QUANT_DISTINCT_TYPES, Expected.QUANT_TENSOR_INFO_OFFSET, Expected.QUANT_TOKENIZER_KEYS),
        fixture("tok", Expected.TOK_BYTES, Expected.TOK_HEADER, Expected.TOK_BYTE_ORDER, Expected.TOK_FIELDS, Expected.TOK_FIELD_OFFSETS, Expected.TOK_FIELD_TYPE_OFFSETS, Expected.TOK_FIELD_VALUE_OFFSETS, Expected.TOK_FIELD_KINDS, Expected.TOK_FIELD_ITEM_KINDS, Expected.TOK_FIELD_INTS, Expected.TOK_FIELD_FLOATS, Expected.TOK_FIELD_BOOLS, Expected.TOK_FIELD_STRINGS, Expected.TOK_TENSORS, Expected.TOK_TENSOR_TYPES, Expected.TOK_TENSOR_TYPE_IDS, Expected.TOK_TENSOR_DIMS, Expected.TOK_TENSOR_ELEMENTS, Expected.TOK_TENSOR_SIZES, Expected.TOK_TENSOR_OFFSETS, Expected.TOK_TENSOR_DATA, Expected.TOK_DISTINCT_TYPES, Expected.TOK_TENSOR_INFO_OFFSET, Expected.TOK_TOKENIZER_KEYS),
        fixture("empty", Expected.EMPTY_BYTES, Expected.EMPTY_HEADER, Expected.EMPTY_BYTE_ORDER, Expected.EMPTY_FIELDS, Expected.EMPTY_FIELD_OFFSETS, Expected.EMPTY_FIELD_TYPE_OFFSETS, Expected.EMPTY_FIELD_VALUE_OFFSETS, Expected.EMPTY_FIELD_KINDS, Expected.EMPTY_FIELD_ITEM_KINDS, Expected.EMPTY_FIELD_INTS, Expected.EMPTY_FIELD_FLOATS, Expected.EMPTY_FIELD_BOOLS, Expected.EMPTY_FIELD_STRINGS, Expected.EMPTY_TENSORS, Expected.EMPTY_TENSOR_TYPES, Expected.EMPTY_TENSOR_TYPE_IDS, Expected.EMPTY_TENSOR_DIMS, Expected.EMPTY_TENSOR_ELEMENTS, Expected.EMPTY_TENSOR_SIZES, Expected.EMPTY_TENSOR_OFFSETS, Expected.EMPTY_TENSOR_DATA, Expected.EMPTY_DISTINCT_TYPES, Expected.EMPTY_TENSOR_INFO_OFFSET, Expected.EMPTY_TOKENIZER_KEYS),
        fixture("scalar", Expected.SCALAR_BYTES, Expected.SCALAR_HEADER, Expected.SCALAR_BYTE_ORDER, Expected.SCALAR_FIELDS, Expected.SCALAR_FIELD_OFFSETS, Expected.SCALAR_FIELD_TYPE_OFFSETS, Expected.SCALAR_FIELD_VALUE_OFFSETS, Expected.SCALAR_FIELD_KINDS, Expected.SCALAR_FIELD_ITEM_KINDS, Expected.SCALAR_FIELD_INTS, Expected.SCALAR_FIELD_FLOATS, Expected.SCALAR_FIELD_BOOLS, Expected.SCALAR_FIELD_STRINGS, Expected.SCALAR_TENSORS, Expected.SCALAR_TENSOR_TYPES, Expected.SCALAR_TENSOR_TYPE_IDS, Expected.SCALAR_TENSOR_DIMS, Expected.SCALAR_TENSOR_ELEMENTS, Expected.SCALAR_TENSOR_SIZES, Expected.SCALAR_TENSOR_OFFSETS, Expected.SCALAR_TENSOR_DATA, Expected.SCALAR_DISTINCT_TYPES, Expected.SCALAR_TENSOR_INFO_OFFSET, Expected.SCALAR_TOKENIZER_KEYS),
        fixture("v2", Expected.V2_BYTES, Expected.V2_HEADER, Expected.V2_BYTE_ORDER, Expected.V2_FIELDS, Expected.V2_FIELD_OFFSETS, Expected.V2_FIELD_TYPE_OFFSETS, Expected.V2_FIELD_VALUE_OFFSETS, Expected.V2_FIELD_KINDS, Expected.V2_FIELD_ITEM_KINDS, Expected.V2_FIELD_INTS, Expected.V2_FIELD_FLOATS, Expected.V2_FIELD_BOOLS, Expected.V2_FIELD_STRINGS, Expected.V2_TENSORS, Expected.V2_TENSOR_TYPES, Expected.V2_TENSOR_TYPE_IDS, Expected.V2_TENSOR_DIMS, Expected.V2_TENSOR_ELEMENTS, Expected.V2_TENSOR_SIZES, Expected.V2_TENSOR_OFFSETS, Expected.V2_TENSOR_DATA, Expected.V2_DISTINCT_TYPES, Expected.V2_TENSOR_INFO_OFFSET, Expected.V2_TOKENIZER_KEYS),
        fixture("edge", Expected.EDGE_BYTES, Expected.EDGE_HEADER, Expected.EDGE_BYTE_ORDER, Expected.EDGE_FIELDS, Expected.EDGE_FIELD_OFFSETS, Expected.EDGE_FIELD_TYPE_OFFSETS, Expected.EDGE_FIELD_VALUE_OFFSETS, Expected.EDGE_FIELD_KINDS, Expected.EDGE_FIELD_ITEM_KINDS, Expected.EDGE_FIELD_INTS, Expected.EDGE_FIELD_FLOATS, Expected.EDGE_FIELD_BOOLS, Expected.EDGE_FIELD_STRINGS, Expected.EDGE_TENSORS, Expected.EDGE_TENSOR_TYPES, Expected.EDGE_TENSOR_TYPE_IDS, Expected.EDGE_TENSOR_DIMS, Expected.EDGE_TENSOR_ELEMENTS, Expected.EDGE_TENSOR_SIZES, Expected.EDGE_TENSOR_OFFSETS, Expected.EDGE_TENSOR_DATA, Expected.EDGE_DISTINCT_TYPES, Expected.EDGE_TENSOR_INFO_OFFSET, Expected.EDGE_TOKENIZER_KEYS),
    ]


class QuantCase:
    """One tensor type's stored blocks, and the values ggml's decoder wrote for them,
    as the oracle's comma-joined text."""
    name: str
    block: int
    size: int
    data: list[int]
    values: str


def quant_types():
    """Every block type the port decodes, in the order the suite reports them."""
    return [
        QuantCase(name="F32", block=Expected.DEQUANT_F32_BLOCK, size=Expected.DEQUANT_F32_SIZE, data=Expected.DEQUANT_F32_BYTES, values=Expected.DEQUANT_F32_VALUES),
        QuantCase(name="Q8_0", block=Expected.DEQUANT_Q8_0_BLOCK, size=Expected.DEQUANT_Q8_0_SIZE, data=Expected.DEQUANT_Q8_0_BYTES, values=Expected.DEQUANT_Q8_0_VALUES),
        QuantCase(name="Q3_K", block=Expected.DEQUANT_Q3_K_BLOCK, size=Expected.DEQUANT_Q3_K_SIZE, data=Expected.DEQUANT_Q3_K_BYTES, values=Expected.DEQUANT_Q3_K_VALUES),
        QuantCase(name="Q4_K", block=Expected.DEQUANT_Q4_K_BLOCK, size=Expected.DEQUANT_Q4_K_SIZE, data=Expected.DEQUANT_Q4_K_BYTES, values=Expected.DEQUANT_Q4_K_VALUES),
        QuantCase(name="Q5_K", block=Expected.DEQUANT_Q5_K_BLOCK, size=Expected.DEQUANT_Q5_K_SIZE, data=Expected.DEQUANT_Q5_K_BYTES, values=Expected.DEQUANT_Q5_K_VALUES),
        QuantCase(name="Q6_K", block=Expected.DEQUANT_Q6_K_BLOCK, size=Expected.DEQUANT_Q6_K_SIZE, data=Expected.DEQUANT_Q6_K_BYTES, values=Expected.DEQUANT_Q6_K_VALUES),
        QuantCase(name="IQ4_NL", block=Expected.DEQUANT_IQ4_NL_BLOCK, size=Expected.DEQUANT_IQ4_NL_SIZE, data=Expected.DEQUANT_IQ4_NL_BYTES, values=Expected.DEQUANT_IQ4_NL_VALUES),
        QuantCase(name="IQ4_XS", block=Expected.DEQUANT_IQ4_XS_BLOCK, size=Expected.DEQUANT_IQ4_XS_SIZE, data=Expected.DEQUANT_IQ4_XS_BYTES, values=Expected.DEQUANT_IQ4_XS_VALUES),
        QuantCase(name="IQ3_S", block=Expected.DEQUANT_IQ3_S_BLOCK, size=Expected.DEQUANT_IQ3_S_SIZE, data=Expected.DEQUANT_IQ3_S_BYTES, values=Expected.DEQUANT_IQ3_S_VALUES),
    ]


def empty_file():
    """The file a failed load falls back to, so every assertion still reports."""
    return Reader.GGUFFile(data=[], version=0, tensor_count=0, kv_count=0, alignment=0, data_offset=0, byte_order="", fields=[], tensors=[])


def loaded(fixture):
    """The parsed file, or the empty fallback that fails every assertion for it."""
    match Reader.read(fixture.data):
        case Success(value=file):
            return file
        case Error(message=_problem):
            return empty_file()


def at_or(xs, i, fallback):
    """Element `i` of a list, or `fallback` when the index is past the end."""
    return unwrap_or(listGet(xs, i), fallback)


def ats(xs, i):
    """Element `i` of a string list, or ""."""
    return at_or(xs, i, "")


def ati(xs, i):
    """Element `i` of an int list, or 0."""
    return at_or(xs, i, 0)


def atf(xs, i):
    """Element `i` of a float list, or 0.0."""
    return at_or(xs, i, 0.0)


def atb(xs, i):
    """Element `i` of a bool list, or false."""
    return at_or(xs, i, False)


def atl(xs, i):
    """Element `i` of a list of lists, or the empty list."""
    return at_or(xs, i, [])


def flag_int(value):
    """A boolean as the 1 or 0 an int list compares."""
    if value:
        return 1
    else:
        return 0


def first_difference(got, want):
    """The first index where two string lists differ, or the length of `want`."""
    return fold(range(len(want)), len(want), lambda found, i: found if found < len(want) else (found if ats(got, i) == ats(want, i) else i))


def length_failure(tag, got, want):
    """A failure naming both lengths, so a short list never checks nothing."""
    return Check.failed(tag, "length " + toString(len(got)) + ", want " + toString(len(want)))


def str_list_equal(tag, got, want):
    """Exact equality of two string lists, naming the first difference."""
    if len(got) != len(want):
        return length_failure(tag, got, want)
    else:
        i = first_difference(got, want)
        return Check.truth(tag, i == len(want), "index " + toString(i) + ": got " + ats(got, i) + ", want " + ats(want, i))


def rows(tag, got, want, compare):
    """One `compare` assertion per row of `want`, numbered, or a single failure when
    the row counts differ — a short list of rows would otherwise check nothing."""
    if len(got) != len(want):
        return [length_failure(tag, got, want)]
    else:
        return toList(map(range(len(want)), lambda i: compare(tag + "." + toString(i), atl(got, i), atl(want, i))))


def int_rows(tag, got, want):
    """One exact int-list assertion per row of a list of int lists."""
    return rows(tag, got, want, lambda name, row, expected: Check.ints_equal(name, row, expected))


def float_rows(tag, got, want):
    """One float-list assertion per row, compared with numpy's isclose rule."""
    return rows(tag, got, want, lambda name, row, expected: Check.close_all(name, row, expected, TIGHT, ATOL))


def str_rows(tag, got, want):
    """One string-list assertion per row of a list of string lists."""
    return rows(tag, got, want, lambda name, row, expected: str_list_equal(name, row, expected))


def bool_rows(tag, got, want):
    """One bool-list assertion per row, compared as 1 and 0."""
    return rows(tag, got, want, lambda name, row, expected: Check.ints_equal(name, mapList(row, flag_int), mapList(expected, flag_int)))


def swapped(fixture):
    """Whether the file's words are byte-swapped, from its recorded byte order."""
    return fixture.byte_order == "S"


def float_kind(kind):
    """Whether a declared value or element kind is one of the two float types."""
    return kind == "float32" or kind == "float64"


def cell_case(tag, field, kind, ints, floats, bools, strings):
    """One metadata value through the accessor its declared kind selects."""
    if kind == "string":
        return equal_case(tag, Reader.field_string(field), ats(strings, 0))
    elif kind == "bool":
        return equal_case(tag, Reader.field_bool(field), atb(bools, 0))
    elif float_kind(kind):
        return equal_case(tag, Reader.field_float(field), atf(floats, 0))
    elif kind == "array":
        return array_case(tag, field, ints, floats, bools, strings)
    else:
        return equal_case(tag, Reader.field_int(field), ati(ints, 0))


def array_case(tag, field, ints, floats, bools, strings):
    """An array entry's elements through the accessor its element kind selects."""
    if field.value.item_kind == "string":
        return strings_case(tag, Reader.field_strings(field), strings)
    elif field.value.item_kind == "bool":
        return flags_case(tag, Reader.field_bools(field), bools)
    elif float_kind(field.value.item_kind):
        return numbers_case(tag, Reader.field_floats(field), floats)
    elif field.value.item_kind == "array":
        return error_case(tag, Reader.field_ints(field))
    else:
        return ints_case(tag, Reader.field_ints(field), ints)


def checked(tag, result, compare):
    """`compare(tag, got)` for the value a read produced, or a failure carrying the
    message the refusal recorded."""
    match result:
        case Success(value=got):
            return compare(tag, got)
        case Error(message=problem):
            return Check.failed(tag, problem)


def equal_case(tag, result, want):
    """One scalar — an integer, a boolean or a string — compared exactly."""
    return checked(tag, result, lambda t, got: Check.equal(t, got, want))


def ints_case(tag, result, want):
    """An integer list compared exactly."""
    return checked(tag, result, lambda t, got: Check.ints_equal(t, got, want))


def flags_case(tag, result, want):
    """A boolean list compared exactly, as 1 and 0."""
    return checked(tag, result, lambda t, got: Check.ints_equal(t, mapList(got, flag_int), mapList(want, flag_int)))


def numbers_case(tag, result, want):
    """A float list compared with numpy's isclose rule."""
    return checked(tag, result, lambda t, got: Check.close_all(t, got, want, TIGHT, ATOL))


def strings_case(tag, result, want):
    """A string list compared exactly."""
    return checked(tag, result, lambda t, got: str_list_equal(t, got, want))


def error_case(tag, result):
    """A read that must refuse the input rather than return a value."""
    match result:
        case Success(value=_got):
            return Check.failed(tag, "the read returned a value where gguf rejects the input")
        case Error(message=_problem):
            return Check.passed(tag)








