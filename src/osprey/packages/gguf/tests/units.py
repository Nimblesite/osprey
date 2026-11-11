# osprey: flavor=python
# osprey: namespace=gguf_tests
# osprey: module=Units
"""The gguf port's low-level reads exercised on their own, without the file walk in
front of them: the fixed header, each metadata entry's key and value decoded straight
from the offset the oracle recorded, the tensor section parsed directly, and the
bounds checks that turn bad input into an `Error` instead of a value.
"""
from gguf import Chain, Constants, Reader, Tensors, Values
from gguf_expected import Expected
from testing import Check

from gguf_tests.Support import fixtures, loaded, swapped, str_list_equal, ats, ati, atl, cell_case, equal_case, error_case

__all__ = ["cases"]


def cases():
    """Every low-level read the port exposes, on the fixtures the oracle wrote."""
    return listConcat(header_cases(), listConcat(value_cases(), listConcat(bytes_cases(), listConcat(section_cases(), listConcat(record_cases(), listConcat(limit_cases(), bounds_cases()))))))


def header_cases():
    """The fixed header read on its own, and the inputs it refuses."""
    match Values.header_at(Expected.BASE_BYTES):
        case Success(value=head):
            read = [
                Check.equal("values.header.version", head.version, ati(Expected.BASE_HEADER, 0)),
                Check.equal("values.header.tensor_count", head.tensor_count, ati(Expected.BASE_HEADER, 1)),
                Check.equal("values.header.kv_count", head.kv_count, ati(Expected.BASE_HEADER, 2)),
                Check.equal("values.header.swapped", head.swapped, False),
            ]
            return listConcat(read, header_refusal_cases())
        case Error(message=problem):
            return [Check.failed("values.header", problem)]


def header_refusal_cases():
    """A bad magic and a short buffer are errors; gguf's own message is compared."""
    return [recorded_refusal("values.header.magic", Expected.R_MAGIC_BYTES, Expected.R_MAGIC_MESSAGE),
            error_case("values.header.short", Values.header_at(Expected.R_SHORT_BYTES)),
            Check.equal("values.header.swapped_be", swapped_of(Expected.BE_BYTES), True)]


def swapped_of(data):
    """Whether the header of `data` says its words are byte-swapped."""
    match Values.header_at(data):
        case Success(value=head):
            return head.swapped
        case Error(message=_problem):
            return False


def recorded_refusal(tag, data, want):
    """A read of `data` that must fail with gguf's own recorded message."""
    match Values.header_at(data):
        case Success(value=_head):
            return Check.failed(tag, "the header parsed an input gguf refuses")
        case Error(message=problem):
            return Check.equal(tag, problem, want)


def value_cases():
    """Each fixture's own entries decoded straight from their recorded offsets."""
    return foldList(fixtures(), [], lambda acc, fixture: listConcat(acc, fixture_value_cases(fixture)))


def fixture_value_cases(fixture):
    """Every entry a fixture sets, read three ways: key, declared type and value."""
    return foldList(real_indices(fixture.value_offsets), [], lambda acc, i: listConcat(acc, entry_cases(fixture, i)))


def real_indices(offsets):
    """The indices of the entries the file itself sets: the header figures it publishes
    carry no offset of their own, and the oracle records -1 for those."""
    return filterList(toList(range(len(offsets))), lambda i: ati(offsets, i) >= 0)


def entry_cases(fixture, i):
    """One entry's key text, its declared type and its value, read by offset."""
    kind = ats(fixture.kinds, i)
    name = ats(fixture.fields, i)
    key = fixture.name + ".value." + name
    return [string_case(key + ".key", fixture, ati(fixture.offsets, i), name),
            kind_case(key + ".kind", fixture, i, kind),
            value_case(key, fixture, i, kind, atl(fixture.ints, i), atl(fixture.floats, i), atl(fixture.bools, i), atl(fixture.strings, i))]


def string_case(tag, fixture, off, want):
    """A metadata key decoded straight from its offset."""
    match Values.string_at(fixture.data, off, swapped(fixture)):
        case Success(value=sized):
            return cell_case(tag, Reader.Field(offset=off, name=tag, value=sized.value), "string", [], [], [], [want])
        case Error(message=problem):
            return Check.failed(tag, problem)


def kind_case(tag, fixture, i, want):
    """An entry's declared value type, read straight from its type word."""
    return equal_case(tag, Values.kind_at(fixture.data, ati(fixture.type_offsets, i), swapped(fixture)), want)


def value_case(tag, fixture, i, kind, ints, floats, bools, strings):
    """An entry's value decoded straight from its value offset."""
    match Values.value_at(fixture.data, ati(fixture.value_offsets, i), kind, swapped(fixture)):
        case Success(value=sized):
            return cell_case(tag, Reader.Field(offset=ati(fixture.value_offsets, i), name=tag, value=sized.value), kind, ints, floats, bools, strings)
        case Error(message=problem):
            return Check.failed(tag, problem)


def bytes_cases():
    """Raw bytes, an array's elements and the truncation text, without a file walk."""
    return listConcat(byte_cases(), array_cases())


def byte_cases():
    """A tensor's bytes read by offset, and the message a short read reports."""
    return [bytes_case("values.bytes", ati(Expected.BASE_TENSOR_OFFSETS, 0), ati(Expected.BASE_TENSOR_SIZES, 0), atl(Expected.BASE_TENSOR_DATA, 0)),
            truncation_case("values.truncated", len(Expected.BASE_BYTES))]


def bytes_case(tag, off, count, want):
    """`count` bytes at `off` compared with the tensor bytes the oracle recorded."""
    match Values.bytes_at(Expected.BASE_BYTES, off, count):
        case Success(value=got):
            return Check.ints_equal(tag, got, want)
        case Error(message=problem):
            return Check.failed(tag, problem)


def truncation_case(tag, off):
    """A read past the end reports the offset, naming the same text `truncated` gives."""
    match Values.bytes_at(Expected.BASE_BYTES, off, 1):
        case Success(value=_got):
            return Check.failed(tag, "a read past the end returned a value")
        case Error(message=problem):
            return Check.equal(tag, problem, Values.truncated(off))


def array_cases():
    """An array entry's elements read through `values_at`, and its empty siblings."""
    entry = index_of(Expected.BASE_FIELDS, "ints")
    if entry < 0:
        return [Check.failed("values.values_at", "the fixture has no `ints` array")]
    else:
        return [values_at_case("values.values_at", entry)]


def values_at_case(tag, entry):
    """One array's elements read straight from its elements' first offset."""
    count = len(atl(Expected.BASE_FIELD_INTS, entry))
    off = ati(Expected.BASE_FIELD_VALUE_OFFSETS, entry) + 12
    match Values.values_at(Expected.BASE_BYTES, off, count, "int32", False):
        case Success(value=sized):
            return cell_case(tag, Reader.Field(offset=off, name=tag, value=sized.value), "array", atl(Expected.BASE_FIELD_INTS, entry), [], [], [])
        case Error(message=problem):
            return Check.failed(tag, problem)


def prefix_of(data, count):
    """The first `count` bytes of `data`."""
    return unwrap_or(Values.bytes_at(data, 0, count), [])


def header_only_cases(fixture):
    """A prefix that stops where the tensor data starts reads like the whole file."""
    size = len(fixture.data)
    start = ati(fixture.header, 4)
    match Reader.read_header(prefix_of(fixture.data, start), size):
        case Success(value=file):
            return [
                Check.equal(fixture.name + ".header_only.data_offset", file.data_offset, start),
                str_list_equal(fixture.name + ".header_only.names", Reader.tensor_names(file), fixture.tensors),
                str_list_equal(fixture.name + ".header_only.keys", Reader.field_names(file), fixture.fields),
                Check.ints_equal(fixture.name + ".header_only.sizes", mapList(file.tensors, lambda info: info.n_bytes), fixture.sizes),
                Check.ints_equal(fixture.name + ".header_only.starts", mapList(file.tensors, lambda info: info.data_offset), fixture.tensor_offsets),
            ]
        case Error(message=problem):
            return [Check.failed(fixture.name + ".header_only", problem)]


def prefix_refusal_cases():
    """A prefix ending inside the header, a file too short for its tensors, and tensor
    bytes that lie past the prefix are each an explicit error."""
    start = ati(Expected.BASE_HEADER, 4)
    size = len(Expected.BASE_BYTES)
    return [
        error_case("header_only.prefix_inside_metadata", Reader.read_header(prefix_of(Expected.BASE_BYTES, 40), size)),
        error_case("header_only.file_shorter_than_tensors", Reader.read_header(prefix_of(Expected.BASE_BYTES, start), start + 1)),
        error_case("header_only.bytes_past_prefix", resultAndThen(Reader.read_header(prefix_of(Expected.BASE_BYTES, start), size), lambda file: Reader.tensor_bytes(file, ats(Expected.BASE_TENSORS, 0)))),
        error_case("read_path.absent_file", Reader.read_path("/nonexistent/weights.gguf", size, 16)),
    ]


def section_cases():
    """The tensor section parsed on its own, each file's declared alignment, and the
    header read from a prefix of the file."""
    return listConcat(prefix_refusal_cases(), listConcat(foldList(fixtures(), [], lambda acc, fixture: listConcat(acc, header_only_cases(fixture))), section_with_alignment()))


def section_with_alignment():
    """The tensor section, then each fixture's declared alignment."""
    return listConcat(section_case(), foldList(fixtures(), [], lambda acc, fixture: listAppend(acc, alignment_case(fixture))))


def section_case():
    """The tensor infos read straight from where the metadata section ends."""
    match Tensors.of(Expected.BASE_BYTES, Expected.BASE_TENSOR_INFO_OFFSET, ati(Expected.BASE_HEADER, 1), False, ati(Expected.BASE_HEADER, 3), len(Expected.BASE_BYTES)):
        case Success(value=placed):
            return [
                Check.equal("tensors.of.alignment", placed.alignment, ati(Expected.BASE_HEADER, 3)),
                Check.equal("tensors.of.data_offset", placed.data_offset, ati(Expected.BASE_HEADER, 4)),
                str_list_equal("tensors.of.names", mapList(placed.tensors, lambda info: info.name), Expected.BASE_TENSORS),
                Check.ints_equal("tensors.of.sizes", mapList(placed.tensors, lambda info: info.n_bytes), Expected.BASE_TENSOR_SIZES),
                Check.ints_equal("tensors.of.offsets", mapList(placed.tensors, lambda info: info.data_offset), Expected.BASE_TENSOR_OFFSETS),
            ]
        case Error(message=problem):
            return [Check.failed("tensors.of", problem)]


def alignment_case(fixture):
    """The alignment a fixture declares agrees with the file the reader returned."""
    file = loaded(fixture)
    match Tensors.alignment_of(file.fields):
        case Success(value=alignment):
            return Check.equal(fixture.name + ".declared_alignment", alignment, ati(fixture.header, 3))
        case Error(message=problem):
            return Check.failed(fixture.name + ".declared_alignment", problem)


def record_cases():
    """The records the port publishes, built here and compared against parsed ones."""
    match Chain.first_or_error(fixtures(), "the suite has no fixtures"):
        case Success(value=base):
            return record_checks(base)
        case Error(message=problem):
            return [Check.failed("record", problem)]


def record_checks(base):
    """One case per record type, against the parsed value it describes."""
    entry = index_of(Expected.BASE_FIELDS, "u32")
    value_kind = Constants.ValueType(id=ati(Expected.VALUE_TYPE_IDS, 0), name=ats(Expected.VALUE_TYPE_NAMES, 0))
    quant_kind = Constants.QuantType(id=ati(Expected.QUANT_TYPE_IDS, 0), name=ats(Expected.QUANT_TYPE_NAMES, 0), block=ati(Expected.QUANT_TYPE_BLOCKS, 0), size=ati(Expected.QUANT_TYPE_SIZES, 0))
    value = Values.Value(kind="uint32", item_kind="", i=ati(atl(Expected.BASE_FIELD_INTS, entry), 0), f=0.0, s="", items=[])
    info = Tensors.TensorInfo(name=ats(Expected.BASE_TENSORS, 0), type=ats(Expected.BASE_TENSOR_TYPES, 0), type_id=ati(Expected.BASE_TENSOR_TYPE_IDS, 0), dims=atl(Expected.BASE_TENSOR_DIMS, 0), n_elements=ati(Expected.BASE_TENSOR_ELEMENTS, 0), n_bytes=ati(Expected.BASE_TENSOR_SIZES, 0), data_offset=ati(Expected.BASE_TENSOR_OFFSETS, 0))
    file = loaded(base)
    return [
        record_equal("record.value_type", first_value_type(), value_kind),
        record_equal("record.quant_type", first_quant_type(), quant_kind),
        record_equal("record.value", parsed_value(file, entry), value),
        record_equal("record.tensor_info", parsed_tensor(file), info),
    ]


def record_equal(tag, got, want):
    """Structural equality of two records, which the golden transcript cannot print."""
    return Check.truth(tag, structuralEquals(got, want), "the record differs from the one those figures describe")


def first_value_type():
    """The first row of the value-type table, or a stand-in."""
    match listGet(Constants.VALUE_TYPES, 0):
        case Success(value=kind):
            return kind
        case Error(message=_problem):
            return Constants.ValueType(id=0 - 1, name="")


def first_quant_type():
    """The first row of the tensor-type table, or a stand-in."""
    match listGet(Constants.QUANT_TYPES, 0):
        case Success(value=quant):
            return quant
        case Error(message=_problem):
            return Constants.QuantType(id=0 - 1, name="", block=0, size=0)


def parsed_value(file, entry):
    """The parsed metadata value a record comparison needs, or a stand-in."""
    match listGet(file.fields, entry):
        case Success(value=field):
            return field.value
        case Error(message=_problem):
            return Values.Value(kind="", item_kind="", i=0, f=0.0, s="", items=[])


def parsed_tensor(file):
    """The first parsed tensor info, or a stand-in."""
    match listGet(file.tensors, 0):
        case Success(value=info):
            return info
        case Error(message=_problem):
            return Tensors.TensorInfo(name="", type="", type_id=0, dims=[], n_elements=0, n_bytes=0, data_offset=0)


def limit_cases():
    """The widest 64-bit words, in both byte orders, and the one no int64 can hold."""
    lowest = Expected.INT64_MIN_BYTES
    return [signed_case("values.int64_min", lowest, False), signed_case("values.int64_min_swapped", listReverse(lowest), True)]


def signed_case(tag, word, is_swapped):
    """A signed 64-bit word with its top bit set decodes to the lowest int64."""
    match Values.value_at(word, 0, "int64", is_swapped):
        case Success(value=sized):
            return cell_case(tag, Reader.Field(offset=0, name=tag, value=sized.value), "int64", [Expected.INT64_MIN], [], [], [])
        case Error(message=problem):
            return Check.failed(tag, problem)


def bounds_cases():
    """Inputs that declare more than the buffer holds are errors, never short reads."""
    return [error_case("bounds.uint64_overflow", Values.value_at(Expected.UINT64_MAX_BYTES, 0, "uint64", False)),
            error_case("bounds.huge_array", Values.values_at(Expected.BASE_BYTES, ati(Expected.BASE_HEADER, 4), 1000000000000, "uint64", False)),
            error_case("bounds.short_string", Values.string_at(Expected.R_SHORT_BYTES, 24, False)),
            error_case("bounds.past_end", Values.value_at(Expected.BASE_BYTES, len(Expected.BASE_BYTES), "uint32", False))]


def index_of(names, name):
    """The index of `name` in `names`, or -1."""
    return fold(range(len(names)), 0 - 1, lambda found, i: found if found >= 0 else (i if ats(names, i) == name else found))
