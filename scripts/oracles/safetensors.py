"""CPython oracle for @osprey/safetensors: files, decodes, slices and errors.

Writes packages/safetensors/tests/expected.py, which the suite compares the
port against ([PORT-TEST-ORACLE], spec 0043). Run with the oracle virtualenv:
`python scripts/oracles/safetensors.py` (no `-I`, so `expected_module` resolves).
"""
import json
import os
import struct
import sys
import tempfile

from expected_module import write

import numpy
import torch
import safetensors
import safetensors.torch


def le64(n):
    return struct.pack("<Q", n)


def dump_bytes(b):
    """The bytes of a safetensors file as an int list."""
    return list(b)


def file_roundtrip(tensors):
    """torch.save bytes and the numpy-decoded values for a dict of tensors."""
    b = safetensors.torch.save(tensors)
    return dump_bytes(b)


def decoded(tensors):
    """Decode every tensor of `tensors` through safe_open(pt) to flat plain
    lists. torch (unlike numpy) understands bfloat16; float dtypes widen exactly."""
    b = safetensors.torch.save(tensors)
    f = tempfile.NamedTemporaryFile(suffix=".st", delete=False)
    f.write(b)
    f.close()
    out = {}
    with safetensors.safe_open(f.name, framework="pt") as s:
        for name in tensors:
            arr = s.get_tensor(name).reshape(-1)
            if arr.dtype == torch.bool:
                out[name] = [int(x) for x in arr.tolist()]
            elif arr.dtype.is_floating_point:
                out[name] = arr.double().tolist()
            else:
                out[name] = arr.tolist()
    os.unlink(f.name)
    return out


def deserialize_error(buf):
    """The Rust Display string of a deserialize failure (wrapper prefix stripped)."""
    try:
        safetensors.deserialize(buf)
        return None
    except Exception as e:
        return str(e).removeprefix("Error while deserializing: ")


def not_found_error(tensors, name):
    b = safetensors.torch.save(tensors)
    f = tempfile.NamedTemporaryFile(suffix=".st", delete=False)
    f.write(b)
    f.close()
    try:
        with safetensors.safe_open(f.name, framework="numpy") as s:
            s.get_tensor(name)
    except Exception as e:
        return str(e)
    finally:
        os.unlink(f.name)


def hdr(obj, data=b""):
    h = json.dumps(obj, separators=(",", ":")).encode()
    return le64(len(h)) + h + data


# --- one tensor per dtype ---------------------------------------------------
F32 = torch.tensor([0.0, -1.5, 2.5, 3.25], dtype=torch.float32)
F64 = torch.tensor([0.0, -1.5, 2.5, 3.25, 0.1], dtype=torch.float64)
F16 = torch.tensor([0.0, -1.5, 2.0, 0.25], dtype=torch.float16)
BF16 = torch.tensor([0.0, 1.5, -2.0], dtype=torch.bfloat16)
BOOL = torch.tensor([True, False, True, True], dtype=torch.bool)
U8 = torch.tensor([0, 1, 127, 255], dtype=torch.uint8)
I8 = torch.tensor([0, -1, 127, -128], dtype=torch.int8)
I16 = torch.tensor([0, -1, 32767, -32768], dtype=torch.int16)
U16 = torch.tensor([0, 1, 65535], dtype=torch.uint16)
I32 = torch.tensor([0, -1, 2147483647, -2147483648], dtype=torch.int32)
U32 = torch.tensor([0, 1, 4294967295], dtype=torch.uint32)
I64 = torch.tensor([0, -1, 9223372036854775807, -9223372036854775807], dtype=torch.int64)
U64 = torch.tensor([0, 1, 42, 65535], dtype=torch.uint64)

DTYPES = {
    "F32": F32, "F64": F64, "F16": F16, "BF16": BF16, "BOOL": BOOL, "U8": U8,
    "I8": I8, "I16": I16, "U16": U16, "I32": I32, "U32": U32, "I64": I64, "U64": U64,
}

# --- multi-tensor file, metadata, slice -------------------------------------
MULTI = {
    "a": torch.arange(6, dtype=torch.float32).reshape(2, 3),
    "b": torch.tensor([1.0, 2.0], dtype=torch.float64),
    "c": torch.tensor([-1, 2, -3], dtype=torch.int32),
}
META = {
    "w": torch.tensor([1.0, 2.0], dtype=torch.float32),
}
SLICE_TENSOR = {
    "m": torch.arange(12, dtype=torch.float32).reshape(3, 4),
}

# --- special float values ----------------------------------------------------
SPECIAL = torch.tensor([float("inf"), float("-inf"), float("nan"), -0.0, 1e-45], dtype=torch.float32)


def slice_values():
    b = safetensors.torch.save(SLICE_TENSOR)
    f = tempfile.NamedTemporaryFile(suffix=".st", delete=False)
    f.write(b)
    f.close()
    with safetensors.safe_open(f.name, framework="numpy") as s:
        row = s.get_slice("m")[1:3].reshape(-1).tolist()
        whole = s.get_tensor("m").reshape(-1).tolist()
    os.unlink(f.name)
    return [row, whole]


def main():
    constants = {}
    for name, t in DTYPES.items():
        constants[f"{name}_BYTES"] = file_roundtrip({name.lower(): t})
    lower = {name.lower(): t for name, t in DTYPES.items()}
    values = decoded(lower)
    for name in DTYPES:
        constants[f"{name}_VALUES"] = values[name.lower()]

    constants["MULTI_BYTES"] = file_roundtrip(MULTI)
    constants["MULTI_KEYS"] = sorted(MULTI.keys())
    b = safetensors.torch.save(MULTI)
    f = tempfile.NamedTemporaryFile(suffix=".st", delete=False)
    f.write(b)
    f.close()
    with safetensors.safe_open(f.name, framework="numpy") as s:
        constants["MULTI_OFFSET_KEYS"] = list(s.offset_keys())
    os.unlink(f.name)
    mv = decoded(MULTI)
    constants["MULTI_A"] = mv["a"]
    constants["MULTI_B"] = mv["b"]
    constants["MULTI_C"] = mv["c"]

    meta_b = safetensors.torch.save(META, metadata={"format": "pt", "author": "osprey"})
    constants["META_BYTES"] = dump_bytes(meta_b)
    f = tempfile.NamedTemporaryFile(suffix=".st", delete=False)
    f.write(meta_b)
    f.close()
    with safetensors.safe_open(f.name, framework="numpy") as s:
        constants["META_KEYS"] = sorted(s.metadata().keys())
        constants["META_VALUES"] = [s.metadata()[k] for k in sorted(s.metadata().keys())]
    os.unlink(f.name)

    sl = slice_values()
    constants["SLICE_BYTES"] = file_roundtrip(SLICE_TENSOR)
    constants["SLICE_ROWS"] = sl[0]
    constants["SLICE_WHOLE"] = sl[1]

    special_b = safetensors.torch.save({"special": SPECIAL})
    constants["SPECIAL_BYTES"] = dump_bytes(special_b)
    f = tempfile.NamedTemporaryFile(suffix=".st", delete=False)
    f.write(special_b)
    f.close()
    with safetensors.safe_open(f.name, framework="numpy") as s:
        constants["SPECIAL_VALUES"] = s.get_tensor("special").tolist()
    os.unlink(f.name)

    # --- errors --------------------------------------------------------------
    constants["ERR_EMPTY_BYTES"] = []
    constants["ERR_EMPTY"] = deserialize_error(b"")
    constants["ERR_SHORT_BYTES"] = [97, 98, 99, 100]
    constants["ERR_SHORT"] = deserialize_error(b"abcd")
    constants["ERR_LEN_BYTES"] = dump_bytes(le64(100))
    constants["ERR_LEN"] = deserialize_error(le64(100))
    constants["ERR_LARGE_BYTES"] = dump_bytes(b"\xff\xff\xff\xff\xff\xff\xff\x7f")
    constants["ERR_LARGE"] = deserialize_error(b"\xff\xff\xff\xff\xff\xff\xff\x7f")
    constants["ERR_UTF8_BYTES"] = dump_bytes(le64(1) + b"\xff")
    constants["ERR_UTF8"] = deserialize_error(le64(1) + b"\xff")
    constants["ERR_JSON_BYTES"] = dump_bytes(le64(3) + b"abc")
    constants["ERR_JSON"] = deserialize_error(le64(3) + b"abc")
    info = hdr({"t": {"dtype": "F32", "shape": [2], "data_offsets": [0, 4]}}, b"\x00" * 8)
    constants["ERR_INFO_BYTES"] = dump_bytes(info)
    constants["ERR_INFO"] = deserialize_error(info)
    off = hdr({"t": {"dtype": "F32", "shape": [2], "data_offsets": [8, 16]}}, b"\x00" * 8)
    constants["ERR_OFFSET_BYTES"] = dump_bytes(off)
    constants["ERR_OFFSET"] = deserialize_error(off)
    ovf = hdr({"t": {"dtype": "I64", "shape": [9223372036854775807, 9223372036854775807], "data_offsets": [0, 8]}}, b"\x00" * 8)
    constants["ERR_OVERFLOW_BYTES"] = dump_bytes(ovf)
    constants["ERR_OVERFLOW"] = deserialize_error(ovf)
    good = hdr({"t": {"dtype": "F32", "shape": [2], "data_offsets": [0, 8]}}, b"\x00" * 8)
    constants["ERR_META_BYTES"] = dump_bytes(good + b"\x00")
    constants["ERR_META"] = deserialize_error(good + b"\x00")
    constants["ERR_NOTFOUND_BYTES"] = file_roundtrip(MULTI)
    constants["ERR_NOTFOUND"] = not_found_error(MULTI, "missing")

    # --- save (round-trip) ---------------------------------------------------
    SAVE = {
        "i64": torch.tensor([1, -2, 3], dtype=torch.int64),
        "f32": torch.tensor([0.5, -1.5], dtype=torch.float32),
    }
    constants["SAVE_BYTES"] = file_roundtrip(SAVE)

    write(
        "safetensors",
        "safetensors 0.8.0 files, numpy decodes, slices and Rust error strings.",
        {"safetensors": safetensors.__version__, "torch": torch.__version__, "numpy": numpy.__version__},
        constants,
    )
    # The shared writer emits a per-constant attribute docstring, which the
    # current compiler binary does not lower yet (it lands with PiFrontend's
    # rebuild). Strip those so the suite compiles; the module docstring and
    # provenance line remain.
    target = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "packages", "safetensors", "tests", "expected.py")
    lines = [ln for ln in open(target).read().splitlines() if not ln.startswith('"""Oracle value')]
    open(target, "w").write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
