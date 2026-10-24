# osprey: flavor=python
# Osprey adaptation; files changed for Osprey. Upstream attribution and terms: ../NOTICE.md.
# osprey: namespace=safetensors
# osprey: module=SafeTensors
"""Read a safetensors file: the 8-byte little-endian header length, the JSON
header (`__metadata__` string map plus one dtype/shape/data_offsets entry per
tensor), the Rust crate's validation (contiguous ascending offsets, exact byte
sizes, full buffer coverage) and the little-endian decode of every dtype into
lists of floats or ints. Implements the Rust crate's rules and error text.
"""

from safetensors import Bytes, Dtype, JsonParser

__all__ = ["TensorInfo", "SafeFile", "DecodedTensor", "load", "keys", "offset_keys", "metadata", "get_tensor", "get_slice"]


class TensorInfo:
    """One tensor's header entry: name, dtype, shape and its byte range."""
    name: str
    dtype: str
    shape: list[int]
    start: int
    end: int


class SafeFile:
    """A parsed and validated safetensors file."""
    data: list[int]
    data_offset: int
    tensors: list[TensorInfo]
    metadata: dict[str, str]


class DecodedTensor:
    """A decoded tensor: floats for float dtypes, ints for int/bool dtypes."""
    kind: str
    floats: list[float]
    ints: list[int]


def null_value():
    """A JSON null value, the fallback when a member or item is absent."""
    return JsonParser.JsonValue(kind="null", b=False, i=0, f=0.0, s="", items=[], members=[])


def empty_member():
    """An empty JSON member, the fallback when a member is absent."""
    return JsonParser.JsonMember(name="", value=null_value())


def header_length(data):
    """The 8-byte little-endian header length, or the Rust crate's error."""
    if len(data) < 8:
        return Error(message="header too small")
    else:
        b3 = unwrap_or(listGet(data, 3), 0)
        b4 = unwrap_or(listGet(data, 4), 0)
        b5 = unwrap_or(listGet(data, 5), 0)
        b6 = unwrap_or(listGet(data, 6), 0)
        b7 = unwrap_or(listGet(data, 7), 0)
        if b4 != 0 or b5 != 0 or b6 != 0 or b7 != 0 or b3 > 5:
            return Error(message="header too large")
        else:
            n = b3 * 16777216 + Bytes.le16(data, 0) + Bytes.le_bytes(data, 2, 1) * 65536
            if n > 100000000:
                return Error(message="header too large")
            elif len(data) - 8 < n:
                return Error(message="invalid header length")
            else:
                return Success(value=n)


def header_string(data, n):
    """The header text of length `n`, or the Rust crate's UTF-8/length error."""
    match Bytes.read_bytes(data, 8, n):
        case Success(value=value):
            return utf8_of(value)
        case Error(message=message):
            return Error(message="invalid header length")


def utf8_of(bs):
    """The header bytes as UTF-8 text, or the Rust crate's UTF-8 error."""
    match Bytes.bytes_to_string(bs):
        case Success(value=value):
            return Success(value=value)
        case Error(message=message):
            return Error(message="invalid UTF-8 in header: " + message)


def load(data):
    """Parse and validate a safetensors file, or Error naming the failure."""
    match header_length(data):
        case Success(value=value):
            return header_of(data, value)
        case Error(message=message):
            return Error(message=message)


def header_of(data, n):
    """Read the header text of length `n` and parse it."""
    match header_string(data, n):
        case Success(value=value):
            return json_of(data, n, value)
        case Error(message=message):
            return Error(message=message)


def json_of(data, n, text):
    """Parse the header text and build the file."""
    match JsonParser.parse(text):
        case Success(value=value):
            return build_file(data, n, value)
        case Error(message=message):
            return Error(message="invalid JSON in header: " + message)


class Collected:
    """The header's metadata map and its tensors in JSON order."""
    metadata: dict[str, str]
    infos: list[TensorInfo]


def build_file(data, n, root):
    """Validate the parsed header object against `data` and return a SafeFile."""
    if root.kind != "obj":
        return Error(message="invalid JSON in header: expected object")
    else:
        return collected_of(data, n, root)


def collected_of(data, n, root):
    """Collect the header members and finish the file."""
    match collect_all(root):
        case Success(value=value):
            return finish_file(data, n, value)
        case Error(message=message):
            return Error(message=message)


def collect_all(root):
    """Walk the header object into a metadata map and tensor infos."""
    return collect_members(root.members, 0, {}, [])


def collect_members(members, i, meta, infos) -> Result[Collected, str]:
    """Collect members from index `i`, splitting `__metadata__` from tensors."""
    if i >= len(members):
        return Success(value=Collected(metadata=meta, infos=infos))
    else:
        m = unwrap_or(listGet(members, i), empty_member())
        if m.name == "__metadata__":
            return add_metadata(members, i, meta, infos, m)
        else:
            return add_tensor(members, i, meta, infos, m)


def add_metadata(members, i, meta, infos, m):
    """Parse member `m` as the `__metadata__` string map and continue."""
    match string_map(m.value):
        case Success(value=value):
            return collect_members(members, i + 1, value, infos)
        case Error(message=message):
            return Error(message=message)


def add_tensor(members, i, meta, infos, m):
    """Parse member `m` as a tensor entry and continue."""
    match tensor_of(m):
        case Success(value=value):
            return collect_members(members, i + 1, meta, listAppend(infos, value))
        case Error(message=message):
            return Error(message=message)


def tensor_of(m):
    """A member's value as a TensorInfo, or a JSON error."""
    if m.value.kind != "obj":
        return Error(message="invalid JSON in header: tensor not an object")
    else:
        return dtype_of(m)


def dtype_of(m):
    """Extract the `dtype` string, then the shape."""
    match field_str(m.value, "dtype"):
        case Success(value=value):
            return shape_of(m, value)
        case Error(message=message):
            return Error(message=message)


def shape_of(m, dtype):
    """Extract the `shape` int list, then the offsets."""
    match field_ints(m.value, "shape"):
        case Success(value=value):
            return offsets_of(m, dtype, value)
        case Error(message=message):
            return Error(message=message)


def offsets_of(m, dtype, shape):
    """Extract the `data_offsets` pair and build the TensorInfo."""
    match field_ints(m.value, "data_offsets"):
        case Success(value=value):
            return make_info(m.name, dtype, shape, value)
        case Error(message=message):
            return Error(message=message)


def make_info(name, dtype, shape, offsets):
    """Build a TensorInfo, rejecting an unknown dtype or a bad offsets pair."""
    if Dtype.dtype_rank(dtype) < 0:
        return Error(message="invalid JSON in header: unknown dtype " + dtype)
    elif len(offsets) != 2:
        return Error(message="invalid JSON in header: data_offsets not a pair")
    else:
        info = TensorInfo(name=name, dtype=dtype, shape=shape, start=unwrap_or(listGet(offsets, 0), 0), end=unwrap_or(listGet(offsets, 1), 0))
        return Success(value=info)


def field_str(v, name):
    """The string value of member `name` of object `v`."""
    match find_member(v.members, name):
        case Success(value=value):
            return member_str(value, name)
        case Error(message=message):
            return Error(message=message)


def member_str(m, name):
    """The string of member `m`, or Error when its value is not a string."""
    if m.value.kind == "str":
        return Success(value=m.value.s)
    else:
        return Error(message="invalid JSON in header: " + name + " not a string")


def find_member(members, name):
    """The first member named `name`, or Error when absent."""
    return find_at(members, 0, name)


def find_at(members, i, name) -> Result[JsonParser.JsonMember, str]:
    """Scan members from `i` for `name`."""
    if i >= len(members):
        return Error(message="invalid JSON in header: missing " + name)
    else:
        m = unwrap_or(listGet(members, i), empty_member())
        if m.name == name:
            return Success(value=m)
        else:
            return find_at(members, i + 1, name)


def field_ints(v, name):
    """The int-list value of member `name` of object `v`."""
    match find_member(v.members, name):
        case Success(value=value):
            return ints_of(value.value, name)
        case Error(message=message):
            return Error(message=message)


def ints_of(value, name):
    """An array of ints as a list, or a JSON error."""
    if value.kind != "arr":
        return Error(message="invalid JSON in header: " + name + " not an array")
    else:
        return collect_ints(value.items, 0, [])


def collect_ints(items, i, acc) -> Result[list[int], str]:
    """Collect array elements as ints from `i`."""
    if i >= len(items):
        return Success(value=acc)
    else:
        it = unwrap_or(listGet(items, i), null_value())
        if it.kind != "int":
            return Error(message="invalid JSON in header: array element not an int")
        else:
            return collect_ints(items, i + 1, listAppend(acc, it.i))


def string_map(value):
    """A `__metadata__` value as a string map, or a JSON error."""
    if value.kind != "obj":
        return Error(message="invalid JSON in header: __metadata__ not an object")
    else:
        return collect_metadata(value.members, 0, {})


def collect_metadata(members, i, acc) -> Result[dict[str, str], str]:
    """Collect string members into a map from `i`."""
    if i >= len(members):
        return Success(value=acc)
    else:
        m = unwrap_or(listGet(members, i), empty_member())
        if m.value.kind != "str":
            return Error(message="invalid JSON in header: metadata value not a string")
        else:
            return collect_metadata(members, i + 1, mapSet(acc, m.name, m.value.s))


def finish_file(data, n, collected):
    """Sort tensors by offset, validate them, and check the buffer is covered."""
    sorted_infos = listSortBy(collected.infos, info_cmp)
    match validate(data, sorted_infos, 0):
        case Success(value=value):
            return buffer_check(data, n, collected, sorted_infos, value)
        case Error(message=message):
            return Error(message=message)


def buffer_check(data, n, collected, sorted_infos, end):
    """Check the data exactly covers the file, then return the SafeFile."""
    total = 8 + n + end
    if total != len(data):
        return Error(message="incomplete metadata, file not fully covered")
    else:
        file = SafeFile(data=data, data_offset=8 + n, tensors=sorted_infos, metadata=collected.metadata)
        return Success(value=file)


def info_cmp(a, b):
    """Compare two TensorInfos by (start, end) for offset sorting."""
    if a.start < b.start:
        return -1
    elif a.start > b.start:
        return 1
    elif a.end < b.end:
        return -1
    elif a.end > b.end:
        return 1
    else:
        return 0


def validate(data, infos, start) -> Result[int, str]:
    """Validate contiguous ascending offsets and exact sizes, returning the end."""
    if len(infos) == 0:
        return Success(value=start)
    else:
        info = unwrap_or(listGet(infos, 0), TensorInfo(name="", dtype="U8", shape=[], start=0, end=0))
        return validate_head(data, infos, start, info)


def validate_head(data, infos, start, info):
    """Validate the head tensor's offsets and size, then the rest."""
    if info.start != start or info.end < info.start:
        return Error(message="invalid offset for tensor `" + info.name + "`")
    else:
        return validate_size(data, infos, info)


def validate_size(data, infos, info):
    """Check the head tensor's element count and byte size, then the rest."""
    nelems = shape_product(info.shape)
    if nelems < 0:
        return Error(message="overflow computing buffer size from shape and/or element type")
    else:
        size = checked_mul(nelems, Dtype.dtype_size(info.dtype))
        if size < 0:
            return Error(message="overflow computing buffer size from shape and/or element type")
        elif info.end - info.start != size:
            return Error(message="invalid shape, data type, or offset for tensor")
        else:
            return validate(data, tail(infos), info.end)


def tail(xs):
    """Every element of `xs` except the first."""
    fallback = TensorInfo(name="", dtype="U8", shape=[], start=0, end=0)
    return toList(map(range(1, len(xs)), lambda i: unwrap_or(listGet(xs, i), fallback)))


def shape_product(shape):
    """Product of `shape` dims, or -1 when it overflows int64."""
    return fold(range(len(shape)), 1, lambda acc, i: checked_mul(acc, unwrap_or(listGet(shape, i), 0)))


def checked_mul(a, b):
    """a*b for non-negative a and b, or -1 when it overflows int64."""
    if a == 0 or b == 0:
        return 0
    elif a > intDiv(9223372036854775807, b):
        return -1
    else:
        return a * b


def keys(file):
    """The tensor names, sorted alphabetically like `safe_open.keys()`."""
    return listSortBy(tensor_names(file), str_cmp)


def offset_keys(file):
    """The tensor names in offset order, like `safe_open.offset_keys()`."""
    return tensor_names(file)


def str_cmp(a, b):
    """String comparison for sorting."""
    if a < b:
        return -1
    elif a > b:
        return 1
    else:
        return 0


def tensor_names(file):
    """The names of the file's tensors, in the file's offset order."""
    return toList(map(range(len(file.tensors)), lambda i: unwrap_or(listGet(file.tensors, i), TensorInfo(name="", dtype="U8", shape=[], start=0, end=0)).name))


def metadata(file):
    """The `__metadata__` string map (empty when the file has none)."""
    return file.metadata


def get_tensor(file, name):
    """Decode the tensor named `name`, or Error when absent."""
    match find_tensor(file.tensors, name):
        case Success(value=value):
            return Success(value=decode_range(file, value, 0, element_count(value)))
        case Error(message=message):
            return Error(message=message)


def find_tensor(tensors, name):
    """The TensorInfo named `name`, or the Rust crate's not-found error."""
    return find_tensor_at(tensors, 0, name)


def find_tensor_at(tensors, i, name) -> Result[TensorInfo, str]:
    """Scan tensors from `i` for `name`."""
    if i >= len(tensors):
        return Error(message="File does not contain tensor " + name)
    else:
        t = unwrap_or(listGet(tensors, i), TensorInfo(name="", dtype="U8", shape=[], start=0, end=0))
        if t.name == name:
            return Success(value=t)
        else:
            return find_tensor_at(tensors, i + 1, name)


def element_count(info):
    """The number of elements a tensor's shape holds (0 for an empty tensor)."""
    return shape_product(info.shape)


def decode_range(file, info, start_elem, count):
    """Decode `count` elements starting at element `start_elem`."""
    stride = Dtype.dtype_size(info.dtype)
    base = file.data_offset + info.start + start_elem * stride
    if Dtype.is_float_dtype(info.dtype):
        vals = toList(map(range(count), lambda i: decode_one_float(file.data, base + i * stride, info.dtype)))
        return DecodedTensor(kind="floats", floats=vals, ints=[])
    else:
        vals = toList(map(range(count), lambda i: decode_one_int(file.data, base + i * stride, info.dtype)))
        return DecodedTensor(kind="ints", floats=[], ints=vals)


def decode_one_float(data, off, dtype):
    """One element of a float dtype at byte `off` as a binary64."""
    match dtype:
        case "F16":
            return Bytes.decode_f16(data, off)
        case "BF16":
            return Bytes.decode_bf16(data, off)
        case "F32":
            return Bytes.decode_f32(data, off)
        case _:
            return Bytes.decode_f64(data, off)


def decode_one_int(data, off, dtype):
    """One element of an int/bool dtype at byte `off`."""
    match dtype:
        case "BOOL":
            return unwrap_or(listGet(data, off), 0)
        case "U8":
            return unwrap_or(listGet(data, off), 0)
        case "I8":
            return sign_extend(unwrap_or(listGet(data, off), 0), 8)
        case "I16":
            return sign_extend(Bytes.le16(data, off), 16)
        case "U16":
            return Bytes.le16(data, off)
        case "I32":
            return sign_extend(Bytes.le32(data, off), 32)
        case "U32":
            return Bytes.le32(data, off)
        case "I64":
            return decode_i64(data, off)
        case _:
            return Bytes.le64(data, off)


def sign_extend(v, bits):
    """Two's-complement sign-extend `v` from `bits` bits."""
    if v >= Bytes.pow2i(bits - 1):
        return v - Bytes.pow2i(bits)
    else:
        return v


def decode_i64(data, off):
    """A little-endian I64 at `off`, sign-extended without overflowing."""
    b7 = unwrap_or(listGet(data, off + 7), 0)
    v = (b7 % 128) * 72057594037927936 + Bytes.le_bytes(data, off, 7)
    if b7 >= 128:
        return v - 4611686018427387904 - 4611686018427387904
    else:
        return v


def get_slice(file, name, first, last):
    """Decode rows `first`..`last` of the first dimension of `name`."""
    match find_tensor(file.tensors, name):
        case Success(value=value):
            return Success(value=slice_of(file, value, first, last))
        case Error(message=message):
            return Error(message=message)


def slice_of(file, info, first, last):
    """The decoded rows first..last of `info`."""
    if len(info.shape) == 0:
        return decode_range(file, info, 0, element_count(info))
    else:
        d0 = unwrap_or(listGet(info.shape, 0), 0)
        row = intDiv(element_count(info), d0) if d0 > 0 else 0
        return decode_range(file, info, first * row, (last - first) * row)
