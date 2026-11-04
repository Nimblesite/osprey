# osprey: flavor=python
# osprey: namespace=gguf
# osprey: module=Constants
"""GGUF's fixed tables from `gguf` 0.19.0's `gguf.constants`: the file magic, the
versions its reader accepts, the default alignment, every metadata value type and
every tensor quantization type with its block and byte sizes, and the tokenizer
metadata keys Phase 0 and Phase 5 read. A tensor type's block size is the number of
elements one block holds and its size is the bytes that block occupies, so a tensor
of `n` elements occupies `n * size // block` bytes.
"""

__all__ = ["MAGIC", "VERSION", "SUPPORTED_VERSIONS", "DEFAULT_ALIGNMENT", "ValueType", "QuantType", "VALUE_TYPES", "QUANT_TYPES", "TOKENIZER_KEYS", "value_type", "quant_type"]


class ValueType:
    """One GGUF metadata value type: its wire id and lower-case name."""
    id: int
    name: str


class QuantType:
    """One ggml tensor type: its wire id, name, block size and bytes per block."""
    id: int
    name: str
    block: int
    size: int


MAGIC = 1179993927
"""The four bytes `GGUF` read as a little-endian uint32."""
VERSION = 3
"""The GGUF version this port writes nothing for and reads the third of."""
SUPPORTED_VERSIONS = [2, 3]
"""The versions the reference reader accepts, and so the ones this port accepts."""
DEFAULT_ALIGNMENT = 32
"""Where the tensor data starts when the file declares no `general.alignment`."""
VALUE_TYPES = [
    ValueType(id=0, name="uint8"),
    ValueType(id=1, name="int8"),
    ValueType(id=2, name="uint16"),
    ValueType(id=3, name="int16"),
    ValueType(id=4, name="uint32"),
    ValueType(id=5, name="int32"),
    ValueType(id=6, name="float32"),
    ValueType(id=7, name="bool"),
    ValueType(id=8, name="string"),
    ValueType(id=9, name="array"),
    ValueType(id=10, name="uint64"),
    ValueType(id=11, name="int64"),
    ValueType(id=12, name="float64"),
]
"""The thirteen metadata value types, with the names `Field.value.kind` spells."""

QUANT_TYPES = [
    QuantType(id=0, name="F32", block=1, size=4),
    QuantType(id=1, name="F16", block=1, size=2),
    QuantType(id=2, name="Q4_0", block=32, size=18),
    QuantType(id=3, name="Q4_1", block=32, size=20),
    QuantType(id=6, name="Q5_0", block=32, size=22),
    QuantType(id=7, name="Q5_1", block=32, size=24),
    QuantType(id=8, name="Q8_0", block=32, size=34),
    QuantType(id=9, name="Q8_1", block=32, size=40),
    QuantType(id=10, name="Q2_K", block=256, size=84),
    QuantType(id=11, name="Q3_K", block=256, size=110),
    QuantType(id=12, name="Q4_K", block=256, size=144),
    QuantType(id=13, name="Q5_K", block=256, size=176),
    QuantType(id=14, name="Q6_K", block=256, size=210),
    QuantType(id=15, name="Q8_K", block=256, size=292),
    QuantType(id=16, name="IQ2_XXS", block=256, size=66),
    QuantType(id=17, name="IQ2_XS", block=256, size=74),
    QuantType(id=18, name="IQ3_XXS", block=256, size=98),
    QuantType(id=19, name="IQ1_S", block=256, size=50),
    QuantType(id=20, name="IQ4_NL", block=32, size=18),
    QuantType(id=21, name="IQ3_S", block=256, size=110),
    QuantType(id=22, name="IQ2_S", block=256, size=82),
    QuantType(id=23, name="IQ4_XS", block=256, size=136),
    QuantType(id=24, name="I8", block=1, size=1),
    QuantType(id=25, name="I16", block=1, size=2),
    QuantType(id=26, name="I32", block=1, size=4),
    QuantType(id=27, name="I64", block=1, size=8),
    QuantType(id=28, name="F64", block=1, size=8),
    QuantType(id=29, name="IQ1_M", block=256, size=56),
    QuantType(id=30, name="BF16", block=1, size=2),
    QuantType(id=34, name="TQ1_0", block=256, size=54),
    QuantType(id=35, name="TQ2_0", block=256, size=66),
    QuantType(id=39, name="MXFP4", block=32, size=17),
    QuantType(id=40, name="NVFP4", block=64, size=36),
    QuantType(id=41, name="Q1_0", block=128, size=18),
]
"""Every ggml tensor type with the elements one block holds and the bytes it takes."""

TOKENIZER_KEYS = [
    "tokenizer.ggml.model",
    "tokenizer.ggml.pre",
    "tokenizer.ggml.tokens",
    "tokenizer.ggml.token_type",
    "tokenizer.ggml.token_type_count",
    "tokenizer.ggml.scores",
    "tokenizer.ggml.merges",
    "tokenizer.ggml.bos_token_id",
    "tokenizer.ggml.eos_token_id",
    "tokenizer.ggml.eot_token_id",
    "tokenizer.ggml.eom_token_id",
    "tokenizer.ggml.unknown_token_id",
    "tokenizer.ggml.seperator_token_id",
    "tokenizer.ggml.padding_token_id",
    "tokenizer.ggml.mask_token_id",
    "tokenizer.ggml.add_bos_token",
    "tokenizer.ggml.add_eos_token",
    "tokenizer.ggml.add_sep_token",
    "tokenizer.ggml.add_space_prefix",
    "tokenizer.ggml.remove_extra_whitespaces",
    "tokenizer.ggml.precompiled_charsmap",
    "tokenizer.huggingface.json",
    "tokenizer.rwkv.world",
    "tokenizer.chat_template",
    "tokenizer.chat_template.{name}",
    "tokenizer.chat_templates",
    "tokenizer.ggml.fim_pre_token_id",
    "tokenizer.ggml.fim_suf_token_id",
    "tokenizer.ggml.fim_mid_token_id",
    "tokenizer.ggml.fim_pad_token_id",
    "tokenizer.ggml.fim_rep_token_id",
    "tokenizer.ggml.fim_sep_token_id",
    "tokenizer.ggml.prefix_token_id",
    "tokenizer.ggml.suffix_token_id",
    "tokenizer.ggml.middle_token_id",
]
"""Every tokenizer metadata key the reference package declares, in its own order."""


def value_type(id):
    """The metadata value type with wire id `id`, or gguf's enum error."""
    return find_type(VALUE_TYPES, 0, id)


def find_type(types, i, id) -> Result[ValueType, str]:
    """Scan `types` from index `i` for wire id `id`."""
    if i >= len(types):
        return Error(message=toString(id) + " is not a valid GGUFValueType")
    else:
        found = unwrap_or(listGet(types, i), ValueType(id=-1, name=""))
        if found.id == id:
            return Success(value=found)
        else:
            return find_type(types, i + 1, id)


def quant_type(id):
    """The tensor type with wire id `id`, or gguf's enum error."""
    return find_quant(QUANT_TYPES, 0, id)


def find_quant(types, i, id) -> Result[QuantType, str]:
    """Scan `types` from index `i` for wire id `id`."""
    if i >= len(types):
        return Error(message=toString(id) + " is not a valid GGMLQuantizationType")
    else:
        found = unwrap_or(listGet(types, i), QuantType(id=-1, name="", block=1, size=1))
        if found.id == id:
            return Success(value=found)
        else:
            return find_quant(types, i + 1, id)
