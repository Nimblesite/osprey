# osprey: flavor=python
# osprey: namespace=port_common
# osprey: module=Checkpoint
"""A tiny float64 safetensors checkpoint for the example applications.

The original applications keep their model or index in memory. These ports
persist the trained parameters or corpus vectors through the safetensors file
format so the example exercises real serialization and reading. The writer
builds the 8-byte little-endian header length, the JSON header padded to eight
bytes, and the little-endian float64 payload; the reader is `@osprey/safetensors`
itself, so every value is decoded by the ported library rather than by app code.
"""

from safetensors import Bytes, SafeTensors

__all__ = ["encode_f64", "decode_f64"]


def tensor_entry(name, count):
    """One tensor's JSON header entry, with its single contiguous byte range."""
    return "{\"" + name + "\":{\"dtype\":\"F64\",\"shape\":[" + toString(count) + "],\"data_offsets\":[0," + toString(wrapMul(count, 8)) + "]}}"


def ascii_bytes(text):
    """Every byte of an ASCII header, as an int list."""
    return toList(map(range(byteLength(text)), lambda i: unwrap_or(byteAt(text, i), 0)))


def pad_header(text):
    """Pad the JSON header with spaces so the data buffer starts 8-byte aligned."""
    remainder = wrapSub(byteLength(text), wrapMul(intDiv(byteLength(text), 8), 8))
    pad = 0 if remainder == 0 else wrapSub(8, remainder)
    return text + unwrap_or(repeat(" ", pad), "")


def payload(values):
    """The little-endian float64 bytes of every value, in order."""
    return foldList(values, [], lambda acc, x: listConcat(acc, Bytes.f64_bytes(x)))


def encode_f64(name, values):
    """A complete safetensors file holding one float64 tensor."""
    header = pad_header(tensor_entry(name, len(values)))
    return listConcat(Bytes.int_bytes(byteLength(header), 8), listConcat(ascii_bytes(header), payload(values)))


def decode_f64(data, name):
    """The tensor named `name`, decoded by the ported safetensors reader."""
    match SafeTensors.load(data):
        case Success(value=file):
            return tensor_float_list(file, name)
        case Error(message=message):
            return Error(message=message)


def tensor_float_list(file, name):
    """The float values of one tensor, or the reader's error."""
    match SafeTensors.get_tensor(file, name):
        case Success(value=tensor):
            return Success(value=tensor.floats)
        case Error(message=message):
            return Error(message=message)
