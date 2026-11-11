# osprey: flavor=python
# osprey: namespace=gguf
# osprey: module=Values
"""Byte words, the fixed GGUF header and every metadata value kind, decoded from a
GGUF byte buffer the way `gguf` 0.19.0 decodes them.

A file holds little-endian words, and a file written for the other byte order reverses
each word before it is decoded: its version word has a zero low half, which is exactly
the test the reference reader applies. A metadata value is an integer, a float, a
boolean, a string or an array of them, held in one tagged `Value` record.

Every read is bounds-checked: input that ends before a field it declares is an `Error`,
never a short read, and a uint64 that does not fit an int64 is an `Error` rather than a
wrapped number.
"""

from gguf import Chain, Constants
from safetensors import Bytes

__all__ = ["Value", "header_at", "value_at", "values_at", "string_at", "kind_at", "bytes_at", "truncated"]


class Value:
    """One metadata value. `kind` names its GGUF value type; a scalar fills the
    payload field its type uses (`i`, `f` or `s`), and an array fills `items` and
    `item_kind`, which stays empty while the array has no element to tell it."""
    kind: str
    item_kind: str
    i: int
    f: float
    s: str
    items: list[Value]


class Sized:
    """A value with the number of bytes it occupies in the file (package-internal)."""
    value: Value
    size: int


class ArrayWalk:
    """A left-to-right read of a run of values (package-internal)."""
    offs: int
    items: list[Sized]
    problem: str


class Header:
    """The fixed header: the version, the two counts, the byte order and where the
    metadata section starts (package-internal)."""
    version: int
    tensor_count: int
    kv_count: int
    swapped: bool
    offs: int


class Counts:
    """The two counts that follow the version word (package-internal)."""
    tensor_count: int
    kv_count: int


def header_at(data):
    """The magic, the version and the two counts, in the file's own byte order."""
    return andThen(require_magic(data), lambda _: version_at(data))


def require_magic(data):
    """The file's magic, or gguf's rejection when the first four bytes differ."""
    return andThen(unsigned_at(data, 0, 4, False), checked_magic)


def checked_magic(magic):
    """`magic` itself when the file is a GGUF file."""
    if magic != Constants.MAGIC:
        return Error(message="GGUF magic invalid")
    else:
        return Success(value=magic)


def version_at(data):
    """The version word, which also says whether the file is byte-swapped."""
    return andThen(bytes_at(data, 4, 4), lambda word: checked_version(data, word))


def checked_version(data, word):
    """The version, its byte order and the counts that follow it."""
    swapped = number_of(word) % 65536 == 0
    version = number_of(ordered_word(word, swapped))
    if not listContains(Constants.SUPPORTED_VERSIONS, version):
        return Error(message="Sorry, file appears to be version " + toString(version) + " which we cannot handle")
    else:
        return andThen(counts_at(data, swapped), lambda counts: Success(value=header_of(version, counts, swapped)))


def header_of(version, counts, swapped):
    """The header a version and two counts describe."""
    return Header(version=version, tensor_count=counts.tensor_count, kv_count=counts.kv_count, swapped=swapped, offs=24)


def counts_at(data, swapped):
    """The tensor and metadata counts that follow the version word."""
    return andThen(u64_at(data, 8, swapped), lambda tensor_count: andThen(u64_at(data, 16, swapped), lambda kv_count: Success(value=Counts(tensor_count=tensor_count, kv_count=kv_count))))


def value_at(data, off, kind, swapped):
    """The metadata value of `kind` at `off`, with the bytes it occupies."""
    if kind == "string":
        return string_at(data, off, swapped)
    elif kind == "array":
        return array_at(data, off, swapped)
    else:
        return scalar_at(data, off, kind, swapped)


def kind_at(data, off, swapped):
    """The name of the metadata value type the 4-byte id at `off` names."""
    return andThen(unsigned_at(data, off, 4, swapped), kind_name)


def kind_name(id):
    """A value type's name, or gguf's enum error for an id that has no member."""
    return andThen(Constants.value_type(id), name_of_type)


def name_of_type(kind):
    """The name of a value type record."""
    return Success(value=kind.name)


def scalar_at(data, off, kind, swapped):
    """A fixed-width scalar of `kind` at `off`, with the bytes it occupies."""
    return andThen(word_at(data, off, width_of(kind), swapped), lambda word: decode_word(word, kind, off))


def decode_word(word, kind, off):
    """The scalar whose bytes are `word`, least significant first."""
    if kind == "float32":
        return Success(value=Sized(value=float_value(kind, Bytes.decode_f32(word, 0)), size=4))
    elif kind == "float64":
        return Success(value=Sized(value=float_value(kind, Bytes.decode_f64(word, 0)), size=8))
    elif kind == "bool":
        return Success(value=Sized(value=bool_value(unwrap_or(listGet(word, 0), 0)), size=1))
    elif kind == "uint64":
        return andThen(u64_of(word, off), lambda number: Success(value=Sized(value=int_value(kind, number), size=8)))
    elif kind == "int64":
        return andThen(int64_of(word), lambda number: Success(value=Sized(value=int_value(kind, number), size=8)))
    else:
        return Success(value=Sized(value=int_value(kind, integer_of(word, kind)), size=width_of(kind)))


def integer_of(word, kind):
    """A narrower integer, two's-complement for the signed kinds."""
    number = number_of(word)
    if kind == "int8":
        return signed(number, 8)
    elif kind == "int16":
        return signed(number, 16)
    elif kind == "int32":
        return signed(number, 32)
    else:
        return number


def signed(number, bits):
    """The two's-complement value of `number` read as a `bits`-bit word."""
    half = Bytes.pow2i(bits - 1)
    if number >= half:
        return number - 2 * half
    else:
        return number


def u64_of(word, off):
    """The unsigned 64-bit word `word`, or Error above the int64 range."""
    high = unwrap_or(listGet(word, 7), 0)
    if high >= 128:
        return Error(message="gguf: the uint64 at offset " + toString(off) + " exceeds the int64 range")
    else:
        return Success(value=high * 72057594037927936 + number_of(word))


def int64_of(word):
    """The signed 64-bit word `word`, whose top bit makes the value negative."""
    low = Bytes.le_bytes(word, 0, 7)
    high = unwrap_or(listGet(word, 7), 0)
    if high >= 128:
        return Success(value=(high - 256) * 72057594037927936 + low)
    else:
        return Success(value=high * 72057594037927936 + low)


def width_of(kind):
    """The byte width of a fixed-width metadata value type."""
    if kind == "uint8" or kind == "int8" or kind == "bool":
        return 1
    elif kind == "uint16" or kind == "int16":
        return 2
    elif kind == "float32" or kind == "uint32" or kind == "int32":
        return 4
    else:
        return 8


def int_value(kind, number):
    """An integer metadata value."""
    return Value(kind=kind, item_kind="", i=number, f=0.0, s="", items=[])


def float_value(kind, number):
    """A floating-point metadata value."""
    return Value(kind=kind, item_kind="", i=0, f=number, s="", items=[])


def bool_value(byte):
    """A boolean metadata value, which GGUF stores as one byte."""
    if byte != 0:
        return Value(kind="bool", item_kind="", i=1, f=0.0, s="", items=[])
    else:
        return Value(kind="bool", item_kind="", i=0, f=0.0, s="", items=[])


def string_at(data, off, swapped):
    """A GGUF string at `off`: a uint64 byte count, then that many UTF-8 bytes."""
    return andThen(u64_at(data, off, swapped), lambda count: string_body(data, off + 8, count))


def string_body(data, off, count):
    """The UTF-8 text of `count` bytes at `off`, with the entry's whole byte size."""
    return andThen(text_at(data, off, count), lambda text: Success(value=Sized(value=string_value(text), size=8 + count)))


def text_at(data, off, count):
    """`count` bytes at `off` as text, or Error when they are not valid UTF-8."""
    return andThen(bytes_at(data, off, count), Bytes.bytes_to_string)


def string_value(text):
    """A string metadata value."""
    return Value(kind="string", item_kind="", i=0, f=0.0, s=text, items=[])


def array_at(data, off, swapped):
    """An array at `off`: an element type id, a uint64 count, then the elements."""
    return andThen(kind_at(data, off, swapped), lambda kind: array_body(data, off, kind, swapped))


def array_body(data, off, kind, swapped):
    """The element count of the array whose element type is `kind`."""
    return andThen(u64_at(data, off + 4, swapped), lambda count: array_values(data, off, kind, count, swapped))


def array_values(data, off, kind, count, swapped):
    """The array's elements, with the whole entry's byte size."""
    return andThen(values_at(data, off + 12, count, kind, swapped), lambda value: Success(value=Sized(value=value.value, size=12 + value.size)))


def array_value(kind, count, items):
    """An array metadata value; an empty array cannot say what its elements are."""
    if count == 0:
        return Value(kind="array", item_kind="", i=0, f=0.0, s="", items=items)
    else:
        return Value(kind="array", item_kind=kind, i=0, f=0.0, s="", items=items)


def values_at(data, off, count, kind, swapped):
    """`count` values of `kind` in sequence, with the bytes they occupy."""
    if count < 0 or off < 0 or count > len(data) - off:
        return Error(message=truncated(off))
    else:
        return walked(data, off, count, kind, swapped)


def walked(data, off, count, kind, swapped):
    """The fold that reads `count` values from `off` without recursing per element."""
    step = lambda acc, _i: value_step(data, acc, kind, swapped)
    walk = Chain.walk_checked(count, ArrayWalk(offs=off, items=[], problem=""), step)
    return andThen(walk, lambda done: Success(value=Sized(value=array_value(kind, count, mapList(done.items, item_value)), size=done.offs - off)))


def item_value(sized):
    """The value a sized value holds."""
    return sized.value


def value_step(data, acc, kind, swapped):
    """Read one more value into `acc`, or carry the first failure."""
    if acc.problem != "":
        return acc
    else:
        return take_value(data, acc, kind, swapped)


def take_value(data, acc, kind, swapped):
    """One value appended to `acc`, or `acc` carrying its error."""
    match value_at(data, acc.offs, kind, swapped):
        case Success(value=value):
            return ArrayWalk(offs=acc.offs + value.size, items=listAppend(acc.items, value), problem="")
        case Error(message=problem):
            return ArrayWalk(offs=acc.offs, items=acc.items, problem=problem)


def bytes_at(data, off, n):
    """The `n` bytes at `off`, or the truncation error naming the offset."""
    match Bytes.read_bytes(data, off, n):
        case Success(value=value):
            return Success(value=value)
        case Error(message=_problem):
            return Error(message=truncated(off))


def truncated(off):
    """The error text for input that ends before a field it declares."""
    return "gguf: unexpected end of file at offset " + toString(off)


def word_at(data, off, n, swapped):
    """The `n` bytes at `off`, reversed when the file's words are byte-swapped."""
    return andThen(bytes_at(data, off, n), lambda word: Success(value=ordered_word(word, swapped)))


def ordered_word(word, swapped):
    """`word` in file order, or reversed for a file of the opposite byte order."""
    if swapped:
        return listReverse(word)
    else:
        return word


def number_of(word):
    """The unsigned integer `word` spells, least significant byte first."""
    return Bytes.le_bytes(word, 0, len(word))


def unsigned_at(data, off, n, swapped):
    """The unsigned `n`-byte word at `off`, n at most 7 so it cannot overflow."""
    return andThen(word_at(data, off, n, swapped), lambda word: Success(value=number_of(word)))


def u64_at(data, off, swapped):
    """The uint64 at `off`, or Error when it exceeds the int64 range."""
    return andThen(word_at(data, off, 8, swapped), lambda word: u64_of(word, off))
