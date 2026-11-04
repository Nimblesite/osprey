# @osprey/gguf

An Osprey Python-flavor port of the GGUF reader from [`gguf` 0.19.0](https://github.com/ggml-org/llama.cpp/tree/master/gguf-py) (the `gguf-py` package llama.cpp ships). It reads a GGUF file from a byte buffer: the fixed header, every metadata entry in file order, every tensor's info entry, the alignment, the tensor-data offset and each tensor's stored bytes. It also decodes the block formats the chosen weights file uses from those bytes into binary32 values, matching ggml's own decoders bit for bit.

The public interface uses Osprey values and explicit byte buffers rather than a memory map and Python objects. A weights file too large for a list is parsed from its first bytes with `read_header`, and its tensor ranges are mapped with `gpuMapFile`.

The [Python-flavor ports contract](../../../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The [CPython oracle](oracle/gguf.cpython) writes the fixtures with the real `GGUFWriter` and computes every reference value with the real `GGUFReader`, into `tests/expected.py`. It compiles ggml's own `ggml-quants.c` at a pinned llama.cpp commit for the block decoders' values. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey src/osprey/packages/gguf/tests --run
target/release/osprey src/osprey/packages/gguf/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey src/osprey/packages/gguf/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.

## Layout

| Module | Contents |
| --- | --- |
| `gguf.Constants` | the magic, the accepted versions, the alignment, every metadata value type, every ggml tensor type with its block and byte sizes, and the tokenizer key catalogue |
| `gguf.Chain` | `then` and `first_or_error`, the Result plumbing the other modules share |
| `gguf.Values` | byte-order-aware word reads, the fixed header, and every metadata value kind |
| `gguf.Tensors` | the tensor info entries, the declared alignment, the tensor-data offset and each tensor's byte range |
| `gguf.Reader` | `read` and the accessors a phase uses: keys, tensor names and types, tensor bytes, tokenizer keys |
| `gguf.Quant` | `decode` from a tensor's stored bytes to binary32 values: F32, Q8_0, Q3_K, Q4_K, Q5_K, Q6_K, IQ4_NL, IQ4_XS and IQ3_S, with ggml's own IQ tables |

## API map

| Osprey | `gguf` 0.19.0 |
| --- | --- |
| `Reader.read(data)` | `GGUFReader(path)` |
| `Reader.field(file, key)` | `GGUFReader.get_field(key)` |
| `Reader.field_names(file)` | `GGUFReader.fields.keys()` |
| `Reader.tensor(file, index)` | `GGUFReader.get_tensor(idx)` |
| `Reader.tensor_names(file)` | `[t.name for t in reader.tensors]` |
| `Reader.tensor_types(file)` | `[t.tensor_type.name for t in reader.tensors]` |
| `Reader.distinct_tensor_types(file)` | `sorted({t.tensor_type.name for t in reader.tensors})` |
| `Reader.read_header(prefix, file_size)` | `GGUFReader(path)`, leaving the tensor data on disk |
| `Reader.read_path(path, file_size, count)` | `GGUFReader(path)`, reading `count` bytes and doubling until the header fits |
| `Reader.tensor_bytes(file, name)` | `reader.tensors[i].data.tobytes()` |
| `Reader.tokenizer_keys(file)` | `[k for k in reader.fields if k.startswith("tokenizer.")]` |
| `Reader.field_int` / `field_bool` / `field_float` / `field_string` | `ReaderField.contents()` for that kind, as `Result` |
| `Reader.field_ints` / `field_bools` / `field_floats` / `field_strings` | `ReaderField.contents()` for an array of that element kind |
| `GGUFFile.version`, `tensor_count`, `kv_count` | the `GGUF.version`, `GGUF.tensor_count` and `GGUF.kv_count` fields |
| `GGUFFile.alignment`, `data_offset`, `byte_order` | `GGUFReader.alignment`, `data_offset`, `byte_order` |
| `GGUFFile.fields` | `GGUFReader.fields.values()`, in the same order |
| `GGUFFile.tensors` | `GGUFReader.tensors` |
| `TensorInfo.name`, `type`, `dims`, `n_elements`, `n_bytes`, `data_offset` | `ReaderTensor.name`, `tensor_type`, `shape`, `n_elements`, `n_bytes`, `data_offset` |
| `Values.header_at(data)` | the header `GGUFReader.__init__` reads before the metadata section |
| `Values.value_at` / `string_at` / `values_at` / `kind_at` | `_get_field_parts`, `_get_str` and `_get` |
| `Values.bytes_at` | `_get(offset, np.uint8, count)` |
| `Tensors.of(data, offs, count, swapped, alignment)` | `_build_tensor_info` followed by `_build_tensors` |
| `Tensors.alignment_of(fields)` | the `general.alignment` check in `GGUFReader.__init__` |
| `Constants.VALUE_TYPES` | `GGUFValueType` |
| `Constants.QUANT_TYPES` | `GGMLQuantizationType` with `GGML_QUANT_SIZES` |
| `Constants.TOKENIZER_KEYS` | `Keys.Tokenizer` |
| `Constants.SUPPORTED_VERSIONS` | `READER_SUPPORTED_VERSIONS` |
| `Quant.decode(type_name, data)` | ggml's `dequantize_row_*` for that tensor type, the C decoder behind each `GGMLQuantizationType` |
| `Quant.SUPPORTED` | the tensor types the chosen weights file uses, out of `GGMLQuantizationType` |

## Divergences

- **A byte buffer, not a path.** `Reader.read` takes a `list[int]` of byte values, like the rest of this tree's format ports; the reference reader memory-maps a file. The caller loads the file, because the current runtime's `readFile` is NUL-terminated and truncates binary input at its first zero byte.
- **Errors are `Result`.** Every failure is `Error`, including the cases the reference reports as `None`: `get_field` on an absent key returns `None`, while `Reader.field` returns `Error`.
- **`int64` is the widest integer.** A `uint64` metadata value above 2^63 − 1 is an `Error`; the reference returns a Python integer of unbounded width. A signed 64-bit word decodes in full, two's complement.
- **Truncation and out-of-range lengths are this port's own message.** Input that ends before a field it declares, a string longer than the buffer, and a tensor whose declared bytes are missing are `Error` with `gguf: unexpected end of file at offset N` or `gguf: tensor ... data is truncated at offset N`. The reference fails inside numpy (`When changing to a larger dtype...`, `cannot reshape array...`), whose text is an implementation detail of numpy. Every message the reference reader raises itself is reproduced exactly: `GGUF magic invalid`, `Sorry, file appears to be version N which we cannot handle`, `Duplicate ... already in list at offset N`, `Bad type for general.alignment field`, `Invalid alignment: must be a non-zero power of two`, `Found duplicated tensor with name ...`, `Quantized tensor row size (...) is not a multiple of ... block size (...)`, and the type-id rejections.
- **An array of arrays parses but its typed accessors refuse it.** The entries are read and their bytes walked, so the following fields stay in step, but `field_ints` and its siblings return `Error` for such an entry: the reference flattens nested arrays in `ReaderField.contents()` (its own `FIXME`), so the row structure its accessor returns does not exist in the file.
- **Overflowing sizes are errors.** A dims product or a byte count that would overflow `int64` is an `Error`; the reference computes it with numpy, which wraps.
- **Block decoders are ggml's, for the types this model uses.** `Quant.decode` walks whole blocks and returns binary32 values for `F32`, `Q8_0`, `Q3_K`, `Q4_K`, `Q5_K`, `Q6_K`, `IQ4_NL`, `IQ4_XS` and `IQ3_S`. Every other tensor type is an `Error`, and so is a byte count that is not a whole number of blocks; no block is truncated or padded. The arithmetic runs at `float32` in ggml's order, because a wider type rounds differently and would disagree in the last bit. Its two IQ tables are ggml's own, `iq3s_grid` and `kvalues_iq4nl` from `ggml-common.h` at llama.cpp commit `3de52ac38818`, and the module says so. The oracle's values come from that commit's compiled `dequantize_row_*`, which gguf-py's own `dequantize` agrees with word for word on every fixture block.

## Tests

The suite reports 758 assertions against `GGUFWriter` fixtures read back by the real `GGUFReader`: every metadata kind in both byte orders, arrays of every element kind, an empty array and an empty string, a 0-dimensional tensor, five quantized tensor types, tokenizer metadata, GGUF v2 and v3, a custom alignment, and sixteen rejected inputs (bad magic, unsupported versions, a duplicate key, a duplicate tensor, three bad alignments, an unknown value type, an unknown tensor type, a misaligned quantized row, three truncations, a short file and an empty one).

Fifty-eight of them cover the block decoders, against values ggml's compiled `dequantize_row_*` wrote: 64 blocks per type, built from a seed, from every byte fill, and from every binary16 scale edge alone and crossed with the other scale at both extremes. Each type's whole fixture and its first block on its own are compared exactly, with no tolerance, so a rounding difference in the last bit fails. Every type is also offered one byte short and one byte over, both `Error`, and no bytes at all, which is no blocks and so no values; three names outside the list are refused.
