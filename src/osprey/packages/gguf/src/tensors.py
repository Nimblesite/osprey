# osprey: flavor=python
# osprey: namespace=gguf
# osprey: module=Tensors
"""The tensor section of a GGUF file: every tensor's info entry, the alignment
`general.alignment` declares, the tensor-data offset and each tensor's byte range.

A tensor info entry is a name, a dim count, that many uint64 dims as stored (innermost
first), a ggml type id and an offset within the data section. The entry's byte size is
`elements * size // block` from the type's block and byte sizes, which is why a
quantized row must be a whole number of blocks. Nothing here decodes blocks: the
reader reports where each tensor's bytes are, so a later phase can decode them.
"""

from gguf import Chain, Constants, Values
from safetensors import Bytes

__all__ = ["TensorInfo", "of", "alignment_of"]

ALIGNMENT_KEY = "general.alignment"


class TensorInfo:
    """One tensor's info entry: its name, ggml type, dims as stored (innermost
    first), element count, byte size and where its bytes start in the buffer."""
    name: str
    type: str
    type_id: int
    dims: list[int]
    n_elements: int
    n_bytes: int
    data_offset: int


class Placed:
    """The tensor section: the alignment, where the data starts and every tensor."""
    alignment: int
    data_offset: int
    tensors: list[TensorInfo]


class TensorDraft:
    """A tensor info entry while it is read: name, dims, type and raw data offset."""
    off: int
    name: str
    next: int
    dims: list[int]
    type_id: int
    raw_offset: int


class TensorSized:
    """One tensor info entry with the bytes it occupies."""
    draft: TensorDraft
    size: int


class TensorWalk:
    """A left-to-right read of the tensor info section."""
    offs: int
    drafts: list[TensorDraft]
    problem: str


class PlaceWalk:
    """The tensors placed so far, or the first one that could not be."""
    items: list[TensorInfo]
    problem: str


def of(data, offs, count, swapped, alignment, file_size):
    """The tensor section: every entry checked against a file of `file_size` bytes, and
    where the tensor data starts once `offs` is rounded up to the alignment."""
    return Chain.then(walk(data, offs, count, swapped), lambda walked: place_all(file_size, walked, alignment))


def place_all(file_size, walked, alignment):
    """The placed tensors and the tensor-data offset."""
    start = layout_offset(alignment, walked.offs)
    return Chain.then(place_tensors(file_size, walked.drafts, start), lambda tensors: Success(value=Placed(alignment=alignment, data_offset=start, tensors=tensors)))


def walk(data, offs, count, swapped):
    """Every tensor info entry in file order, or the first that cannot be read."""
    if count < 0 or count > len(data) - offs:
        return Error(message=Values.truncated(offs))
    else:
        return checked_walk(data, offs, count, swapped)


def checked_walk(data, offs, count, swapped):
    """The tensor info walk, once its count is known to fit inside the buffer."""
    step = lambda acc, _i: tensor_step(data, acc, swapped)
    return Chain.walk_checked(count, TensorWalk(offs=offs, drafts=[], problem=""), step)


def tensor_step(data, acc, swapped):
    """Read one more tensor info entry into `acc`, or carry its first failure."""
    if acc.problem != "":
        return acc
    else:
        return read_tensor(data, acc, swapped)


def read_tensor(data, acc, swapped):
    """One tensor info entry appended to `acc`, or `acc` carrying the error."""
    match tensor_info_at(data, acc.offs, swapped):
        case Success(value=value):
            return TensorWalk(offs=acc.offs + value.size, drafts=listAppend(acc.drafts, value.draft), problem="")
        case Error(message=message):
            return TensorWalk(offs=acc.offs, drafts=acc.drafts, problem=message)


def tensor_info_at(data, off, swapped):
    """One tensor info entry: its name, dim count, dims, ggml type and data offset."""
    return Chain.then(Values.string_at(data, off, swapped), lambda name: draft_dim_count(data, draft_named(off, name), swapped))


def draft_named(off, name):
    """The draft of an entry whose name has just been read."""
    return TensorDraft(off=off, name=name.value.s, next=off + name.size, dims=[], type_id=0, raw_offset=0)


def draft_dim_count(data, draft, swapped):
    """The dim count that follows the tensor's name, then the dims themselves."""
    return Chain.then(Values.value_at(data, draft.next, "uint32", swapped), lambda count: draft_dims(data, draft, count.value.i, swapped))


def draft_dims(data, draft, count, swapped):
    """The dims as stored, innermost first, then the type that follows them."""
    return Chain.then(dims_at(data, draft.next + 4, count, swapped), lambda dims: draft_type(data, with_dims(draft, dims, count), swapped))


def with_dims(draft, dims, count):
    """The draft with its dims read and its cursor past them."""
    return TensorDraft(off=draft.off, name=draft.name, next=draft.next + 4 + 8 * count, dims=dims, type_id=0, raw_offset=0)


def dims_at(data, off, count, swapped):
    """The `count` uint64 dims at `off`."""
    return Chain.then(Values.values_at(data, off, count, "uint64", swapped), lambda value: Success(value=mapList(value.value.items, dim_value)))


def dim_value(item):
    """The integer a uint64 metadata value holds."""
    return item.i


def draft_type(data, draft, swapped):
    """The tensor's ggml type id, then its offset within the data section."""
    return Chain.then(Values.value_at(data, draft.next, "uint32", swapped), lambda id: draft_offset(data, with_type(draft, id.value.i), swapped))


def with_type(draft, id):
    """The draft with its type read and its cursor past it."""
    return TensorDraft(off=draft.off, name=draft.name, next=draft.next + 4, dims=draft.dims, type_id=id, raw_offset=0)


def draft_offset(data, draft, swapped):
    """The tensor's offset within the data section, which completes the entry."""
    return Chain.then(Values.value_at(data, draft.next, "uint64", swapped), lambda offset: Success(value=TensorSized(draft=with_offset(draft, offset.value.i), size=draft.next + 8 - draft.off)))


def with_offset(draft, offset):
    """The draft with its data offset read, which completes it."""
    return TensorDraft(off=draft.off, name=draft.name, next=draft.next + 8, dims=draft.dims, type_id=draft.type_id, raw_offset=offset)


def alignment_of(fields):
    """The alignment `general.alignment` declares, or gguf's default when absent."""
    declared = filterList(fields, names_alignment)
    if len(declared) == 0:
        return Success(value=Constants.DEFAULT_ALIGNMENT)
    else:
        return Chain.then(listGet(declared, 0), checked_alignment)


def names_alignment(field):
    """Whether the metadata entry is the alignment declaration."""
    return field.name == ALIGNMENT_KEY


def checked_alignment(field):
    """The declared alignment, or gguf's own rejection of a bad one."""
    if field.value.kind != "uint32":
        return Error(message="Bad type for general.alignment field")
    elif is_power_of_two(field.value.i):
        return Success(value=field.value.i)
    else:
        return Error(message="Invalid alignment: must be a non-zero power of two")


def is_power_of_two(value):
    """Whether `value` is a positive power of two; zero and other values are not."""
    if value <= 0 or value % 2 != 0:
        return value == 1
    else:
        return is_power_of_two(intDiv(value, 2))


def layout_offset(alignment, end):
    """`end` rounded up to `alignment`: where the tensor data starts."""
    pad = end % alignment
    if pad == 0:
        return end
    else:
        return end + alignment - pad


def place_tensors(file_size, drafts, start):
    """Every tensor's info record, or the first tensor the file cannot satisfy."""
    walked = foldList(drafts, PlaceWalk(items=[], problem=""), lambda acc, draft: place_step(file_size, acc, draft, start))
    if walked.problem != "":
        return Error(message=walked.problem)
    else:
        return Success(value=walked.items)


def place_step(file_size, acc, draft, start):
    """Place one more tensor into `acc`, or carry its first failure."""
    if acc.problem != "":
        return acc
    else:
        return place_new(file_size, acc, draft, start)


def place_new(file_size, acc, draft, start):
    """One tensor appended to `acc`, or `acc` carrying the error."""
    if listContains(mapList(acc.items, tensor_name), draft.name):
        return PlaceWalk(items=acc.items, problem="Found duplicated tensor with name " + draft.name)
    else:
        return placed_tensor(file_size, acc, draft, start)


def tensor_name(info):
    """A tensor's name."""
    return info.name


def placed_tensor(file_size, acc, draft, start):
    """The tensor placed against the buffer, or `acc` carrying its error."""
    match tensor_of(draft, start, file_size):
        case Success(value=value):
            return PlaceWalk(items=listAppend(acc.items, value), problem="")
        case Error(message=message):
            return PlaceWalk(items=acc.items, problem=message)


def tensor_of(draft, start, buffer_size):
    """One tensor's info record, once its type, size and bytes all check out."""
    return Chain.then(Constants.quant_type(draft.type_id), lambda quant: placed(draft, quant, start, buffer_size))


def placed(draft, quant, start, buffer_size):
    """The record, once the tensor's innermost dim fits its type's block size."""
    if block_ok(draft.dims, quant):
        return placed_in_buffer(draft, quant, start, buffer_size)
    else:
        return Error(message=block_message(draft.dims, quant))


def block_ok(dims, quant):
    """Whether the innermost dim is a whole number of the type's blocks."""
    if quant.block == 1:
        return True
    elif len(dims) == 0:
        return False
    else:
        return unwrap_or(listGet(dims, 0), 0) % quant.block == 0


def block_message(dims, quant):
    """gguf's error for a row that is not a whole number of blocks."""
    return "Quantized tensor row size (" + toString(unwrap_or(listGet(dims, 0), 0)) + ") is not a multiple of " + quant.name + " block size (" + toString(quant.block) + ")"


def placed_in_buffer(draft, quant, start, buffer_size):
    """The record, once the tensor's bytes are known to start inside the buffer."""
    if draft.raw_offset > buffer_size - start:
        return Error(message=truncated_tensor(draft.name, buffer_size))
    else:
        return checked_tensor(draft, quant, start, buffer_size)


def checked_tensor(draft, quant, start, buffer_size):
    """The record, once the tensor's whole byte range lies inside the buffer."""
    offset = start + draft.raw_offset
    size = tensor_byte_count(draft.dims, quant)
    if size < 0:
        return Error(message="gguf: the dims of tensor `" + draft.name + "` overflow its byte size")
    elif size > buffer_size - offset:
        return Error(message=truncated_tensor(draft.name, offset))
    else:
        return Success(value=tensor_record(draft, quant, offset, size))


def tensor_record(draft, quant, offset, size):
    """The info record for a tensor whose byte range is known to fit."""
    return TensorInfo(name=draft.name, type=quant.name, type_id=quant.id, dims=draft.dims, n_elements=element_count(draft.dims), n_bytes=size, data_offset=offset)


def truncated_tensor(name, offset):
    """The error for a tensor whose declared bytes are not all in the buffer."""
    return "gguf: tensor `" + name + "` data is truncated at offset " + toString(offset)


def element_count(dims):
    """The number of elements `dims` multiply to, or -1 when that overflows."""
    return foldList(dims, 1, lambda acc, dim: Bytes.checked_mul(acc, dim))


def tensor_byte_count(dims, quant):
    """The bytes a tensor occupies, or -1 when its count or size overflows."""
    elements = element_count(dims)
    if elements < 0:
        return -1
    else:
        return intDiv(Bytes.checked_mul(elements, quant.size), quant.block)
