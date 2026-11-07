# osprey: flavor=python
"""@osprey/safetensors suite: load, decode every dtype, keys, metadata, slices
and the Rust crate's validation errors, all against safetensors 0.8.0."""
from testing import Check
from safetensors import Bytes, Dtype, JsonParser, SafeTensors
from safetensors_expected import Expected

RTOL = 0.000000001
ATOL = 0.00000000000001
F16_WORD = [0, 62]
BF16_WORD = [192, 63]
F32_WORD = [0, 0, 192, 63]
F64_WORD = [0, 0, 0, 0, 0, 0, 248, 63]


def empty_file():
    return SafeTensors.SafeFile(data=[], data_offset=0, tensors=[], metadata={})


def empty_tensor():
    return SafeTensors.DecodedTensor(kind="none", floats=[], ints=[])


def concat_all(xs):
    """Concatenate a list of lists into one list."""
    return fold(range(len(xs)), [], lambda acc, i: listConcat(acc, unwrap_or(listGet(xs, i), [])))


def file_of(data):
    """The loaded file, or the empty fallback that fails every assertion for it."""
    return unwrap_or(SafeTensors.load(data), empty_file())


def tensor_of(file, name):
    """The named tensor, or the empty fallback that fails every assertion for it."""
    return unwrap_or(SafeTensors.get_tensor(file, name), empty_tensor())


def slice_of(file, name, start, stop):
    """The named slice, or the empty fallback that fails every assertion for it."""
    return unwrap_or(SafeTensors.get_slice(file, name, start, stop), empty_tensor())


def float_case(name, data, want):
    """One float tensor: its kind and its decoded values."""
    got = tensor_of(file_of(data), name)
    return [Check.truth(name + ".kind", got.kind == "floats", "kind " + got.kind), Check.close_all(name + ".values", got.floats, want, RTOL, ATOL)]


def int_case(name, data, want):
    """One int tensor: its kind and its decoded values."""
    got = tensor_of(file_of(data), name)
    return [Check.truth(name + ".kind", got.kind == "ints", "kind " + got.kind), Check.ints_equal(name + ".values", got.ints, want)]


def err_case(tag, data, want):
    """One malformed file: load must fail with exactly `want`."""
    return [rejection_check(tag, SafeTensors.load(data), want, "load succeeded")]


def dtype_table(names, files, values, case):
    """One dtype's checks per index of parallel name, file and value lists."""
    return concat_all(toList(map(range(len(names)), lambda i: case(unwrap_or(listGet(names, i), ""), unwrap_or(listGet(files, i), []), unwrap_or(listGet(values, i), [])))))


def utf8_bytes(text):
    """`text`'s UTF-8 bytes, in order."""
    return toList(map(range(byteLength(text)), lambda i: unwrap_or(byteAt(text, i), 0)))


def header_file(header, data):
    """A safetensors buffer: the 8-byte header length, the header text, then `data`."""
    return listConcat(listConcat(Bytes.int_bytes(byteLength(header), 8), utf8_bytes(header)), data)


def header_edge_cases():
    """Hand-built headers: an unknown dtype, offset orderings load must sort or
    reject, and a repeated name, whose equal-key comparison sorts it twice."""
    ordered = file_of(header_file('{"b":{"dtype":"F32","shape":[1],"data_offsets":[4,8]},"a":{"dtype":"F32","shape":[1],"data_offsets":[0,4]}}', [0, 0, 0, 0, 0, 0, 0, 0]))
    repeated = file_of(header_file('{"a":{"dtype":"U8","shape":[1],"data_offsets":[0,1]},"a":{"dtype":"U8","shape":[1],"data_offsets":[1,2]}}', [7, 9]))
    same_start = '{"p":{"dtype":"F32","shape":[1],"data_offsets":[0,4]},"q":{"dtype":"F32","shape":[1],"data_offsets":[0,8]}}'
    same_start_reversed = '{"q":{"dtype":"F32","shape":[1],"data_offsets":[0,8]},"p":{"dtype":"F32","shape":[1],"data_offsets":[0,4]}}'
    zeros = [0, 0, 0, 0, 0, 0, 0, 0]
    return [
        rejection_check("hdr.dtype_unknown", SafeTensors.load(header_file('{"t":{"dtype":"Q9","shape":[1],"data_offsets":[0,4]}}', zeros)), "invalid JSON in header: unknown dtype Q9", "load succeeded"),
        rejection_check("hdr.same_start", SafeTensors.load(header_file(same_start, zeros)), "invalid offset for tensor `q`", "load succeeded"),
        rejection_check("hdr.same_start_reversed", SafeTensors.load(header_file(same_start_reversed, zeros)), "invalid offset for tensor `q`", "load succeeded"),
        str_list_equal("hdr.offsets_out_of_order", SafeTensors.offset_keys(ordered), ["a", "b"]),
        str_list_equal("hdr.repeated_name_keys", SafeTensors.keys(repeated), ["a", "a"]),
    ]


def dtype_cases():
    """Every dtype's file decodes to the oracle's values."""
    return concat_all([
        dtype_table(["f32", "f64", "f16", "bf16"], [Expected.F32_BYTES, Expected.F64_BYTES, Expected.F16_BYTES, Expected.BF16_BYTES], [Expected.F32_VALUES, Expected.F64_VALUES, Expected.F16_VALUES, Expected.BF16_VALUES], float_case),
        dtype_table(["bool", "u8", "i8", "i16", "u16", "i32", "u32", "i64", "u64"], [Expected.BOOL_BYTES, Expected.U8_BYTES, Expected.I8_BYTES, Expected.I16_BYTES, Expected.U16_BYTES, Expected.I32_BYTES, Expected.U32_BYTES, Expected.I64_BYTES, Expected.U64_BYTES], [Expected.BOOL_VALUES, Expected.U8_VALUES, Expected.I8_VALUES, Expected.I16_VALUES, Expected.U16_VALUES, Expected.I32_VALUES, Expected.U32_VALUES, Expected.I64_VALUES, Expected.U64_VALUES], int_case),
    ])


def multi_cases():
    """A multi-tensor file: keys in offset order and each tensor's values."""
    file = file_of(Expected.MULTI_BYTES)
    a = tensor_of(file, "a")
    b = tensor_of(file, "b")
    c = tensor_of(file, "c")
    return [
        str_list_equal("multi.keys", SafeTensors.keys(file), Expected.MULTI_KEYS),
        str_list_equal("multi.offset_keys", SafeTensors.offset_keys(file), Expected.MULTI_OFFSET_KEYS),
        Check.close_all("multi.a", a.floats, Expected.MULTI_A, RTOL, ATOL),
        Check.close_all("multi.b", b.floats, Expected.MULTI_B, RTOL, ATOL),
        Check.ints_equal("multi.c", c.ints, Expected.MULTI_C),
    ]


def str_list_equal(tag, got, want):
    """Exact equality of two string lists, naming the first difference."""
    i = fold(range(len(want)), len(want), lambda found, k: found if found < len(want) or unwrap_or(listGet(got, k), "") == unwrap_or(listGet(want, k), "") else k)
    if len(got) != len(want):
        return Check.failed(tag, "length " + toString(len(got)) + ", want " + toString(len(want)))
    else:
        return Check.truth(tag, i == len(want), "index " + toString(i))


def str_cmp(a, b):
    """String comparison for sorting."""
    if a < b:
        return -1
    elif a > b:
        return 1
    else:
        return 0


def meta_cases():
    """The `__metadata__` string map round-trips through load."""
    file = file_of(Expected.META_BYTES)
    meta = SafeTensors.metadata(file)
    keys = listSortBy(mapKeys(meta), str_cmp)
    return [
        str_list_equal("meta.keys", keys, Expected.META_KEYS),
        Check.equal("meta.author", unwrap_or(mapGet(meta, "author"), ""), "osprey"),
        Check.equal("meta.format", unwrap_or(mapGet(meta, "format"), ""), "pt"),
    ]


def slice_cases():
    """get_slice rows of the first dimension."""
    file = file_of(Expected.SLICE_BYTES)
    rows = slice_of(file, "m", 1, 3)
    whole = tensor_of(file, "m")
    return [
        Check.close_all("slice.rows", rows.floats, Expected.SLICE_ROWS, RTOL, ATOL),
        Check.close_all("slice.whole", whole.floats, Expected.SLICE_WHOLE, RTOL, ATOL),
    ]


def special_cases():
    """Non-finite and subnormal float32 values decode exactly."""
    file = file_of(Expected.SPECIAL_BYTES)
    got = tensor_of(file, "special")
    want = Expected.SPECIAL_VALUES
    return toList(map(range(len(want)), lambda i: Check.close("special." + toString(i), unwrap_or(listGet(got.floats, i), 0.0), Check.float_of(unwrap_or(listGet(want, i), "")), RTOL, ATOL)))


def err_table(tags, files, wants):
    """One failed-load check per index of parallel tag, file and message lists."""
    return concat_all(toList(map(range(len(tags)), lambda i: err_case(unwrap_or(listGet(tags, i), ""), unwrap_or(listGet(files, i), []), unwrap_or(listGet(wants, i), "")))))


def error_cases():
    """Every malformed header fails with the Rust crate's message."""
    return concat_all([
        err_table(["err.empty", "err.short", "err.len", "err.large", "err.utf8"], [Expected.ERR_EMPTY_BYTES, Expected.ERR_SHORT_BYTES, Expected.ERR_LEN_BYTES, Expected.ERR_LARGE_BYTES, Expected.ERR_UTF8_BYTES], [Expected.ERR_EMPTY, Expected.ERR_SHORT, Expected.ERR_LEN, Expected.ERR_LARGE, Expected.ERR_UTF8]),
        header_prefix_case("err.json", Expected.ERR_JSON_BYTES, "invalid JSON in header:"),
        err_table(["err.info", "err.offset", "err.overflow", "err.meta"], [Expected.ERR_INFO_BYTES, Expected.ERR_OFFSET_BYTES, Expected.ERR_OVERFLOW_BYTES, Expected.ERR_META_BYTES], [Expected.ERR_INFO, Expected.ERR_OFFSET, Expected.ERR_OVERFLOW, Expected.ERR_META]),
    ])


def notfound_cases():
    """A missing tensor name reports the wrapper's not-found message."""
    return [rejection_check("notfound", SafeTensors.get_tensor(file_of(Expected.ERR_NOTFOUND_BYTES), "missing"), Expected.ERR_NOTFOUND, "get_tensor succeeded")]


def dtype_helper_cases():
    order = Dtype.dtype_order()
    return [
        Check.ints_equal("dtype.sizes", mapList(order, Dtype.dtype_size), [1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 8, 8, 8]),
        Check.ints_equal("dtype.ranks", mapList(order, Dtype.dtype_rank), toList(range(13))),
        Check.equal("dtype.unknown_size", Dtype.dtype_size("unknown"), 0),
        Check.equal("dtype.unknown_rank", Dtype.dtype_rank("unknown"), -1),
        Check.equal("dtype.float", Dtype.is_float_dtype("BF16"), True),
        Check.equal("dtype.nonfloat", Dtype.is_float_dtype("I64"), False),
        Check.equal("dtype.int", Dtype.is_int_dtype("BOOL"), True),
        Check.equal("dtype.nonint", Dtype.is_int_dtype("F64"), False),
    ]


def byte_read_cases():
    return [
        Check.equal("bytes.pow2i", Bytes.pow2i(16), 65536),
        Check.equal("bytes.pow2", Bytes.pow2(-2), 0.25),
        Check.equal("bytes.le_bytes", Bytes.le_bytes([255, 1, 2], 1, 2), 513),
        Check.equal("bytes.checked_mul", Bytes.checked_mul(6, 7), 42),
        Check.equal("bytes.checked_mul_zero", Bytes.checked_mul(0, 99), 0),
        Check.equal("bytes.checked_mul_overflow", Bytes.checked_mul(9223372036854775807, 2), -1),
        Check.equal("bytes.le16", Bytes.le16([255, 1, 2], 1), 513),
        Check.equal("bytes.le32", Bytes.le32([1, 2, 3, 4], 0), 67305985),
        Check.equal("bytes.le64", Bytes.le64([1, 2, 0, 0, 0, 0, 0, 0], 0), 513),
        Check.ints_equal("bytes.read_bytes", unwrap_or(Bytes.read_bytes([1, 2, 3], 1, 2), []), [2, 3]),
        Check.equal("bytes.read_u64", unwrap_or(Bytes.read_u64([1, 2, 0, 0, 0, 0, 0, 0], 0), -1), 513),
        Check.equal("bytes.bytes_to_string", unwrap_or(Bytes.bytes_to_string([195, 169, 240, 159, 152, 128]), ""), "é😀"),
        Check.equal("bytes.bytes_to_string.long", byteLength(unwrap_or(Bytes.bytes_to_string(long_text_bytes()), "")), 16384),
        Check.truth("bytes.bytes_to_string.long_invalid", is_error(Bytes.bytes_to_string(listAppend(long_text_bytes(), 255))), "a bad byte past 16384 valid ones must be an Error"),
        Check.ints_equal("bytes.int_bytes", Bytes.int_bytes(-2, 2), [254, 255]),
    ]


def float_byte_cases():
    """IEEE words for 1.5 also match CPython struct.pack's little-endian bytes."""
    return [
        Check.truth("bytes.f16_from_bits", Bytes.f16_from_bits(15872) == toFloat16(1.5), "f16_from_bits(15872) != toFloat16(1.5)"),
        Check.equal("bytes.bf16_from_bits", Bytes.bf16_from_bits(16320), 1.5),
        Check.truth("bytes.f32_from_bits", Bytes.f32_from_bits(1069547520) == toFloat32(1.5), "f32_from_bits(1069547520) != toFloat32(1.5)"),
        Check.equal("bytes.f64_from_parts", Bytes.f64_from_parts(0, 1023, 2251799813685248), 1.5),
        Check.equal("bytes.decode_f16", Bytes.decode_f16(F16_WORD, 0), 1.5),
        Check.equal("bytes.decode_bf16", Bytes.decode_bf16(BF16_WORD, 0), 1.5),
        Check.equal("bytes.decode_f32", Bytes.decode_f32(F32_WORD, 0), 1.5),
        Check.equal("bytes.decode_f64", Bytes.decode_f64(F64_WORD, 0), 1.5),
        Check.ints_equal("bytes.f16_bytes", Bytes.f16_bytes(1.5), F16_WORD),
        Check.ints_equal("bytes.bf16_bytes", Bytes.bf16_bytes(1.5), BF16_WORD),
        Check.ints_equal("bytes.f32_bytes", Bytes.f32_bytes(1.5), F32_WORD),
        Check.ints_equal("bytes.f64_bytes", Bytes.f64_bytes(1.5), F64_WORD),
    ]


def parsed_checks(tag, text, checks_of):
    """Parse `text`: one failed check on Error, else `checks_of(value)`."""
    match JsonParser.parse(text):
        case Error(message=message):
            return [Check.failed(tag, message)]
        case Success(value=value):
            return checks_of(value)


def json_member_checks(value):
    """The single member is a `value` int of 7 inside an object."""
    number = JsonParser.JsonValue(kind="int", b=False, i=7, f=0.0, s="", items=[], members=[])
    member = JsonParser.JsonMember(name="value", value=number)
    got = unwrap_or(listGet(value.members, 0), member)
    return [Check.equal("json.kind", value.kind, "obj"), Check.equal("json.members", len(value.members), 1), Check.equal("json.member", got.name, member.name), Check.equal("json.number", got.value.i, number.i)]


def json_helper_cases():
    """A one-member object parses with its kind and value intact."""
    return parsed_checks("json.parse", "{\"value\":7}", json_member_checks)


def tensor_info_case():
    info = SafeTensors.TensorInfo(name="x", dtype="F32", shape=[2], start=0, end=8)
    return Check.equal("tensor_info.byte_extent", info.end - info.start, Dtype.dtype_size(info.dtype) * unwrap_or(listGet(info.shape, 0), 0))


def json_array_checks(value):
    """The parsed array's five items: two bools, null, an exponent float, a string."""
    fallback = empty_json_value()
    items = value.items
    return [Check.equal("json.array_count", len(items), 5), Check.equal("json.true", unwrap_or(listGet(items, 0), fallback).b, True), Check.equal("json.false", unwrap_or(listGet(items, 1), fallback).b, False), Check.equal("json.null", unwrap_or(listGet(items, 2), fallback).kind, "null"), Check.equal("json.exponent", unwrap_or(listGet(items, 3), fallback).f, 125.0), Check.equal("json.escaped_unicode", unwrap_or(listGet(items, 4), fallback).s, "中\n\t\b\f\r/\\\"")]


def json_value_cases():
    """RFC 8259 literals, exponent numbers and escaped Unicode are header values."""
    return parsed_checks("json.values", '[true,false,null,1.25e2,"\\u4e2d\\n\\t\\b\\f\\r\\/\\\\\\\""]', json_array_checks)


def binary_edge_cases():
    """IEEE little-endian signed zero and smallest subnormals; a UTF-8 BMP word."""
    return [
        Check.ints_equal("bytes.negative_zero", Bytes.f64_bytes(-0.0), [0, 0, 0, 0, 0, 0, 0, 128]),
        Check.ints_equal("bytes.f64_subnormal", Bytes.f64_bytes(Check.float_of("5e-324")), [1, 0, 0, 0, 0, 0, 0, 0]),
        Check.ints_equal("bytes.f16_subnormal", Bytes.f16_bytes(Bytes.pow2(-24)), [1, 0]),
        Check.equal("bytes.utf8_bmp", unwrap_or(Bytes.bytes_to_string([228, 184, 173]), ""), "中"),
    ]


def helper_cases():
    return concat_all([dtype_helper_cases(), byte_read_cases(), float_byte_cases(), json_helper_cases(), json_value_cases(), binary_edge_cases(), byte_guard_cases(), utf8_cases(), f64_encode_cases(), f16_encode_cases(), bf16_encode_cases(), f32_encode_cases(), int_encode_cases(), json_hex_case(), json_empty_case(), json_number_case(), json_reject_cases(), [tensor_info_case()]])


def header_prefix_case(tag, data, prefix):
    """A malformed header must fail inside the crate's JSON error family."""
    match SafeTensors.load(data):
        case Success(value=value):
            return [Check.failed(tag, "load succeeded")]
        case Error(message=message):
            ok = len(message) >= len(prefix) and unwrap_or(substring(message, 0, len(prefix)), "") == prefix
            return [Check.truth(tag, ok, message)]


def prefix_table(tags, files, prefix):
    """One header-prefix rejection per index of parallel tag and file lists."""
    return concat_all(toList(map(range(len(tags)), lambda i: header_prefix_case(unwrap_or(listGet(tags, i), ""), unwrap_or(listGet(files, i), []), prefix))))


def header_json_cases():
    """Header JSON violations: object, tensor fields, offsets and metadata."""
    return prefix_table(["hdr.array", "hdr.tensor", "hdr.no_dtype", "hdr.dtype_type", "hdr.no_shape", "hdr.shape_type", "hdr.shape_elem", "hdr.offsets_pair", "hdr.offsets_type", "hdr.meta_type", "hdr.meta_value"], [Expected.HDR_ARRAY_BYTES, Expected.HDR_TENSOR_BYTES, Expected.HDR_NO_DTYPE_BYTES, Expected.HDR_DTYPE_TYPE_BYTES, Expected.HDR_NO_SHAPE_BYTES, Expected.HDR_SHAPE_TYPE_BYTES, Expected.HDR_SHAPE_ELEM_BYTES, Expected.HDR_OFFSETS_PAIR_BYTES, Expected.HDR_OFFSETS_TYPE_BYTES, Expected.HDR_META_TYPE_BYTES, Expected.HDR_META_VALUE_BYTES], Expected.ERR_JSON_PREFIX)


def header_size_cases():
    """Oversized header lengths and overflowing element counts keep crate text."""
    return err_table(["hdr.len_hi", "hdr.huge", "hdr.overflow"], [Expected.HDR_LEN_HI_BYTES, Expected.HDR_HUGE_BYTES, Expected.HDR_OVERFLOW_BYTES], [Expected.HDR_LEN_HI, Expected.HDR_HUGE, Expected.HDR_OVERFLOW])


def slice_edge_cases():
    """One row, an empty range, a 1-D vector, a 0-d scalar and a whole scalar."""
    m = file_of(Expected.SLICE_BYTES)
    v = file_of(Expected.VECTOR_BYTES)
    s = file_of(Expected.SCALAR_BYTES)
    row = slice_of(m, "m", 2, 3)
    none = slice_of(m, "m", 1, 1)
    part = slice_of(v, "v", 1, 3)
    whole = tensor_of(s, "x")
    return [
        Check.close_all("slice.one_row", row.floats, Expected.SLICE_ONE_ROW, RTOL, ATOL),
        Check.close_all("slice.empty_rows", none.floats, Expected.SLICE_EMPTY_ROWS, RTOL, ATOL),
        Check.close_all("slice.vector", part.floats, Expected.VECTOR_SLICE, RTOL, ATOL),
        rejection_check("slice.scalar", SafeTensors.get_slice(s, "x", 0, 1), Expected.SCALAR_SLICE_ERROR, "operation succeeded instead of rejecting invalid input"),
        Check.close_all("slice.scalar_whole", whole.floats, Expected.SCALAR_VALUE, RTOL, ATOL),
    ]


def rejection_check(tag, result, want, detail):
    """A read that must fail with exactly `want`; `detail` says what a wrong success did."""
    match result:
        case Success(value=_value):
            return Check.failed(tag, detail)
        case Error(message=message):
            return Check.equal(tag, message, want)


def rejection(tag, result, describe):
    """A read that must fail with a diagnostic, `describe(value)` naming a wrong success."""
    match result:
        case Success(value=value):
            return Check.failed(tag, describe(value))
        case Error(message=message):
            return Check.truth(tag, len(message) > 0, message)


def zero_shape_cases():
    """A file whose tensors are all zero-length is valid and decodes to nothing."""
    file = file_of(Expected.ZERO_BYTES)
    a = tensor_of(file, "a")
    b = tensor_of(file, "b")
    return [
        str_list_equal("zero.keys", SafeTensors.keys(file), Expected.ZERO_KEYS),
        Check.equal("zero.floats", len(a.floats), 0),
        Check.equal("zero.ints", len(b.ints), 0),
    ]


def slice_notfound_cases():
    """get_slice on an absent tensor reports the crate's not-found message."""
    return [rejection_check("slice.notfound", SafeTensors.get_slice(file_of(Expected.ERR_NOTFOUND_BYTES), "missing", 0, 1), Expected.SLICE_NOTFOUND, "get_slice succeeded")]


def byte_error(tag, result):
    """A byte read that must fail instead of inventing bytes."""
    return rejection(tag, result, lambda _value: "read succeeded")


def byte_guard_cases():
    """Byte readers reject negative, past-the-end and oversized ranges."""
    return [
        byte_error("bytes.read_negative", Bytes.read_bytes([1, 2, 3], -1, 2)),
        byte_error("bytes.read_past_end", Bytes.read_bytes([1, 2, 3], 2, 2)),
        byte_error("bytes.u64_short", Bytes.read_u64([1, 2, 3], 0)),
        byte_error("bytes.u64_huge", Bytes.read_u64(Expected.U64_HUGE_BYTES, 0)),
    ]


def utf8_error(tag, bs):
    """A UTF-8 decode that must fail rather than substitute replacement text."""
    return rejection(tag, Bytes.bytes_to_string(bs), lambda value: "decoded to " + value)


def utf8_cases():
    """Sequences CPython's UTF-8 decoder rejects are errors, not text."""
    return toList(map(range(len(Expected.UTF8_INVALID)), lambda i: utf8_error("bytes.utf8_invalid_" + toString(i), unwrap_or(listGet(Expected.UTF8_INVALID, i), []))))


def encode_table(encode, tags, values, wants):
    """One byte-encoding check per index of parallel tag, value and expected lists."""
    return toList(map(range(len(tags)), lambda i: Check.ints_equal(unwrap_or(listGet(tags, i), ""), encode(unwrap_or(listGet(values, i), 0.0)), unwrap_or(listGet(wants, i), []))))


def f64_encode_cases():
    """struct.pack's bytes for NaN and both infinities."""
    return encode_table(Bytes.f64_bytes, ["bytes.f64_nan", "bytes.f64_inf", "bytes.f64_neg_inf"], [Check.float_of("nan"), Check.float_of("inf"), Check.float_of("-inf")], [Expected.F64_NAN_BYTES, Expected.F64_INF_BYTES, Expected.F64_NEG_INF_BYTES])


def f16_encode_cases():
    """numpy binary16 bytes: signed zero, NaN, infinities, overflow, rounding."""
    return encode_table(Bytes.f16_bytes, ["bytes.f16_zero", "bytes.f16_neg_zero", "bytes.f16_nan", "bytes.f16_inf", "bytes.f16_neg_inf", "bytes.f16_overflow", "bytes.f16_tiny", "bytes.f16_boundary", "bytes.f16_round", "bytes.f16_min_subnormal"], [0.0, -0.0, Check.float_of("nan"), Check.float_of("inf"), Check.float_of("-inf"), 65520.0, Bytes.pow2(-40), Check.float_of("6.103e-05"), 2047.5, Bytes.pow2(-24)], [Expected.F16_ZERO_BYTES, Expected.F16_NEG_ZERO_BYTES, Expected.F16_NAN_BYTES, Expected.F16_INF_BYTES, Expected.F16_NEG_INF_BYTES, Expected.F16_OVERFLOW_BYTES, Expected.F16_TINY_BYTES, Expected.F16_BOUNDARY_BYTES, Expected.F16_ROUND_BYTES, Expected.F16_MIN_SUBNORMAL_BYTES])


def bf16_encode_cases():
    """torch bfloat16 bytes: specials, overflow, carry, ties and the subnormal range."""
    return listConcat(encode_table(Bytes.bf16_bytes, ["bytes.bf16_zero", "bytes.bf16_neg_zero", "bytes.bf16_nan", "bytes.bf16_inf", "bytes.bf16_neg_inf", "bytes.bf16_overflow", "bytes.bf16_half", "bytes.bf16_negative", "bytes.bf16_carry", "bytes.bf16_tie_up", "bytes.bf16_tie_down", "bytes.bf16_min_subnormal", "bytes.bf16_below_normal", "bytes.bf16_min_normal", "bytes.bf16_double_subnormal", "bytes.bf16_neg_double_subnormal"], [0.0, -0.0, Check.float_of("nan"), Check.float_of("inf"), Check.float_of("-inf"), Check.float_of("1e39"), 0.5, -0.75, 1.9999, 1.01171875, 1.00390625, Bytes.pow2(-133), Bytes.pow2(-126) - Bytes.pow2(-135), Bytes.pow2(-126), Check.float_of("5e-324"), Check.float_of("-5e-324")], [Expected.BF16_ZERO_BYTES, Expected.BF16_NEG_ZERO_BYTES, Expected.BF16_NAN_BYTES, Expected.BF16_INF_BYTES, Expected.BF16_NEG_INF_BYTES, Expected.BF16_OVERFLOW_BYTES, Expected.BF16_HALF_BYTES, Expected.BF16_NEGATIVE_BYTES, Expected.BF16_CARRY_BYTES, Expected.BF16_TIE_UP_BYTES, Expected.BF16_TIE_DOWN_BYTES, Expected.BF16_MIN_SUBNORMAL_BYTES, Expected.BF16_BELOW_NORMAL_BYTES, Expected.BF16_MIN_NORMAL_BYTES, Expected.BF16_DOUBLE_SUBNORMAL_BYTES, Expected.BF16_NEG_DOUBLE_SUBNORMAL_BYTES]), [
        Check.truth("bytes.f64_from_parts_neg_inf", Bytes.f64_from_parts(1, 2047, 0) == Check.float_of("-inf"), "exponent 2047, fraction 0 is not an infinity"),
        Check.truth("bytes.f64_from_parts_nan", Bytes.f64_from_parts(0, 2047, 1) != Bytes.f64_from_parts(0, 2047, 1), "exponent 2047, fraction 1 is not a NaN"),
    ])


def f32_encode_cases():
    """numpy binary32 bytes: zero, the largest finite value and overflow."""
    return encode_table(Bytes.f32_bytes, ["bytes.f32_zero", "bytes.f32_max", "bytes.f32_overflow"], [0.0, Check.float_of("3.4028234663852886e38"), Check.float_of("1e39")], [Expected.F32_ZERO_BYTES, Expected.F32_MAX_BYTES, Expected.F32_OVERFLOW_BYTES])


def int_encode_cases():
    """int_bytes writes struct.pack's low bytes for a positive value."""
    return [Check.ints_equal("bytes.int_positive", Bytes.int_bytes(1000, 3), Expected.INT_TRUNCATE_BYTES)]


def empty_json_value():
    return JsonParser.JsonValue(kind="missing", b=False, i=0, f=0.0, s="", items=[], members=[])


def empty_json_member():
    return JsonParser.JsonMember(name="", value=empty_json_value())


def json_hex_checks(value):
    """Each parsed item's string equals the oracle's expected characters."""
    want = Expected.JSON_HEX_VALUES
    return toList(map(range(len(want)), lambda i: Check.equal("json.hex_" + toString(i), unwrap_or(listGet(value.items, i), empty_json_value()).s, unwrap_or(listGet(want, i), ""))))


def json_hex_case():
    """Uppercase unicode escapes decode to the three characters CPython's json gives."""
    return parsed_checks("json.hex", Expected.JSON_HEX_TEXT, json_hex_checks)


def json_empty_checks(value):
    """The two members' kinds and sizes match the oracle's lists."""
    a = unwrap_or(listGet(value.members, 0), empty_json_member())
    b = unwrap_or(listGet(value.members, 1), empty_json_member())
    return [
        Check.equal("json.empty_a", a.value.kind, unwrap_or(listGet(Expected.JSON_EMPTY_KINDS, 0), "")),
        Check.equal("json.empty_a_size", len(a.value.items), unwrap_or(listGet(Expected.JSON_EMPTY_SIZES, 0), -1)),
        Check.equal("json.empty_b", b.value.kind, unwrap_or(listGet(Expected.JSON_EMPTY_KINDS, 1), "")),
        Check.equal("json.empty_b_size", len(b.value.members), unwrap_or(listGet(Expected.JSON_EMPTY_SIZES, 1), -1)),
    ]


def json_empty_case():
    """An empty array and an empty object hold nothing, as CPython's json agrees."""
    return parsed_checks("json.empty", Expected.JSON_EMPTY_TEXT, json_empty_checks)


def json_number_checks(value):
    """The parsed int and float items match the oracle's kinds and values."""
    a = unwrap_or(listGet(value.items, 0), empty_json_value())
    b = unwrap_or(listGet(value.items, 1), empty_json_value())
    return [
        Check.equal("json.number_int_kind", a.kind, unwrap_or(listGet(Expected.JSON_NUMBER_KINDS, 0), "")),
        Check.equal("json.number_int", a.i, Expected.JSON_NUMBER_INT),
        Check.equal("json.number_float_kind", b.kind, unwrap_or(listGet(Expected.JSON_NUMBER_KINDS, 1), "")),
        Check.close("json.number_float", b.f, Expected.JSON_NUMBER_FLOAT, RTOL, ATOL),
    ]


def json_number_case():
    """A negative integer and an exponent float parse to CPython json's values."""
    return parsed_checks("json.numbers", Expected.JSON_NUMBER_TEXT, json_number_checks)


def json_reject(tag, text):
    """A text CPython's json module rejects must not parse here either."""
    return rejection(tag, JsonParser.parse(text), lambda value: "parsed as " + value.kind)


def json_reject_cases():
    """Each malformed JSON text CPython rejects is an error here too."""
    return listConcat(toList(map(range(len(Expected.JSON_INVALID)), lambda i: json_reject("json.reject_" + toString(i), unwrap_or(listGet(Expected.JSON_INVALID, i), "")))), [
        rejection_check('json.unterminated_escape', JsonParser.parse('"\\'), "invalid JSON: unterminated escape", "operation succeeded instead of rejecting invalid input"),
        rejection_check('json.unterminated_member_escape', JsonParser.parse('{"\\'), "invalid JSON: unterminated escape", "operation succeeded instead of rejecting invalid input"),
    ])


def all_cases():
    """Every case of the suite."""
    return concat_all([dtype_cases(), multi_cases(), meta_cases(), slice_cases(), special_cases(), error_cases(), notfound_cases(), header_json_cases(), header_size_cases(), header_edge_cases(), slice_edge_cases(), zero_shape_cases(), slice_notfound_cases(), helper_cases()])


def long_text_bytes():
    """16,384 bytes of two-byte text: longer than the real chat template, and far
    deeper than a per-byte recursion's stack allows."""
    return fold(range(13), [195, 169], lambda acc, _i: listConcat(acc, acc))


def is_error(result):
    match result:
        case Success(value=_value):
            return False
        case Error(message=_message):
            return True


def main():
    return Check.run(all_cases)
