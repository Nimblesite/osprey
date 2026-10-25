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


def float_case(tag, name, data, want):
    """One float tensor: its kind and its decoded values."""
    file = unwrap_or(SafeTensors.load(data), empty_file())
    got = unwrap_or(SafeTensors.get_tensor(file, name), empty_tensor())
    return [Check.truth(tag + ".kind", got.kind == "floats", "kind " + got.kind), Check.close_all(tag + ".values", got.floats, want, RTOL, ATOL)]


def int_case(tag, name, data, want):
    """One int tensor: its kind and its decoded values."""
    file = unwrap_or(SafeTensors.load(data), empty_file())
    got = unwrap_or(SafeTensors.get_tensor(file, name), empty_tensor())
    return [Check.truth(tag + ".kind", got.kind == "ints", "kind " + got.kind), Check.ints_equal(tag + ".values", got.ints, want)]


def err_case(tag, data, want):
    """One malformed file: load must fail with exactly `want`."""
    match SafeTensors.load(data):
        case Success(value=value):
            return [Check.failed(tag, "load succeeded")]
        case Error(message=message):
            return [Check.equal(tag, message, want)]


def json_err_case(tag, data):
    """A malformed JSON header: load must fail with the serde-style prefix."""
    match SafeTensors.load(data):
        case Success(value=value):
            return [Check.failed(tag, "load succeeded")]
        case Error(message=message):
            ok = len(message) >= 23 and unwrap_or(substring(message, 0, 23), "") == "invalid JSON in header:"
            return [Check.truth(tag, ok, message)]


def dtype_cases():
    """Every dtype's file decodes to the oracle's values."""
    return concat_all([
        float_case("f32", "f32", Expected.F32_BYTES, Expected.F32_VALUES),
        float_case("f64", "f64", Expected.F64_BYTES, Expected.F64_VALUES),
        float_case("f16", "f16", Expected.F16_BYTES, Expected.F16_VALUES),
        float_case("bf16", "bf16", Expected.BF16_BYTES, Expected.BF16_VALUES),
        int_case("bool", "bool", Expected.BOOL_BYTES, Expected.BOOL_VALUES),
        int_case("u8", "u8", Expected.U8_BYTES, Expected.U8_VALUES),
        int_case("i8", "i8", Expected.I8_BYTES, Expected.I8_VALUES),
        int_case("i16", "i16", Expected.I16_BYTES, Expected.I16_VALUES),
        int_case("u16", "u16", Expected.U16_BYTES, Expected.U16_VALUES),
        int_case("i32", "i32", Expected.I32_BYTES, Expected.I32_VALUES),
        int_case("u32", "u32", Expected.U32_BYTES, Expected.U32_VALUES),
        int_case("i64", "i64", Expected.I64_BYTES, Expected.I64_VALUES),
        int_case("u64", "u64", Expected.U64_BYTES, Expected.U64_VALUES),
    ])


def multi_cases():
    """A multi-tensor file: keys in offset order and each tensor's values."""
    file = unwrap_or(SafeTensors.load(Expected.MULTI_BYTES), empty_file())
    a = unwrap_or(SafeTensors.get_tensor(file, "a"), empty_tensor())
    b = unwrap_or(SafeTensors.get_tensor(file, "b"), empty_tensor())
    c = unwrap_or(SafeTensors.get_tensor(file, "c"), empty_tensor())
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
    file = unwrap_or(SafeTensors.load(Expected.META_BYTES), empty_file())
    meta = SafeTensors.metadata(file)
    keys = listSortBy(mapKeys(meta), str_cmp)
    return [
        str_list_equal("meta.keys", keys, Expected.META_KEYS),
        Check.equal("meta.author", unwrap_or(mapGet(meta, "author"), ""), "osprey"),
        Check.equal("meta.format", unwrap_or(mapGet(meta, "format"), ""), "pt"),
    ]


def slice_cases():
    """get_slice rows of the first dimension."""
    file = unwrap_or(SafeTensors.load(Expected.SLICE_BYTES), empty_file())
    rows = unwrap_or(SafeTensors.get_slice(file, "m", 1, 3), empty_tensor())
    whole = unwrap_or(SafeTensors.get_tensor(file, "m"), empty_tensor())
    return [
        Check.close_all("slice.rows", rows.floats, Expected.SLICE_ROWS, RTOL, ATOL),
        Check.close_all("slice.whole", whole.floats, Expected.SLICE_WHOLE, RTOL, ATOL),
    ]


def special_cases():
    """Non-finite and subnormal float32 values decode exactly."""
    file = unwrap_or(SafeTensors.load(Expected.SPECIAL_BYTES), empty_file())
    got = unwrap_or(SafeTensors.get_tensor(file, "special"), empty_tensor())
    want = Expected.SPECIAL_VALUES
    return toList(map(range(len(want)), lambda i: Check.close("special." + toString(i), unwrap_or(listGet(got.floats, i), 0.0), Check.float_of(unwrap_or(listGet(want, i), "")), RTOL, ATOL)))


def error_cases():
    """Every malformed header fails with the Rust crate's message."""
    return concat_all([
        err_case("err.empty", Expected.ERR_EMPTY_BYTES, Expected.ERR_EMPTY),
        err_case("err.short", Expected.ERR_SHORT_BYTES, Expected.ERR_SHORT),
        err_case("err.len", Expected.ERR_LEN_BYTES, Expected.ERR_LEN),
        err_case("err.large", Expected.ERR_LARGE_BYTES, Expected.ERR_LARGE),
        err_case("err.utf8", Expected.ERR_UTF8_BYTES, Expected.ERR_UTF8),
        json_err_case("err.json", Expected.ERR_JSON_BYTES),
        err_case("err.info", Expected.ERR_INFO_BYTES, Expected.ERR_INFO),
        err_case("err.offset", Expected.ERR_OFFSET_BYTES, Expected.ERR_OFFSET),
        err_case("err.overflow", Expected.ERR_OVERFLOW_BYTES, Expected.ERR_OVERFLOW),
        err_case("err.meta", Expected.ERR_META_BYTES, Expected.ERR_META),
    ])


def notfound_cases():
    """A missing tensor name reports the wrapper's not-found message."""
    file = unwrap_or(SafeTensors.load(Expected.ERR_NOTFOUND_BYTES), empty_file())
    match SafeTensors.get_tensor(file, "missing"):
        case Success(value=value):
            return [Check.failed("notfound", "get_tensor succeeded")]
        case Error(message=message):
            return [Check.equal("notfound", message, Expected.ERR_NOTFOUND)]


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
        Check.equal("bytes.le16", Bytes.le16([255, 1, 2], 1), 513),
        Check.equal("bytes.le32", Bytes.le32([1, 2, 3, 4], 0), 67305985),
        Check.equal("bytes.le64", Bytes.le64([1, 2, 0, 0, 0, 0, 0, 0], 0), 513),
        Check.ints_equal("bytes.read_bytes", unwrap_or(Bytes.read_bytes([1, 2, 3], 1, 2), []), [2, 3]),
        Check.equal("bytes.read_u64", unwrap_or(Bytes.read_u64([1, 2, 0, 0, 0, 0, 0, 0], 0), -1), 513),
        Check.equal("bytes.bytes_to_string", unwrap_or(Bytes.bytes_to_string([195, 169, 240, 159, 152, 128]), ""), "é😀"),
        Check.ints_equal("bytes.int_bytes", Bytes.int_bytes(-2, 2), [254, 255]),
    ]


def float_byte_cases():
    """IEEE words for 1.5 also match CPython struct.pack's little-endian bytes."""
    return [
        Check.equal("bytes.f16_from_bits", Bytes.f16_from_bits(15872), 1.5),
        Check.equal("bytes.bf16_from_bits", Bytes.bf16_from_bits(16320), 1.5),
        Check.equal("bytes.f32_from_bits", Bytes.f32_from_bits(1069547520), 1.5),
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


def json_helper_cases():
    number = JsonParser.JsonValue(kind="int", b=False, i=7, f=0.0, s="", items=[], members=[])
    member = JsonParser.JsonMember(name="value", value=number)
    match JsonParser.parse("{\"value\":7}"):
        case Error(message=message):
            return [Check.failed("json.parse", message)]
        case Success(value=value):
            got = unwrap_or(listGet(value.members, 0), member)
            return [Check.equal("json.kind", value.kind, "obj"), Check.equal("json.members", len(value.members), 1), Check.equal("json.member", got.name, member.name), Check.equal("json.number", got.value.i, number.i)]


def tensor_info_case():
    info = SafeTensors.TensorInfo(name="x", dtype="F32", shape=[2], start=0, end=8)
    return Check.equal("tensor_info.byte_extent", info.end - info.start, Dtype.dtype_size(info.dtype) * unwrap_or(listGet(info.shape, 0), 0))


def json_value_cases():
    """RFC 8259 literals, exponent numbers and escaped Unicode are header values."""
    match JsonParser.parse('[true,false,null,1.25e2,"\\u4e2d\\n\\t\\b\\f\\r\\/\\\\\\\""]'):
        case Error(message=message):
            return [Check.failed("json.values", message)]
        case Success(value=value):
            fallback = JsonParser.JsonValue(kind="missing", b=False, i=0, f=0.0, s="", items=[], members=[])
            items = value.items
            return [Check.equal("json.array_count", len(items), 5), Check.equal("json.true", unwrap_or(listGet(items, 0), fallback).b, True), Check.equal("json.false", unwrap_or(listGet(items, 1), fallback).b, False), Check.equal("json.null", unwrap_or(listGet(items, 2), fallback).kind, "null"), Check.equal("json.exponent", unwrap_or(listGet(items, 3), fallback).f, 125.0), Check.equal("json.escaped_unicode", unwrap_or(listGet(items, 4), fallback).s, "中\n\t\b\f\r/\\\"")]


def binary_edge_cases():
    """IEEE little-endian signed zero and smallest subnormals; a UTF-8 BMP word."""
    return [
        Check.ints_equal("bytes.negative_zero", Bytes.f64_bytes(-0.0), [0, 0, 0, 0, 0, 0, 0, 128]),
        Check.ints_equal("bytes.f64_subnormal", Bytes.f64_bytes(Check.float_of("5e-324")), [1, 0, 0, 0, 0, 0, 0, 0]),
        Check.ints_equal("bytes.f16_subnormal", Bytes.f16_bytes(Bytes.pow2(-24)), [1, 0]),
        Check.equal("bytes.utf8_bmp", unwrap_or(Bytes.bytes_to_string([228, 184, 173]), ""), "中"),
    ]


def helper_cases():
    return concat_all([dtype_helper_cases(), byte_read_cases(), float_byte_cases(), json_helper_cases(), json_value_cases(), binary_edge_cases(), byte_guard_cases(), utf8_cases(), f64_encode_cases(), f16_encode_cases(), f32_encode_cases(), int_encode_cases(), json_hex_case(), json_empty_case(), json_number_case(), json_reject_cases(), [tensor_info_case()]])


def header_prefix_case(tag, data, prefix):
    """A malformed header must fail inside the crate's JSON error family."""
    match SafeTensors.load(data):
        case Success(value=value):
            return [Check.failed(tag, "load succeeded")]
        case Error(message=message):
            ok = len(message) >= len(prefix) and unwrap_or(substring(message, 0, len(prefix)), "") == prefix
            return [Check.truth(tag, ok, message)]


def header_json_cases():
    """Header JSON violations: object, tensor fields, offsets and metadata."""
    prefix = Expected.ERR_JSON_PREFIX
    return concat_all([
        header_prefix_case("hdr.array", Expected.HDR_ARRAY_BYTES, prefix),
        header_prefix_case("hdr.tensor", Expected.HDR_TENSOR_BYTES, prefix),
        header_prefix_case("hdr.no_dtype", Expected.HDR_NO_DTYPE_BYTES, prefix),
        header_prefix_case("hdr.dtype_type", Expected.HDR_DTYPE_TYPE_BYTES, prefix),
        header_prefix_case("hdr.no_shape", Expected.HDR_NO_SHAPE_BYTES, prefix),
        header_prefix_case("hdr.shape_type", Expected.HDR_SHAPE_TYPE_BYTES, prefix),
        header_prefix_case("hdr.shape_elem", Expected.HDR_SHAPE_ELEM_BYTES, prefix),
        header_prefix_case("hdr.offsets_pair", Expected.HDR_OFFSETS_PAIR_BYTES, prefix),
        header_prefix_case("hdr.offsets_type", Expected.HDR_OFFSETS_TYPE_BYTES, prefix),
        header_prefix_case("hdr.meta_type", Expected.HDR_META_TYPE_BYTES, prefix),
        header_prefix_case("hdr.meta_value", Expected.HDR_META_VALUE_BYTES, prefix),
    ])


def header_size_cases():
    """Oversized header lengths and overflowing element counts keep crate text."""
    return concat_all([
        err_case("hdr.len_hi", Expected.HDR_LEN_HI_BYTES, Expected.HDR_LEN_HI),
        err_case("hdr.huge", Expected.HDR_HUGE_BYTES, Expected.HDR_HUGE),
        err_case("hdr.overflow", Expected.HDR_OVERFLOW_BYTES, Expected.HDR_OVERFLOW),
    ])


def slice_edge_cases():
    """One row, an empty range, a 1-D vector, a 0-d scalar and a whole scalar."""
    m = unwrap_or(SafeTensors.load(Expected.SLICE_BYTES), empty_file())
    v = unwrap_or(SafeTensors.load(Expected.VECTOR_BYTES), empty_file())
    s = unwrap_or(SafeTensors.load(Expected.SCALAR_BYTES), empty_file())
    row = unwrap_or(SafeTensors.get_slice(m, "m", 2, 3), empty_tensor())
    none = unwrap_or(SafeTensors.get_slice(m, "m", 1, 1), empty_tensor())
    part = unwrap_or(SafeTensors.get_slice(v, "v", 1, 3), empty_tensor())
    scalar = unwrap_or(SafeTensors.get_slice(s, "x", 0, 1), empty_tensor())
    whole = unwrap_or(SafeTensors.get_tensor(s, "x"), empty_tensor())
    return [
        Check.close_all("slice.one_row", row.floats, Expected.SLICE_ONE_ROW, RTOL, ATOL),
        Check.close_all("slice.empty_rows", none.floats, Expected.SLICE_EMPTY_ROWS, RTOL, ATOL),
        Check.close_all("slice.vector", part.floats, Expected.VECTOR_SLICE, RTOL, ATOL),
        Check.close_all("slice.scalar", scalar.floats, Expected.SCALAR_VALUE, RTOL, ATOL),
        Check.close_all("slice.scalar_whole", whole.floats, Expected.SCALAR_VALUE, RTOL, ATOL),
    ]


def zero_shape_cases():
    """A file whose tensors are all zero-length is valid and decodes to nothing."""
    file = unwrap_or(SafeTensors.load(Expected.ZERO_BYTES), empty_file())
    a = unwrap_or(SafeTensors.get_tensor(file, "a"), empty_tensor())
    b = unwrap_or(SafeTensors.get_tensor(file, "b"), empty_tensor())
    return [
        str_list_equal("zero.keys", SafeTensors.keys(file), Expected.ZERO_KEYS),
        Check.equal("zero.floats", len(a.floats), 0),
        Check.equal("zero.ints", len(b.ints), 0),
    ]


def slice_notfound_cases():
    """get_slice on an absent tensor reports the crate's not-found message."""
    file = unwrap_or(SafeTensors.load(Expected.ERR_NOTFOUND_BYTES), empty_file())
    match SafeTensors.get_slice(file, "missing", 0, 1):
        case Success(value=value):
            return [Check.failed("slice.notfound", "get_slice succeeded")]
        case Error(message=message):
            return [Check.equal("slice.notfound", message, Expected.SLICE_NOTFOUND)]


def byte_error(tag, result):
    """A byte read that must fail instead of inventing bytes."""
    match result:
        case Success(value=value):
            return Check.failed(tag, "read succeeded")
        case Error(message=message):
            return Check.truth(tag, len(message) > 0, message)


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
    match Bytes.bytes_to_string(bs):
        case Success(value=value):
            return Check.failed(tag, "decoded to " + value)
        case Error(message=message):
            return Check.truth(tag, len(message) > 0, message)


def utf8_cases():
    """Sequences CPython's UTF-8 decoder rejects are errors, not text."""
    return toList(map(range(len(Expected.UTF8_INVALID)), lambda i: utf8_error("bytes.utf8_invalid_" + toString(i), unwrap_or(listGet(Expected.UTF8_INVALID, i), []))))


def f64_encode_cases():
    """struct.pack's bytes for NaN and both infinities."""
    return [
        Check.ints_equal("bytes.f64_nan", Bytes.f64_bytes(Check.float_of("nan")), Expected.F64_NAN_BYTES),
        Check.ints_equal("bytes.f64_inf", Bytes.f64_bytes(Check.float_of("inf")), Expected.F64_INF_BYTES),
        Check.ints_equal("bytes.f64_neg_inf", Bytes.f64_bytes(Check.float_of("-inf")), Expected.F64_NEG_INF_BYTES),
    ]


def f16_encode_cases():
    """numpy binary16 bytes: signed zero, NaN, infinities, overflow, rounding."""
    return [
        Check.ints_equal("bytes.f16_zero", Bytes.f16_bytes(0.0), Expected.F16_ZERO_BYTES),
        Check.ints_equal("bytes.f16_neg_zero", Bytes.f16_bytes(-0.0), Expected.F16_NEG_ZERO_BYTES),
        Check.ints_equal("bytes.f16_nan", Bytes.f16_bytes(Check.float_of("nan")), Expected.F16_NAN_BYTES),
        Check.ints_equal("bytes.f16_inf", Bytes.f16_bytes(Check.float_of("inf")), Expected.F16_INF_BYTES),
        Check.ints_equal("bytes.f16_neg_inf", Bytes.f16_bytes(Check.float_of("-inf")), Expected.F16_NEG_INF_BYTES),
        Check.ints_equal("bytes.f16_overflow", Bytes.f16_bytes(65520.0), Expected.F16_OVERFLOW_BYTES),
        Check.ints_equal("bytes.f16_tiny", Bytes.f16_bytes(Bytes.pow2(-40)), Expected.F16_TINY_BYTES),
        Check.ints_equal("bytes.f16_boundary", Bytes.f16_bytes(Check.float_of("6.103e-05")), Expected.F16_BOUNDARY_BYTES),
        Check.ints_equal("bytes.f16_round", Bytes.f16_bytes(2047.5), Expected.F16_ROUND_BYTES),
        Check.ints_equal("bytes.f16_min_subnormal", Bytes.f16_bytes(Bytes.pow2(-24)), Expected.F16_MIN_SUBNORMAL_BYTES),
    ]


def f32_encode_cases():
    """numpy binary32 bytes: zero, the largest finite value and overflow."""
    return [
        Check.ints_equal("bytes.f32_zero", Bytes.f32_bytes(0.0), Expected.F32_ZERO_BYTES),
        Check.ints_equal("bytes.f32_max", Bytes.f32_bytes(Check.float_of("3.4028234663852886e38")), Expected.F32_MAX_BYTES),
        Check.ints_equal("bytes.f32_overflow", Bytes.f32_bytes(Check.float_of("1e39")), Expected.F32_OVERFLOW_BYTES),
    ]


def int_encode_cases():
    """int_bytes writes struct.pack's low bytes for a positive value."""
    return [Check.ints_equal("bytes.int_positive", Bytes.int_bytes(1000, 3), Expected.INT_TRUNCATE_BYTES)]


def empty_json_value():
    return JsonParser.JsonValue(kind="missing", b=False, i=0, f=0.0, s="", items=[], members=[])


def empty_json_member():
    return JsonParser.JsonMember(name="", value=empty_json_value())


def json_hex_case():
    """Uppercase unicode escapes decode to the three characters CPython's json gives."""
    match JsonParser.parse(Expected.JSON_HEX_TEXT):
        case Error(message=message):
            return [Check.failed("json.hex", message)]
        case Success(value=value):
            items = value.items
            want = Expected.JSON_HEX_VALUES
            return toList(map(range(len(want)), lambda i: Check.equal("json.hex_" + toString(i), unwrap_or(listGet(items, i), empty_json_value()).s, unwrap_or(listGet(want, i), ""))))


def json_empty_case():
    """An empty array and an empty object hold nothing, as CPython's json agrees."""
    match JsonParser.parse(Expected.JSON_EMPTY_TEXT):
        case Error(message=message):
            return [Check.failed("json.empty", message)]
        case Success(value=value):
            a = unwrap_or(listGet(value.members, 0), empty_json_member())
            b = unwrap_or(listGet(value.members, 1), empty_json_member())
            return [
                Check.equal("json.empty_a", a.value.kind, unwrap_or(listGet(Expected.JSON_EMPTY_KINDS, 0), "")),
                Check.equal("json.empty_a_size", len(a.value.items), unwrap_or(listGet(Expected.JSON_EMPTY_SIZES, 0), -1)),
                Check.equal("json.empty_b", b.value.kind, unwrap_or(listGet(Expected.JSON_EMPTY_KINDS, 1), "")),
                Check.equal("json.empty_b_size", len(b.value.members), unwrap_or(listGet(Expected.JSON_EMPTY_SIZES, 1), -1)),
            ]


def json_number_case():
    """A negative integer and an exponent float parse to CPython json's values."""
    match JsonParser.parse(Expected.JSON_NUMBER_TEXT):
        case Error(message=message):
            return [Check.failed("json.numbers", message)]
        case Success(value=value):
            a = unwrap_or(listGet(value.items, 0), empty_json_value())
            b = unwrap_or(listGet(value.items, 1), empty_json_value())
            return [
                Check.equal("json.number_int_kind", a.kind, unwrap_or(listGet(Expected.JSON_NUMBER_KINDS, 0), "")),
                Check.equal("json.number_int", a.i, Expected.JSON_NUMBER_INT),
                Check.equal("json.number_float_kind", b.kind, unwrap_or(listGet(Expected.JSON_NUMBER_KINDS, 1), "")),
                Check.close("json.number_float", b.f, Expected.JSON_NUMBER_FLOAT, RTOL, ATOL),
            ]


def json_reject(tag, text):
    """A text CPython's json module rejects must not parse here either."""
    match JsonParser.parse(text):
        case Success(value=value):
            return Check.failed(tag, "parsed as " + value.kind)
        case Error(message=message):
            return Check.truth(tag, len(message) > 0, message)


def json_reject_cases():
    """Each malformed JSON text CPython rejects is an error here too."""
    return toList(map(range(len(Expected.JSON_INVALID)), lambda i: json_reject("json.reject_" + toString(i), unwrap_or(listGet(Expected.JSON_INVALID, i), ""))))


def run_cases():
    """Build every case and report it, under the strict arithmetic policy."""
    cases = concat_all([dtype_cases(), multi_cases(), meta_cases(), slice_cases(), special_cases(), error_cases(), notfound_cases(), header_json_cases(), header_size_cases(), slice_edge_cases(), zero_shape_cases(), slice_notfound_cases(), helper_cases()])
    return Check.report(cases)


def main():
    match Check.strictly(run_cases):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
