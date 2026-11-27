# osprey: flavor=python
# osprey: namespace=gguf
# osprey: module=Reader
"""Read a GGUF file from a byte buffer the way `gguf` 0.19.0's `GGUFReader` does.

`read` returns the version and the two counts, every metadata entry in file order
(the three fixed header figures first, then the file's own keys), every tensor's name,
ggml type, dims as stored, element count, byte size and data offset, the alignment
`general.alignment` declares, the tensor-data offset and the file's byte order. A file
written for the opposite byte order parses too: its version word reads zero in its low
half, exactly the test the reference reader applies.

Every walk is a fold, never recursion per entry, so a file with a million metadata
entries costs a loop and not a stack. Truncated input, an unknown type, a duplicate
key or tensor, a bad alignment and a tensor whose declared bytes are missing are all
`Error`; nothing is guessed and nothing is padded.
"""

from gguf import Chain, Constants, Tensors, Values

__all__ = ["Field", "GGUFFile", "read", "read_header", "read_path", "field", "field_names", "tensor", "tensor_names", "tensor_types", "distinct_tensor_types", "tensor_bytes", "tokenizer_keys", "field_int", "field_bool", "field_float", "field_string", "field_ints", "field_bools", "field_floats", "field_strings"]

INT_KINDS = ["uint8", "int8", "uint16", "int16", "uint32", "int32", "uint64", "int64"]
FLOAT_KINDS = ["float32", "float64"]
TOKENIZER_PREFIX = "tokenizer."


class Field:
    """One metadata entry: where it starts, its key and its value."""
    offset: int
    name: str
    value: Values.Value


class GGUFFile:
    """A parsed file: its buffer, header figures, metadata entries and tensors."""
    data: list[int]
    version: int
    tensor_count: int
    kv_count: int
    alignment: int
    data_offset: int
    byte_order: str
    fields: list[Field]
    tensors: list[Tensors.TensorInfo]


class Section:
    """The metadata section: its entries and where the tensor info section starts."""
    offs: int
    fields: list[Field]


class FieldSized:
    """One metadata entry with the bytes it occupies (package-internal)."""
    field: Field
    size: int


class FieldWalk:
    """A left-to-right read of the metadata section (package-internal)."""
    offs: int
    items: list[Field]
    problem: str


def read(data):
    """Parse a GGUF file from `data`, or Error naming the first thing wrong with it.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 0, 0, 0, 0]):
            case Success(value=f):
                print(f"v{f.version} {f.kv_count} field, {f.tensor_count} tensors, align {f.alignment}")
            case Error(message=m):
                print(m)
        match Reader.read([71, 71, 85, 70]):
            case Success():
                print("read")
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    v3 1 field, 0 tensors, align 32
    gguf: unexpected end of file at offset 4
    ```
    """
    return read_header(data, len(data))


def read_header(prefix, file_size):
    """Parse the metadata and tensor infos from the first bytes of a `file_size`-byte file.

    Every tensor's range is checked against `file_size`, so weights too large to hold
    in a list stay on disk; `tensor_bytes` refuses a tensor past the prefix.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        data = [71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 0, 0, 0, 0]
        match Reader.read_header(data, len(data)):
            case Success(value=f):
                print(f"{f.kv_count} field, data at {f.data_offset}")
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    1 field, data at 64
    ```
    """
    return andThen(Values.header_at(prefix), lambda head: andThen(fields_of(prefix, head), lambda section: assemble(prefix, head, section, file_size)))


def read_path(path, file_size, count):
    """Parse the header of the file at `path`, reading `count` bytes and doubling that
    until the metadata and tensor infos all fit, or until the whole file has been read.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read_path("/nonexistent/model.gguf", 64, 64):
            case Success():
                print("read")
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    readFileBytes: /nonexistent/model.gguf: No such file or directory
    ```
    """
    return andThen(readFileBytes(path, 0, count), lambda prefix: read_prefix(path, file_size, count, prefix))


def read_prefix(path, file_size, count, prefix):
    """The parsed header, or a longer read when this prefix ends inside it."""
    match read_header(prefix, file_size):
        case Success(value=file):
            return Success(value=file)
        case Error(message=message):
            return retry_longer(path, file_size, count, message)


def retry_longer(path, file_size, count, message):
    """Another read at twice the size, or `message` once the whole file has been seen."""
    if count >= file_size:
        return Error(message=message)
    else:
        return read_path(path, file_size, count * 2)


def fields_of(data, head):
    """Every metadata entry in file order, or the first entry that cannot be read."""
    if head.kv_count < 0 or head.kv_count > len(data) - head.offs:
        return Error(message=Values.truncated(head.offs))
    else:
        return checked_walk(data, head)


def checked_walk(data, head):
    """The metadata walk, once its count is known to fit inside the buffer."""
    step = lambda acc, _i: field_step(data, acc, head.swapped)
    walk = Chain.walk_checked(head.kv_count, FieldWalk(offs=head.offs, items=header_fields(head), problem=""), step)
    return andThen(walk, lambda done: Success(value=Section(offs=done.offs, fields=done.items)))


def header_fields(head):
    """The three entries GGUFReader publishes for the fixed header."""
    return [
        header_field(4, "GGUF.version", "uint32", head.version),
        header_field(8, "GGUF.tensor_count", "uint64", head.tensor_count),
        header_field(16, "GGUF.kv_count", "uint64", head.kv_count),
    ]


def header_field(offset, name, kind, number):
    """One fixed header figure as the metadata entry GGUFReader publishes."""
    return Field(offset=offset, name=name, value=Values.Value(kind=kind, item_kind="", i=number, f=0.0, s="", items=[]))


def field_step(data, acc, swapped):
    """Read one more metadata entry into `acc`, or carry its first failure."""
    if acc.problem != "":
        return acc
    else:
        return read_field(data, acc, swapped)


def read_field(data, acc, swapped):
    """One metadata entry appended to `acc`, or `acc` carrying the error."""
    match field_at(data, acc.offs, swapped):
        case Success(value=value):
            return place_field(acc, value)
        case Error(message=message):
            return FieldWalk(offs=acc.offs, items=acc.items, problem=message)


def place_field(acc, entry):
    """`acc` with the entry appended, or gguf's duplicate-key rejection."""
    if listContains(mapList(acc.items, field_name), entry.field.name):
        return FieldWalk(offs=acc.offs, items=acc.items, problem=duplicate_key(entry))
    else:
        return FieldWalk(offs=acc.offs + entry.size, items=listAppend(acc.items, entry.field), problem="")


def duplicate_key(entry):
    """gguf's error for a metadata key the file has already set."""
    return "Duplicate " + entry.field.name + " already in list at offset " + toString(entry.field.offset)


def field_at(data, off, swapped):
    """One metadata entry at `off`: its key string, value type and value."""
    return andThen(Values.string_at(data, off, swapped), lambda key: field_value(data, off, key, swapped))


def field_value(data, off, key, swapped):
    """The entry's value type, which follows its key."""
    return andThen(Values.kind_at(data, off + key.size, swapped), lambda kind: entry_at(data, off, key, kind, swapped))


def entry_at(data, off, key, kind, swapped):
    """The entry's value, once its type is known."""
    return andThen(Values.value_at(data, off + key.size + 4, kind, swapped), lambda value: Success(value=field_sized(off, key, value)))


def field_sized(off, key, value):
    """The finished entry and the bytes it occupies."""
    return FieldSized(field=Field(offset=off, name=key.value.s, value=value.value), size=key.size + 4 + value.size)


def tensors_of(data, head, section, file_size):
    """The tensor section placed against the file, and where its data starts."""
    return andThen(Tensors.alignment_of(section.fields), lambda alignment: Tensors.of(data, section.offs, head.tensor_count, head.swapped, alignment, file_size))


def assemble(data, head, section, file_size):
    """The parsed file, once every tensor's type, size and byte range check out."""
    return andThen(tensors_of(data, head, section, file_size), lambda placed: Success(value=make_file(data, head, section, placed)))


def make_file(data, head, section, placed):
    """The GGUFFile the header, metadata entries and tensors describe."""
    return GGUFFile(data=data, version=head.version, tensor_count=head.tensor_count, kv_count=head.kv_count, alignment=placed.alignment, data_offset=placed.data_offset, byte_order=byte_order_of(head.swapped), fields=section.fields, tensors=placed.tensors)


def byte_order_of(swapped):
    """gguf's byte-order flag: `I` for the file's own order, `S` for the other one."""
    if swapped:
        return "S"
    else:
        return "I"


def first_named(items, name, missing):
    """The first item named `name`, or the Error carrying `missing`."""
    return Chain.first_or_error(filterList(items, lambda item: item.name == name), missing)


def field(file, key):
    """The metadata entry named `key`, or Error when the file has none.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 0, 0, 0, 0]):
            case Success(value=f):
                print(f"{andThen(Reader.field(f, 'general.name'), Reader.field_string)}")
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    Success(tiny)
    ```
    """
    return first_named(file.fields, key, "gguf: the file has no metadata key `" + key + "`")


def field_names(file):
    """Every metadata key in file order.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 20, 0, 0, 0, 0, 0, 0, 0, 116, 111, 107, 101, 110, 105, 122, 101, 114, 46, 103, 103, 109, 108, 46, 109, 111, 100, 101, 108, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 103, 112, 116, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]):
            case Success(value=f):
                print(Reader.field_names(f))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    ["GGUF.version", "GGUF.tensor_count", "GGUF.kv_count", "general.name", "tokenizer.ggml.model"]
    ```
    """
    return mapList(file.fields, field_name)


def field_name(field):
    """An entry's key."""
    return field.name


def tensor(file, index):
    """The tensor at `index`, or Error when the file has fewer tensors.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 1, 0, 0, 0, 0, 0, 0, 0, 119, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 63, 0, 0, 0, 64]):
            case Success(value=f):
                match Reader.tensor(f, 0):
                    case Success(value=t):
                        print(f"{t.name} {t.type} {t.dims} {t.n_bytes} bytes")
                    case Error(message=m):
                        print(m)
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    w F32 [2] 8 bytes
    ```
    """
    if index < 0:
        return Error(message=no_tensor(index))
    else:
        return tensor_at(file.tensors, index)


def tensor_at(tensors, index):
    """The tensor at `index`, or the error naming the absent index."""
    match listGet(tensors, index):
        case Success(value=value):
            return Success(value=value)
        case Error():
            return Error(message=no_tensor(index))


def no_tensor(index):
    """The error naming an absent tensor index."""
    return "gguf: the file has no tensor at index " + toString(index)


def tensor_names(file):
    """Every tensor name, in file order.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 1, 0, 0, 0, 0, 0, 0, 0, 119, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 63, 0, 0, 0, 64]):
            case Success(value=f):
                print(Reader.tensor_names(f))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    ["w"]
    ```
    """
    return mapList(file.tensors, lambda info: info.name)


def tensor_types(file):
    """Every tensor's ggml type name, in file order.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 1, 0, 0, 0, 0, 0, 0, 0, 119, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 63, 0, 0, 0, 64]):
            case Success(value=f):
                print(Reader.tensor_types(f))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    ["F32"]
    ```
    """
    return mapList(file.tensors, lambda info: info.type)


def distinct_tensor_types(file):
    """Each ggml type the file uses once, sorted by name: what a phase reads to know
    which block decoders a file needs.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 1, 0, 0, 0, 0, 0, 0, 0, 119, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 63, 0, 0, 0, 64]):
            case Success(value=f):
                print(Reader.distinct_tensor_types(f))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    ["F32"]
    ```
    """
    return listSort(distinct(tensor_types(file)))


def distinct(values):
    """`values` without repeats, in first-seen order."""
    return foldList(values, [], lambda acc, item: keep_new(acc, item))


def keep_new(acc, item):
    """`acc` with `item` appended when it is not already there."""
    if listContains(acc, item):
        return acc
    else:
        return listAppend(acc, item)


def tensor_bytes(file, name):
    """The tensor's stored bytes, before any block decoding.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 1, 0, 0, 0, 0, 0, 0, 0, 119, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 63, 0, 0, 0, 64]):
            case Success(value=f):
                print(Reader.tensor_bytes(f, "w"))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    Success([0, 0, 128, 63, 0, 0, 0, 64])
    ```
    """
    return andThen(named_tensor(file.tensors, name), lambda value: Values.bytes_at(file.data, value.data_offset, value.n_bytes))


def named_tensor(tensors, name):
    """The first tensor named `name`, or the error naming the absent one."""
    return first_named(tensors, name, no_tensor_named(name))


def no_tensor_named(name):
    """The error naming an absent tensor."""
    return "gguf: the file has no tensor named `" + name + "`"


def tokenizer_keys(file):
    """The metadata keys the file sets under `tokenizer.`, in file order: what tells a
    phase which vocabulary, merges and chat template the file already carries.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 20, 0, 0, 0, 0, 0, 0, 0, 116, 111, 107, 101, 110, 105, 122, 101, 114, 46, 103, 103, 109, 108, 46, 109, 111, 100, 101, 108, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 103, 112, 116, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]):
            case Success(value=f):
                print(Reader.tokenizer_keys(f))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    ["tokenizer.ggml.model"]
    ```
    """
    return mapList(filterList(file.fields, is_tokenizer_field), field_name)


def is_tokenizer_field(field):
    """Whether the entry is a tokenizer metadata key."""
    return startsWith(field.name, TOKENIZER_PREFIX)


def holds(field, ok, label, value):
    """`value` when the entry passes `ok`, or the Error naming the kind it does not
    hold. The value is read eagerly: a field read cannot fault, and a closure here
    would leave the accessor generic for the code generator."""
    if ok:
        return Success(value=value)
    else:
        return Error(message=kind_error(field, label))


def field_int(field):
    """The integer a metadata entry holds, or Error for another kind.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 99, 111, 117, 110, 116, 4, 0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]):
            case Success(value=f):
                print(andThen(Reader.field(f, "count"), Reader.field_int))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    Success(7)
    ```
    """
    return holds(field, listContains(INT_KINDS, field.value.kind), "an int", field.value.i)


def field_bool(field):
    """The boolean a metadata entry holds, or Error for another kind.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 102, 108, 97, 103, 7, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]):
            case Success(value=f):
                print(andThen(Reader.field(f, "flag"), Reader.field_bool))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    Success(true)
    ```
    """
    return holds(field, field.value.kind == "bool", "a bool", field.value.i != 0)


def field_float(field):
    """The number a floating-point metadata entry holds, or Error for another kind.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 114, 97, 116, 105, 111, 6, 0, 0, 0, 0, 0, 0, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]):
            case Success(value=f):
                print(andThen(Reader.field(f, "ratio"), Reader.field_float))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    Success(0.5)
    ```
    """
    return holds(field, listContains(FLOAT_KINDS, field.value.kind), "a float", field.value.f)


def field_string(field):
    """The text a string metadata entry holds, or Error for another kind.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 103, 101, 110, 101, 114, 97, 108, 46, 110, 97, 109, 101, 8, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 105, 110, 121, 0, 0, 0, 0]):
            case Success(value=f):
                print(andThen(Reader.field(f, "general.name"), Reader.field_string))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    Success(tiny)
    ```
    """
    return holds(field, field.value.kind == "string", "a string", field.value.s)


def array_field(field, kinds, label, element):
    """Every `element` of the entry's items when it is an array of one of `kinds`."""
    return holds(field, array_of(field, kinds), label, mapList(field.value.items, element))


def field_ints(field):
    """The integers an array entry holds, or Error for another kind.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 105, 100, 115, 9, 0, 0, 0, 5, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 3, 0, 0, 0, 0]):
            case Success(value=f):
                print(andThen(Reader.field(f, "ids"), Reader.field_ints))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    Success([1, 2, 3])
    ```
    """
    return array_field(field, INT_KINDS, "an int array", element_int)


def field_bools(field):
    """The booleans an array entry holds, or Error for another kind.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 109, 97, 114, 107, 115, 9, 0, 0, 0, 7, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]):
            case Success(value=f):
                print(andThen(Reader.field(f, "marks"), Reader.field_bools))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    Success([true, false])
    ```
    """
    return array_field(field, ["bool"], "a bool array", element_bool)


def field_floats(field):
    """The numbers a floating-point array entry holds, or Error for another kind.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 115, 99, 97, 108, 101, 115, 9, 0, 0, 0, 6, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 63, 0, 0, 0, 64, 0, 0]):
            case Success(value=f):
                print(andThen(Reader.field(f, "scales"), Reader.field_floats))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    Success([0.5, 2.0])
    ```
    """
    return array_field(field, FLOAT_KINDS, "a float array", element_float)


def field_strings(field):
    """The texts a string array entry holds, or Error for another kind.

    # Examples
    ```osprey
    from gguf import Reader

    def show():
        match Reader.read([71, 71, 85, 70, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 116, 97, 103, 115, 9, 0, 0, 0, 8, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 97, 1, 0, 0, 0, 0, 0, 0, 0, 98, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]):
            case Success(value=f):
                print(andThen(Reader.field(f, "tags"), Reader.field_strings))
            case Error(message=m):
                print(m)

    wrappingArithmetic(show)
    ```
    ```output
    Success(["a", "b"])
    ```
    """
    return array_field(field, ["string"], "a string array", element_string)


def array_of(field, kinds):
    """Whether the entry is an array of one of `kinds`, or an empty array."""
    if field.value.kind != "array":
        return False
    elif field.value.item_kind == "":
        return True
    else:
        return listContains(kinds, field.value.item_kind)


def element_int(item):
    """The integer a metadata value holds."""
    return item.i


def element_bool(item):
    """The boolean a metadata value holds."""
    return item.i != 0


def element_float(item):
    """The number a metadata value holds."""
    return item.f


def element_string(item):
    """The text a metadata value holds."""
    return item.s


def kind_error(field, wanted):
    """The error naming what an entry is, when it is not `wanted`."""
    return "gguf: metadata key `" + field.name + "` is not " + wanted + ", it is " + kind_of(field.value)


def kind_of(value):
    """A value's kind, naming an array's element kind when it has one."""
    if value.kind != "array":
        return value.kind
    elif value.item_kind == "":
        return "an empty array"
    else:
        return "an array of " + value.item_kind
