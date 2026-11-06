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
           "cell_case", "error_case", "swapped", "entry_offset", "entry_kind", "entry_item_kind",
           "tensor_dims", "tensor_elements", "tensor_sizes", "tensor_start", "tensor_name"]

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


def fixtures():
    """Every accepted fixture the oracle wrote, in the order the suite reports them."""
    return [
        Fixture(name="base", data=Expected.BASE_BYTES, header=Expected.BASE_HEADER, byte_order=Expected.BASE_BYTE_ORDER,
                fields=Expected.BASE_FIELDS, offsets=Expected.BASE_FIELD_OFFSETS, type_offsets=Expected.BASE_FIELD_TYPE_OFFSETS, value_offsets=Expected.BASE_FIELD_VALUE_OFFSETS,
                kinds=Expected.BASE_FIELD_KINDS, item_kinds=Expected.BASE_FIELD_ITEM_KINDS,
                ints=Expected.BASE_FIELD_INTS, floats=Expected.BASE_FIELD_FLOATS, bools=Expected.BASE_FIELD_BOOLS, strings=Expected.BASE_FIELD_STRINGS,
                tensors=Expected.BASE_TENSORS, types=Expected.BASE_TENSOR_TYPES, type_ids=Expected.BASE_TENSOR_TYPE_IDS, dims=Expected.BASE_TENSOR_DIMS,
                elements=Expected.BASE_TENSOR_ELEMENTS, sizes=Expected.BASE_TENSOR_SIZES, tensor_offsets=Expected.BASE_TENSOR_OFFSETS,
                tensor_data=Expected.BASE_TENSOR_DATA, distinct=Expected.BASE_DISTINCT_TYPES, tensor_info_offset=Expected.BASE_TENSOR_INFO_OFFSET,
                tokenizer_keys=Expected.BASE_TOKENIZER_KEYS),
        Fixture(name="align", data=Expected.ALIGN_BYTES, header=Expected.ALIGN_HEADER, byte_order=Expected.ALIGN_BYTE_ORDER,
                fields=Expected.ALIGN_FIELDS, offsets=Expected.ALIGN_FIELD_OFFSETS, type_offsets=Expected.ALIGN_FIELD_TYPE_OFFSETS, value_offsets=Expected.ALIGN_FIELD_VALUE_OFFSETS,
                kinds=Expected.ALIGN_FIELD_KINDS, item_kinds=Expected.ALIGN_FIELD_ITEM_KINDS,
                ints=Expected.ALIGN_FIELD_INTS, floats=Expected.ALIGN_FIELD_FLOATS, bools=Expected.ALIGN_FIELD_BOOLS, strings=Expected.ALIGN_FIELD_STRINGS,
                tensors=Expected.ALIGN_TENSORS, types=Expected.ALIGN_TENSOR_TYPES, type_ids=Expected.ALIGN_TENSOR_TYPE_IDS, dims=Expected.ALIGN_TENSOR_DIMS,
                elements=Expected.ALIGN_TENSOR_ELEMENTS, sizes=Expected.ALIGN_TENSOR_SIZES, tensor_offsets=Expected.ALIGN_TENSOR_OFFSETS,
                tensor_data=Expected.ALIGN_TENSOR_DATA, distinct=Expected.ALIGN_DISTINCT_TYPES, tensor_info_offset=Expected.ALIGN_TENSOR_INFO_OFFSET,
                tokenizer_keys=Expected.ALIGN_TOKENIZER_KEYS),
        Fixture(name="be", data=Expected.BE_BYTES, header=Expected.BE_HEADER, byte_order=Expected.BE_BYTE_ORDER,
                fields=Expected.BE_FIELDS, offsets=Expected.BE_FIELD_OFFSETS, type_offsets=Expected.BE_FIELD_TYPE_OFFSETS, value_offsets=Expected.BE_FIELD_VALUE_OFFSETS,
                kinds=Expected.BE_FIELD_KINDS, item_kinds=Expected.BE_FIELD_ITEM_KINDS,
                ints=Expected.BE_FIELD_INTS, floats=Expected.BE_FIELD_FLOATS, bools=Expected.BE_FIELD_BOOLS, strings=Expected.BE_FIELD_STRINGS,
                tensors=Expected.BE_TENSORS, types=Expected.BE_TENSOR_TYPES, type_ids=Expected.BE_TENSOR_TYPE_IDS, dims=Expected.BE_TENSOR_DIMS,
                elements=Expected.BE_TENSOR_ELEMENTS, sizes=Expected.BE_TENSOR_SIZES, tensor_offsets=Expected.BE_TENSOR_OFFSETS,
                tensor_data=Expected.BE_TENSOR_DATA, distinct=Expected.BE_DISTINCT_TYPES, tensor_info_offset=Expected.BE_TENSOR_INFO_OFFSET,
                tokenizer_keys=Expected.BE_TOKENIZER_KEYS),
        Fixture(name="quant", data=Expected.QUANT_BYTES, header=Expected.QUANT_HEADER, byte_order=Expected.QUANT_BYTE_ORDER,
                fields=Expected.QUANT_FIELDS, offsets=Expected.QUANT_FIELD_OFFSETS, type_offsets=Expected.QUANT_FIELD_TYPE_OFFSETS, value_offsets=Expected.QUANT_FIELD_VALUE_OFFSETS,
                kinds=Expected.QUANT_FIELD_KINDS, item_kinds=Expected.QUANT_FIELD_ITEM_KINDS,
                ints=Expected.QUANT_FIELD_INTS, floats=Expected.QUANT_FIELD_FLOATS, bools=Expected.QUANT_FIELD_BOOLS, strings=Expected.QUANT_FIELD_STRINGS,
                tensors=Expected.QUANT_TENSORS, types=Expected.QUANT_TENSOR_TYPES, type_ids=Expected.QUANT_TENSOR_TYPE_IDS, dims=Expected.QUANT_TENSOR_DIMS,
                elements=Expected.QUANT_TENSOR_ELEMENTS, sizes=Expected.QUANT_TENSOR_SIZES, tensor_offsets=Expected.QUANT_TENSOR_OFFSETS,
                tensor_data=Expected.QUANT_TENSOR_DATA, distinct=Expected.QUANT_DISTINCT_TYPES, tensor_info_offset=Expected.QUANT_TENSOR_INFO_OFFSET,
                tokenizer_keys=Expected.QUANT_TOKENIZER_KEYS),
        Fixture(name="tok", data=Expected.TOK_BYTES, header=Expected.TOK_HEADER, byte_order=Expected.TOK_BYTE_ORDER,
                fields=Expected.TOK_FIELDS, offsets=Expected.TOK_FIELD_OFFSETS, type_offsets=Expected.TOK_FIELD_TYPE_OFFSETS, value_offsets=Expected.TOK_FIELD_VALUE_OFFSETS,
                kinds=Expected.TOK_FIELD_KINDS, item_kinds=Expected.TOK_FIELD_ITEM_KINDS,
                ints=Expected.TOK_FIELD_INTS, floats=Expected.TOK_FIELD_FLOATS, bools=Expected.TOK_FIELD_BOOLS, strings=Expected.TOK_FIELD_STRINGS,
                tensors=Expected.TOK_TENSORS, types=Expected.TOK_TENSOR_TYPES, type_ids=Expected.TOK_TENSOR_TYPE_IDS, dims=Expected.TOK_TENSOR_DIMS,
                elements=Expected.TOK_TENSOR_ELEMENTS, sizes=Expected.TOK_TENSOR_SIZES, tensor_offsets=Expected.TOK_TENSOR_OFFSETS,
                tensor_data=Expected.TOK_TENSOR_DATA, distinct=Expected.TOK_DISTINCT_TYPES, tensor_info_offset=Expected.TOK_TENSOR_INFO_OFFSET,
                tokenizer_keys=Expected.TOK_TOKENIZER_KEYS),
        Fixture(name="empty", data=Expected.EMPTY_BYTES, header=Expected.EMPTY_HEADER, byte_order=Expected.EMPTY_BYTE_ORDER,
                fields=Expected.EMPTY_FIELDS, offsets=Expected.EMPTY_FIELD_OFFSETS, type_offsets=Expected.EMPTY_FIELD_TYPE_OFFSETS, value_offsets=Expected.EMPTY_FIELD_VALUE_OFFSETS,
                kinds=Expected.EMPTY_FIELD_KINDS, item_kinds=Expected.EMPTY_FIELD_ITEM_KINDS,
                ints=Expected.EMPTY_FIELD_INTS, floats=Expected.EMPTY_FIELD_FLOATS, bools=Expected.EMPTY_FIELD_BOOLS, strings=Expected.EMPTY_FIELD_STRINGS,
                tensors=Expected.EMPTY_TENSORS, types=Expected.EMPTY_TENSOR_TYPES, type_ids=Expected.EMPTY_TENSOR_TYPE_IDS, dims=Expected.EMPTY_TENSOR_DIMS,
                elements=Expected.EMPTY_TENSOR_ELEMENTS, sizes=Expected.EMPTY_TENSOR_SIZES, tensor_offsets=Expected.EMPTY_TENSOR_OFFSETS,
                tensor_data=Expected.EMPTY_TENSOR_DATA, distinct=Expected.EMPTY_DISTINCT_TYPES, tensor_info_offset=Expected.EMPTY_TENSOR_INFO_OFFSET,
                tokenizer_keys=Expected.EMPTY_TOKENIZER_KEYS),
        Fixture(name="scalar", data=Expected.SCALAR_BYTES, header=Expected.SCALAR_HEADER, byte_order=Expected.SCALAR_BYTE_ORDER,
                fields=Expected.SCALAR_FIELDS, offsets=Expected.SCALAR_FIELD_OFFSETS, type_offsets=Expected.SCALAR_FIELD_TYPE_OFFSETS, value_offsets=Expected.SCALAR_FIELD_VALUE_OFFSETS,
                kinds=Expected.SCALAR_FIELD_KINDS, item_kinds=Expected.SCALAR_FIELD_ITEM_KINDS,
                ints=Expected.SCALAR_FIELD_INTS, floats=Expected.SCALAR_FIELD_FLOATS, bools=Expected.SCALAR_FIELD_BOOLS, strings=Expected.SCALAR_FIELD_STRINGS,
                tensors=Expected.SCALAR_TENSORS, types=Expected.SCALAR_TENSOR_TYPES, type_ids=Expected.SCALAR_TENSOR_TYPE_IDS, dims=Expected.SCALAR_TENSOR_DIMS,
                elements=Expected.SCALAR_TENSOR_ELEMENTS, sizes=Expected.SCALAR_TENSOR_SIZES, tensor_offsets=Expected.SCALAR_TENSOR_OFFSETS,
                tensor_data=Expected.SCALAR_TENSOR_DATA, distinct=Expected.SCALAR_DISTINCT_TYPES, tensor_info_offset=Expected.SCALAR_TENSOR_INFO_OFFSET,
                tokenizer_keys=Expected.SCALAR_TOKENIZER_KEYS),
        Fixture(name="v2", data=Expected.V2_BYTES, header=Expected.V2_HEADER, byte_order=Expected.V2_BYTE_ORDER,
                fields=Expected.V2_FIELDS, offsets=Expected.V2_FIELD_OFFSETS, type_offsets=Expected.V2_FIELD_TYPE_OFFSETS, value_offsets=Expected.V2_FIELD_VALUE_OFFSETS,
                kinds=Expected.V2_FIELD_KINDS, item_kinds=Expected.V2_FIELD_ITEM_KINDS,
                ints=Expected.V2_FIELD_INTS, floats=Expected.V2_FIELD_FLOATS, bools=Expected.V2_FIELD_BOOLS, strings=Expected.V2_FIELD_STRINGS,
                tensors=Expected.V2_TENSORS, types=Expected.V2_TENSOR_TYPES, type_ids=Expected.V2_TENSOR_TYPE_IDS, dims=Expected.V2_TENSOR_DIMS,
                elements=Expected.V2_TENSOR_ELEMENTS, sizes=Expected.V2_TENSOR_SIZES, tensor_offsets=Expected.V2_TENSOR_OFFSETS,
                tensor_data=Expected.V2_TENSOR_DATA, distinct=Expected.V2_DISTINCT_TYPES, tensor_info_offset=Expected.V2_TENSOR_INFO_OFFSET,
                tokenizer_keys=Expected.V2_TOKENIZER_KEYS),
        Fixture(name="edge", data=Expected.EDGE_BYTES, header=Expected.EDGE_HEADER, byte_order=Expected.EDGE_BYTE_ORDER,
                fields=Expected.EDGE_FIELDS, offsets=Expected.EDGE_FIELD_OFFSETS, type_offsets=Expected.EDGE_FIELD_TYPE_OFFSETS, value_offsets=Expected.EDGE_FIELD_VALUE_OFFSETS,
                kinds=Expected.EDGE_FIELD_KINDS, item_kinds=Expected.EDGE_FIELD_ITEM_KINDS,
                ints=Expected.EDGE_FIELD_INTS, floats=Expected.EDGE_FIELD_FLOATS, bools=Expected.EDGE_FIELD_BOOLS, strings=Expected.EDGE_FIELD_STRINGS,
                tensors=Expected.EDGE_TENSORS, types=Expected.EDGE_TENSOR_TYPES, type_ids=Expected.EDGE_TENSOR_TYPE_IDS, dims=Expected.EDGE_TENSOR_DIMS,
                elements=Expected.EDGE_TENSOR_ELEMENTS, sizes=Expected.EDGE_TENSOR_SIZES, tensor_offsets=Expected.EDGE_TENSOR_OFFSETS,
                tensor_data=Expected.EDGE_TENSOR_DATA, distinct=Expected.EDGE_DISTINCT_TYPES, tensor_info_offset=Expected.EDGE_TENSOR_INFO_OFFSET,
                tokenizer_keys=Expected.EDGE_TOKENIZER_KEYS),
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


def ats(xs, i):
    """Element `i` of a string list, or ""."""
    return unwrap_or(listGet(xs, i), "")


def ati(xs, i):
    """Element `i` of an int list, or 0."""
    return unwrap_or(listGet(xs, i), 0)


def atf(xs, i):
    """Element `i` of a float list, or 0.0."""
    return unwrap_or(listGet(xs, i), 0.0)


def atb(xs, i):
    """Element `i` of a bool list, or false."""
    match listGet(xs, i):
        case Success(value=value):
            return value
        case Error(message=_problem):
            return False


def atl(xs, i):
    """Element `i` of a list of lists, or the empty list."""
    return unwrap_or(listGet(xs, i), [])


def flag_int(value):
    """A boolean as the 1 or 0 an int list compares."""
    if value:
        return 1
    else:
        return 0


def first_difference(got, want):
    """The first index where two string lists differ, or the length of `want`."""
    return fold(range(len(want)), len(want), lambda found, i: found if found < len(want) else (found if ats(got, i) == ats(want, i) else i))


def str_list_equal(tag, got, want):
    """Exact equality of two string lists, naming the first difference."""
    if len(got) != len(want):
        return Check.failed(tag, "length " + toString(len(got)) + ", want " + toString(len(want)))
    else:
        i = first_difference(got, want)
        return Check.truth(tag, i == len(want), "index " + toString(i) + ": got " + ats(got, i) + ", want " + ats(want, i))


def int_rows(tag, got, want):
    """One exact int-list assertion per row of a list of int lists."""
    if len(got) != len(want):
        return [Check.failed(tag, "length " + toString(len(got)) + ", want " + toString(len(want)))]
    else:
        return toList(map(range(len(want)), lambda i: Check.ints_equal(tag + "." + toString(i), atl(got, i), atl(want, i))))


def float_rows(tag, got, want):
    """One float-list assertion per row, compared with numpy's isclose rule."""
    if len(got) != len(want):
        return [Check.failed(tag, "length " + toString(len(got)) + ", want " + toString(len(want)))]
    else:
        return toList(map(range(len(want)), lambda i: Check.close_all(tag + "." + toString(i), atl(got, i), atl(want, i), TIGHT, ATOL)))


def str_rows(tag, got, want):
    """One string-list assertion per row of a list of string lists."""
    if len(got) != len(want):
        return [Check.failed(tag, "length " + toString(len(got)) + ", want " + toString(len(want)))]
    else:
        return toList(map(range(len(want)), lambda i: str_list_equal(tag + "." + toString(i), atl(got, i), atl(want, i))))


def bool_rows(tag, got, want):
    """One bool-list assertion per row, compared as 1 and 0."""
    if len(got) != len(want):
        return [Check.failed(tag, "length " + toString(len(got)) + ", want " + toString(len(want)))]
    else:
        return toList(map(range(len(want)), lambda i: Check.ints_equal(tag + "." + toString(i), mapList(atl(got, i), flag_int), mapList(atl(want, i), flag_int))))


def swapped(fixture):
    """Whether the file's words are byte-swapped, from its recorded byte order."""
    return fixture.byte_order == "S"


def cell_case(tag, field, kind, ints, floats, bools, strings):
    """One metadata value through the accessor its declared kind selects."""
    if kind == "string":
        return text_case(tag, Reader.field_string(field), ats(strings, 0))
    elif kind == "bool":
        return bool_case(tag, Reader.field_bool(field), atb(bools, 0))
    elif kind == "float32" or kind == "float64":
        return number_case(tag, Reader.field_float(field), atf(floats, 0))
    elif kind == "array":
        return array_case(tag, field, ints, floats, bools, strings)
    else:
        return number_case(tag, Reader.field_int(field), ati(ints, 0))


def array_case(tag, field, ints, floats, bools, strings):
    """An array entry's elements through the accessor its element kind selects."""
    if field.value.item_kind == "string":
        return strings_case(tag, Reader.field_strings(field), strings)
    elif field.value.item_kind == "bool":
        return flags_case(tag, Reader.field_bools(field), bools)
    elif field.value.item_kind == "float32" or field.value.item_kind == "float64":
        return numbers_case(tag, Reader.field_floats(field), floats)
    elif field.value.item_kind == "array":
        return error_case(tag, Reader.field_ints(field))
    else:
        return ints_case(tag, Reader.field_ints(field), ints)


def number_case(tag, result, want):
    """One integer compared exactly."""
    match result:
        case Success(value=got):
            return Check.equal(tag, got, want)
        case Error(message=problem):
            return Check.failed(tag, problem)


def bool_case(tag, result, want):
    """One boolean compared exactly."""
    match result:
        case Success(value=got):
            return Check.equal(tag, got, want)
        case Error(message=problem):
            return Check.failed(tag, problem)


def text_case(tag, result, want):
    """One string compared exactly."""
    match result:
        case Success(value=got):
            return Check.equal(tag, got, want)
        case Error(message=problem):
            return Check.failed(tag, problem)


def ints_case(tag, result, want):
    """An integer list compared exactly."""
    match result:
        case Success(value=got):
            return Check.ints_equal(tag, got, want)
        case Error(message=problem):
            return Check.failed(tag, problem)


def flags_case(tag, result, want):
    """A boolean list compared exactly, as 1 and 0."""
    match result:
        case Success(value=got):
            return Check.ints_equal(tag, mapList(got, flag_int), mapList(want, flag_int))
        case Error(message=problem):
            return Check.failed(tag, problem)


def numbers_case(tag, result, want):
    """A float list compared with numpy's isclose rule."""
    match result:
        case Success(value=got):
            return Check.close_all(tag, got, want, TIGHT, ATOL)
        case Error(message=problem):
            return Check.failed(tag, problem)


def strings_case(tag, result, want):
    """A string list compared exactly."""
    match result:
        case Success(value=got):
            return str_list_equal(tag, got, want)
        case Error(message=problem):
            return Check.failed(tag, problem)


def error_case(tag, result):
    """A read that must refuse the input rather than return a value."""
    match result:
        case Success(value=_got):
            return Check.failed(tag, "the read returned a value where gguf rejects the input")
        case Error(message=_problem):
            return Check.passed(tag)


def entry_offset(field):
    """A metadata entry's starting offset."""
    return field.offset


def entry_kind(field):
    """A metadata entry's declared value kind."""
    return field.value.kind


def entry_item_kind(field):
    """A metadata entry's element kind, empty for a scalar."""
    return field.value.item_kind


def tensor_dims(info):
    """A tensor's dims as stored."""
    return info.dims


def tensor_elements(info):
    """A tensor's element count."""
    return info.n_elements


def tensor_sizes(info):
    """A tensor's byte size."""
    return info.n_bytes


def tensor_start(info):
    """Where a tensor's bytes start."""
    return info.data_offset


def tensor_name(info):
    """A tensor's name."""
    return info.name
