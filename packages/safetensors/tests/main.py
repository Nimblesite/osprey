# osprey: flavor=python
"""@osprey/safetensors suite: load, decode every dtype, keys, metadata, slices
and the Rust crate's validation errors, all against safetensors 0.8.0."""
from testing import Check
from safetensors import SafeTensors
from safetensors_expected import Expected

RTOL = 0.000000001
ATOL = 0.00000000000001


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


def run_cases():
    """Build every case and report it, under the strict arithmetic policy."""
    cases = concat_all([dtype_cases(), multi_cases(), meta_cases(), slice_cases(), special_cases(), error_cases(), notfound_cases()])
    return Check.report(cases)


def main():
    match Check.strictly(run_cases):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
