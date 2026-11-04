"""CPython oracle for @osprey/gguf: GGUF files written by gguf 0.19.0's GGUFWriter
and read back with its GGUFReader, plus the inputs that reader rejects.

Writes packages/gguf/tests/expected.py, which the suite compares the port against
([PORT-TEST-ORACLE], docs/specs/ecosystem/0043-PythonFlavorPorts.md). Run it in the
oracle virtualenv: `python -I scripts/oracles/oracle.py gguf`.

Every accepted fixture records the reader's whole view of the file: the header
figures, each metadata field (name, offset, value type, element type and value),
each tensor info (name, type, dims, element count, byte size, data offset) and each
tensor's raw bytes. Every rejected fixture records the message GGUFReader raised,
or "" when the failure came from numpy's own indexing, whose text is not part of the
reader's contract.
"""
import struct
import tempfile
from importlib.metadata import version
from pathlib import Path

import numpy
from gguf import GGUFReader, GGUFWriter
from gguf.constants import (
    GGML_QUANT_SIZES,
    GGMLQuantizationType,
    GGUF_DEFAULT_ALIGNMENT,
    GGUF_MAGIC,
    GGUF_VERSION,
    GGUFEndian,
    GGUFValueType,
    Keys,
)
from gguf.gguf_reader import READER_SUPPORTED_VERSIONS

from expected_module import write

INT_KINDS = (
    GGUFValueType.UINT8, GGUFValueType.INT8, GGUFValueType.UINT16, GGUFValueType.INT16,
    GGUFValueType.UINT32, GGUFValueType.INT32, GGUFValueType.UINT64, GGUFValueType.INT64,
)
FLOAT_KINDS = (GGUFValueType.FLOAT32, GGUFValueType.FLOAT64)
BUCKETS = ("ints", "floats", "bools", "strings")
# Every message GGUFReader raises itself. A rejection whose text is not one of these
# comes from numpy or builtin indexing and is not part of the reader's contract.
OWNED = (
    "GGUF magic invalid",
    "Sorry, file appears to be version",
    "Duplicate ",
    "Bad type for general.alignment field",
    "Invalid alignment: must be a non-zero power of two",
    "Found duplicated tensor with name",
    "Unknown/unhandled field type",
    "Quantized tensor row size",
)
OWNED_SUFFIXES = ()  # messages a future gguf version adds stay unmatchable until reviewed
TEMP = Path(tempfile.mkdtemp(prefix="osprey-gguf-oracle-"))

BASE_TENSORS = (
    ("ta", numpy.arange(6, dtype=numpy.float32).reshape(2, 3)),
    ("tb", numpy.arange(4, dtype=numpy.float16)),
)
QUANT_TENSORS = (
    ("q4_0", numpy.zeros(64, dtype=numpy.float32), GGMLQuantizationType.Q4_0),
    ("q8_0", numpy.zeros(64, dtype=numpy.float32), GGMLQuantizationType.Q8_0),
    ("q4k", numpy.zeros(256, dtype=numpy.float32), GGMLQuantizationType.Q4_K),
    ("bf16", numpy.zeros(4, dtype=numpy.float32), GGMLQuantizationType.BF16),
    ("i8t", numpy.arange(8, dtype=numpy.int8), None),
)
TOKENS = ["<pad>", "<eos>", "<unk>", "Ġhello", "Ġworld", "Ġ", "a", "b", "ĉ", "ðŁĴĬ", "hello"]
MERGES = ["Ġ h", "Ġ w", "a b"]
CHAT_TEMPLATE = "{{- bos_token }}{% for m in messages %}<|im_start|>{{ m.content }}<|im_end|>{% endfor %}"


def written(name, build, arch="qwen3", endianess=GGUFEndian.LITTLE):
    """The bytes of the GGUF file `build(writer)` fills in."""
    path = TEMP / f"{name}.gguf"
    writer = build(GGUFWriter(path, arch=arch, use_temp_file=False, endianess=endianess))
    writer.write_header_to_file()
    writer.write_kv_data_to_file()
    writer.write_tensors_to_file()
    writer.close()
    return list(path.read_bytes())


def opened(data, name):
    """A GGUFReader over `data`, written to a temporary file of its own."""
    path = TEMP / f"{name}.gguf"
    path.write_bytes(bytes(data))
    return GGUFReader(path)


def patched(data, offset, values):
    """`data` with `values` replacing the bytes at `offset`."""
    out = list(data)
    for index, value in enumerate(values):
        out[offset + index] = value
    return out


def field_value_offset(reader, key):
    """The offset of KV entry `key`'s value type (its value follows 4 bytes later)."""
    return reader.fields[key].offset + 8 + len(key.encode("utf-8"))


def tensor_info_offsets(reader, index):
    """The offsets of tensor `index`'s dims array and its type word."""
    field = reader.tensors[index].field
    dims = field.offset + 8 + len(reader.tensors[index].name.encode("utf-8")) + 4
    return dims, dims + 8 * len(reader.tensors[index].shape)


def base_writer(writer):
    """Every metadata kind, two tensors, and an explicit default alignment."""
    writer.add_name("osprey-tiny")
    writer.add_uint32("general.alignment", 32)
    writer.add_uint8("u8", 250)
    writer.add_int8("i8", -5)
    writer.add_uint16("u16", 65535)
    writer.add_int16("i16", -30000)
    writer.add_uint32("u32", 4000000000)
    writer.add_int32("i32", -2000000000)
    writer.add_float32("f32", 1.5)
    writer.add_uint64("u64", 1 << 40)
    writer.add_int64("i64", -(1 << 40))
    writer.add_float64("f64", -2.5)
    writer.add_bool("b", True)
    writer.add_string("s", "héllo 😀")
    writer.add_array("ints", [1, 2, 3])
    writer.add_array("strs", ["a", "bb", "ccc"])
    writer.add_array("floats", [0.5, 1.25])
    writer.add_array("bools", [True, False])
    for name, tensor in BASE_TENSORS:
        writer.add_tensor(name, tensor)
    return writer


def align_writer(writer):
    """A non-default alignment, which moves the tensor-data offset."""
    writer.add_custom_alignment(64)
    writer.add_tensor("t", numpy.arange(4, dtype=numpy.float32))
    return writer


def big_endian_writer(writer):
    """A file written for the opposite byte order."""
    writer.add_string("s", "héllo")
    writer.add_uint32("u", 70000)
    writer.add_float32("f", -1.5)
    writer.add_int64("i", -(1 << 40))
    writer.add_array("a", [1, 2, 3])
    writer.add_tensor("t", numpy.arange(4, dtype=numpy.float32))
    return writer


def quant_writer(writer):
    """Every quantized type a Qwen GGUF uses, plus one integer tensor."""
    for name, tensor, raw in QUANT_TENSORS:
        writer.add_tensor(name, tensor, raw_dtype=raw)
    return writer


def tokenizer_writer(writer):
    """The tokenizer metadata keys Phase 5 reads, each as a GGUF array or scalar."""
    writer.add_string(Keys.Tokenizer.MODEL, "gpt2")
    writer.add_string(Keys.Tokenizer.PRE, "qwen2")
    writer.add_array(Keys.Tokenizer.LIST, TOKENS)
    writer.add_array(Keys.Tokenizer.TOKEN_TYPE, [3] * len(TOKENS))
    writer.add_array(Keys.Tokenizer.SCORES, [0.0] * len(TOKENS))
    writer.add_array(Keys.Tokenizer.MERGES, MERGES)
    writer.add_uint32(Keys.Tokenizer.BOS_ID, 1)
    writer.add_uint32(Keys.Tokenizer.EOS_ID, 2)
    writer.add_uint32(Keys.Tokenizer.UNK_ID, 3)
    writer.add_bool(Keys.Tokenizer.ADD_BOS, True)
    writer.add_string(Keys.Tokenizer.CHAT_TEMPLATE, CHAT_TEMPLATE)
    writer.add_tensor("tokens_embedding", numpy.zeros(4, dtype=numpy.float32))
    return writer


def empty_writer(writer):
    """A valid file with no tensors at all."""
    return writer


def scalar_writer(writer):
    """A single 0-dimensional tensor: its dims array is empty."""
    writer.add_tensor("scalar", numpy.array(1.5, dtype=numpy.float32))
    return writer


def kv_bytes(key, vtype, value):
    """One KV entry: the key string, its value type and its encoded value."""
    encoded = key.encode("utf-8")
    return struct.pack("<Q", len(encoded)) + encoded + struct.pack("<I", vtype) + value


def kv_text(key, text):
    """A string KV entry, which the writer's `add_string` cannot emit when empty."""
    encoded = text.encode("utf-8")
    return kv_bytes(key, GGUFValueType.STRING, struct.pack("<Q", len(encoded)) + encoded)


def kv_ints(key, values):
    """An INT32 array KV entry."""
    body = b"".join(struct.pack("<i", value) for value in values)
    return kv_bytes(key, GGUFValueType.ARRAY, struct.pack("<I", GGUFValueType.INT32) + struct.pack("<Q", len(values)) + body)


def kv_empty_array(key, sub_type):
    """An array KV entry with no elements: the writer's `add_array` drops these."""
    return kv_bytes(key, GGUFValueType.ARRAY, struct.pack("<I", sub_type) + struct.pack("<Q", 0))


def kv_nested(key, rows):
    """An array whose elements are themselves arrays, which gguf's parser flattens."""
    body = b"".join(struct.pack("<I", GGUFValueType.INT32) + struct.pack("<Q", len(row)) +
                    b"".join(struct.pack("<i", value) for value in row) for row in rows)
    return kv_bytes(key, GGUFValueType.ARRAY, struct.pack("<I", GGUFValueType.ARRAY) + struct.pack("<Q", len(rows)) + body)


def hand_built(entries, version=GGUF_VERSION):
    """A GGUF file of `entries` with no tensors, for the shapes a writer cannot emit."""
    header = b"GGUF" + struct.pack("<I", version) + struct.pack("<Q", 0) + struct.pack("<Q", len(entries))
    return list(header + b"".join(entries))


def field_values(field):
    """A field's value, or its elements, as GGUFReader.contents() returns them."""
    contents = field.contents()
    return contents if field.types[0] == GGUFValueType.ARRAY else [contents]


def value_kind(field):
    """The direct element type of an array field, or the type of a scalar one.

    gguf appends the first element's whole type list, so an array of arrays reports
    [ARRAY, ARRAY, INT32] and its direct element type is the second entry.
    """
    return field.types[1] if len(field.types) > 1 else field.types[0]


def bucket(kind):
    """Which payload list a value of `kind` is recorded in."""
    if kind == GGUFValueType.STRING:
        return "strings"
    if kind in FLOAT_KINDS:
        return "floats"
    if kind == GGUFValueType.BOOL:
        return "bools"
    return "ints"


def payloads(fields):
    """The field payload lists, one bucket per value kind."""
    out = {name: [] for name in BUCKETS}
    for field in fields:
        values = field_values(field)
        kind = value_kind(field)
        for name in BUCKETS:
            out[name].append(values if bucket(kind) == name else [])
    return out


def key_offsets(fields):
    """Each entry's value-type offset and value offset, or -1 for a header figure."""
    types, values = [], []
    for field in fields:
        if field.name.startswith("GGUF."):
            types.append(-1)
            values.append(-1)
        else:
            type_at = field.offset + 8 + len(field.name.encode("utf-8"))
            types.append(type_at)
            values.append(type_at + 4)
    return types, values


def record(name, data):
    """Every constant the suite compares for one accepted file."""
    reader = opened(data, name.lower())
    fields = list(reader.fields.values())
    values = payloads(fields)
    types_at, values_at = key_offsets(fields)
    tensors = reader.tensors
    out = {
        f"{name}_BYTES": data,
        f"{name}_HEADER": [
            int(reader.fields["GGUF.version"].contents()),
            int(reader.fields["GGUF.tensor_count"].contents()),
            int(reader.fields["GGUF.kv_count"].contents()),
            int(reader.alignment),
            int(reader.data_offset),
        ],
        f"{name}_BYTE_ORDER": reader.byte_order,
        f"{name}_FIELDS": [field.name for field in fields],
        f"{name}_FIELD_OFFSETS": [field.offset for field in fields],
        f"{name}_FIELD_KINDS": [field.types[0].name.lower() for field in fields],
        f"{name}_FIELD_ITEM_KINDS": [value_kind(field).name.lower() if len(field.types) > 1 else "" for field in fields],
        f"{name}_FIELD_TYPE_OFFSETS": types_at,
        f"{name}_FIELD_VALUE_OFFSETS": values_at,
        f"{name}_TENSOR_INFO_OFFSET": tensors[0].field.offset if tensors else -1,
        f"{name}_TOKENIZER_KEYS": [key for key in reader.fields if key.startswith("tokenizer.")],
        f"{name}_TENSORS": [tensor.name for tensor in tensors],
        f"{name}_TENSOR_TYPES": [tensor.tensor_type.name for tensor in tensors],
        f"{name}_TENSOR_TYPE_IDS": [int(tensor.tensor_type) for tensor in tensors],
        f"{name}_TENSOR_DIMS": [[int(dim) for dim in tensor.shape] for tensor in tensors],
        f"{name}_TENSOR_ELEMENTS": [int(tensor.n_elements) for tensor in tensors],
        f"{name}_TENSOR_SIZES": [int(tensor.n_bytes) for tensor in tensors],
        f"{name}_TENSOR_OFFSETS": [int(tensor.data_offset) for tensor in tensors],
        f"{name}_TENSOR_DATA": [list(tensor.data.tobytes()) for tensor in tensors],
        f"{name}_DISTINCT_TYPES": sorted({tensor.tensor_type.name for tensor in tensors}),
    }
    for kind in BUCKETS:
        out[f"{name}_FIELD_{kind.upper()}"] = values[kind]
    return out


def owned_message(error):
    """The reader's own message for a rejected input, or "" for a numpy failure."""
    message = error.args[0] if error.args and isinstance(error.args[0], str) else str(error)
    return message if message.startswith(OWNED) or message.endswith(OWNED_SUFFIXES) else ""


def rejected(name, data):
    """The constants for an input gguf 0.19.0 refuses, asserting that it refuses it."""
    try:
        opened(data, name.lower())
    except Exception as error:  # noqa: BLE001 - the reader's failure is the point
        return {f"{name}_BYTES": data, f"{name}_MESSAGE": owned_message(error)}
    raise AssertionError(f"{name}: gguf 0.19.0 accepted this input")


def rejected_cases(base, reader, quant, qreader):
    """Every malformed or truncated input, built from the accepted fixtures."""
    align = field_value_offset(reader, "general.alignment")
    u8 = field_value_offset(reader, "u8")
    i8_name = reader.fields["i8"].offset + 8
    tb_name = reader.tensors[1].field.offset + 8
    q_dims, q_type = tensor_info_offsets(qreader, 0)
    return [
        ("R_MAGIC", patched(base, 0, b"XXXX")),
        ("R_VERSION1", patched(base, 4, struct.pack("<I", 1))),
        ("R_VERSION4", patched(base, 4, struct.pack("<I", 4))),
        ("R_DUPE_KEY", patched(base, i8_name, b"u8")),
        ("R_DUPE_TENSOR", patched(base, tb_name, b"ta")),
        ("R_ALIGN_TYPE", patched(base, align, struct.pack("<I", GGUFValueType.INT32))),
        ("R_ALIGN_ZERO", patched(base, align + 4, struct.pack("<I", 0))),
        ("R_ALIGN_24", patched(base, align + 4, struct.pack("<I", 24))),
        ("R_FIELD_TYPE", patched(base, u8, struct.pack("<I", 13))),
        ("R_QUANT_TYPE", patched(quant, q_type, struct.pack("<I", 31))),
        ("R_QUANT_SHAPE", patched(quant, q_dims, struct.pack("<Q", 65))),
        ("R_TRUNC_KV", base[:100]),
        ("R_TRUNC_TI", base[:reader.data_offset - 8]),
        ("R_TRUNC_DATA", base[:reader.data_offset + 20]),
        ("R_SHORT", base[:6]),
        ("R_EMPTY", []),
    ]


def accepted():
    """Every accepted fixture: its expected-constant prefix and its bytes."""
    base = written("base", base_writer)
    return [
        ("BASE", base),
        ("ALIGN", written("align", align_writer)),
        ("BE", written("big_endian", big_endian_writer, arch="tinyllama", endianess=GGUFEndian.BIG)),
        ("QUANT", written("quant", quant_writer)),
        ("TOK", written("tokenizer", tokenizer_writer)),
        ("EMPTY", written("empty", empty_writer)),
        ("SCALAR", written("scalar", scalar_writer)),
        ("V2", patched(base, 4, struct.pack("<I", 2))),
        ("EDGE", hand_built([
            kv_empty_array("e_ints", GGUFValueType.UINT8),
            kv_empty_array("e_strs", GGUFValueType.STRING),
            kv_text("s0", ""),
            kv_nested("nested", [[10, 20]]),
            kv_ints("ints", [7, 8]),
        ])),
    ]


def tables():
    """The value-type and quantization tables, the tokenizer key catalogue and the
    header constants, all read from the package instead of retyped."""
    return {
        "MAGIC": GGUF_MAGIC,
        "VERSION": GGUF_VERSION,
        "SUPPORTED_VERSIONS": list(READER_SUPPORTED_VERSIONS),
        "DEFAULT_ALIGNMENT": GGUF_DEFAULT_ALIGNMENT,
        "VALUE_TYPE_IDS": [int(kind) for kind in GGUFValueType],
        "VALUE_TYPE_NAMES": [kind.name.lower() for kind in GGUFValueType],
        "QUANT_TYPE_IDS": [int(kind) for kind in GGML_QUANT_SIZES],
        "QUANT_TYPE_NAMES": [kind.name for kind in GGML_QUANT_SIZES],
        "QUANT_TYPE_BLOCKS": [sizes[0] for sizes in GGML_QUANT_SIZES.values()],
        "QUANT_TYPE_SIZES": [sizes[1] for sizes in GGML_QUANT_SIZES.values()],
        "TOKENIZER_KEYS": [value for attribute, value in vars(Keys.Tokenizer).items() if not attribute.startswith("_")],
        "UNKNOWN_VALUE_TYPE": 13,
        "UNKNOWN_QUANT_TYPE": 31,
    }


def limits():
    """The widest 64-bit words: the one value only a signed word holds, and the
    largest unsigned word, which has no int64 spelling at all."""
    lowest = struct.pack("<q", -(2**63))
    highest = struct.pack("<Q", 2**64 - 1)
    return {
        "INT64_MIN_BYTES": list(lowest),
        "INT64_MIN": int(numpy.frombuffer(lowest, dtype="<i8")[0]),
        "UINT64_MAX_BYTES": list(highest),
    }


def main():
    constants = tables()
    constants.update(limits())
    for name, data in accepted():
        constants.update(record(name, data))
    base = constants["BASE_BYTES"]
    reader = opened(base, "base-reader")
    quant = constants["QUANT_BYTES"]
    qreader = opened(quant, "quant-reader")
    for name, data in rejected_cases(base, reader, quant, qreader):
        constants.update(rejected(name, data))
    write(
        "gguf",
        "GGUF v3 files written by gguf 0.19.0's GGUFWriter, GGUFReader's view of their header, "
        "metadata, tensor infos and raw tensor bytes, and the inputs it rejects.",
        {"gguf": version("gguf"), "numpy": numpy.__version__},
        constants,
        docs={
            "VALUE_TYPE_NAMES": "The metadata value-type names in the lower-case spelling the port uses, "
                                "from gguf 0.19.0's GGUFValueType.",
            "TOKENIZER_KEYS": "Every tokenizer metadata key gguf 0.19.0 declares, in declaration order.",
            "UNKNOWN_VALUE_TYPE": "A value-type id GGUFValueType has no member for.",
            "UNKNOWN_QUANT_TYPE": "A tensor-type id GGMLQuantizationType has no member for.",
            "SUPPORTED_VERSIONS": "The GGUF versions gguf 0.19.0's reader accepts.",
            "R_EMPTY_MESSAGE": "Empty: gguf failed inside numpy for an empty file, which carries no "
                               "reader message of its own.",
        },
    )


if __name__ == "__main__":
    main()
