# osprey: flavor=python
"""@osprey/gguf's suite: every accepted fixture read through `read`, every metadata
entry read again through the accessor its declared kind selects, every tensor's info
and raw bytes, the tables a phase reads, every quantized block decoded again against
ggml's own C decoder, and every input `gguf` 0.19.0's GGUFReader rejects, all against
values the CPython oracle computed ([PORT-TEST-ORACLE]).
"""
from gguf import Chain, Constants, Quant, Reader
from gguf_expected import Expected
from testing import Check

from gguf_tests.Support import TIGHT, ATOL, Rejected, fixtures, loaded, quant_types, str_list_equal, int_rows, float_rows, str_rows, bool_rows, ats, ati, atf, atl, cell_case, checked, equal_case, error_case
from gguf_tests import Units

REJECTED = [
    Rejected(name="magic", data=Expected.R_MAGIC_BYTES, message=Expected.R_MAGIC_MESSAGE),
    Rejected(name="version1", data=Expected.R_VERSION1_BYTES, message=Expected.R_VERSION1_MESSAGE),
    Rejected(name="version4", data=Expected.R_VERSION4_BYTES, message=Expected.R_VERSION4_MESSAGE),
    Rejected(name="duplicate_key", data=Expected.R_DUPE_KEY_BYTES, message=Expected.R_DUPE_KEY_MESSAGE),
    Rejected(name="duplicate_tensor", data=Expected.R_DUPE_TENSOR_BYTES, message=Expected.R_DUPE_TENSOR_MESSAGE),
    Rejected(name="alignment_type", data=Expected.R_ALIGN_TYPE_BYTES, message=Expected.R_ALIGN_TYPE_MESSAGE),
    Rejected(name="alignment_zero", data=Expected.R_ALIGN_ZERO_BYTES, message=Expected.R_ALIGN_ZERO_MESSAGE),
    Rejected(name="alignment_24", data=Expected.R_ALIGN_24_BYTES, message=Expected.R_ALIGN_24_MESSAGE),
    Rejected(name="field_type", data=Expected.R_FIELD_TYPE_BYTES, message=Expected.R_FIELD_TYPE_MESSAGE),
    Rejected(name="quant_type", data=Expected.R_QUANT_TYPE_BYTES, message=Expected.R_QUANT_TYPE_MESSAGE),
    Rejected(name="quant_shape", data=Expected.R_QUANT_SHAPE_BYTES, message=Expected.R_QUANT_SHAPE_MESSAGE),
    Rejected(name="truncated_kv", data=Expected.R_TRUNC_KV_BYTES, message=Expected.R_TRUNC_KV_MESSAGE),
    Rejected(name="truncated_info", data=Expected.R_TRUNC_TI_BYTES, message=Expected.R_TRUNC_TI_MESSAGE),
    Rejected(name="truncated_data", data=Expected.R_TRUNC_DATA_BYTES, message=Expected.R_TRUNC_DATA_MESSAGE),
    Rejected(name="short", data=Expected.R_SHORT_BYTES, message=Expected.R_SHORT_MESSAGE),
    Rejected(name="empty", data=Expected.R_EMPTY_BYTES, message=Expected.R_EMPTY_MESSAGE),
]


def header_cases(fixture, file):
    """The header figures, the byte order and the buffer the file was read from."""
    return [
        Check.equal(fixture.name + ".version", file.version, ati(fixture.header, 0)),
        Check.equal(fixture.name + ".tensor_count", file.tensor_count, ati(fixture.header, 1)),
        Check.equal(fixture.name + ".kv_count", file.kv_count, ati(fixture.header, 2)),
        Check.equal(fixture.name + ".alignment", file.alignment, ati(fixture.header, 3)),
        Check.equal(fixture.name + ".data_offset", file.data_offset, ati(fixture.header, 4)),
        Check.equal(fixture.name + ".byte_order", file.byte_order, fixture.byte_order),
        Check.equal(fixture.name + ".buffer", len(file.data), len(fixture.data)),
    ]


def field_cases(fixture, file):
    """Every metadata key, its offset and declared kinds, and its value."""
    lists = [
        str_list_equal(fixture.name + ".keys", Reader.field_names(file), fixture.fields),
        Check.ints_equal(fixture.name + ".field_offsets", mapList(file.fields, lambda entry: entry.offset), fixture.offsets),
        str_list_equal(fixture.name + ".field_kinds", mapList(file.fields, lambda entry: entry.value.kind), fixture.kinds),
        str_list_equal(fixture.name + ".field_item_kinds", mapList(file.fields, lambda entry: entry.value.item_kind), fixture.item_kinds),
    ]
    return listConcat(lists, toList(map(range(len(fixture.fields)), lambda i: field_case(fixture, file, i))))


def field_case(fixture, file, i):
    """One metadata entry's value, found by key the way a phase reads it."""
    tag = fixture.name + ".key." + ats(fixture.fields, i)
    match Reader.field(file, ats(fixture.fields, i)):
        case Success(value=entry):
            return cell_case(tag, entry, ats(fixture.kinds, i), atl(fixture.ints, i), atl(fixture.floats, i), atl(fixture.bools, i), atl(fixture.strings, i))
        case Error(message=problem):
            return Check.failed(tag, problem)


def tensor_cases(fixture, file):
    """Every tensor's info, the types the file uses and each tensor's raw bytes."""
    lists = [
        str_list_equal(fixture.name + ".tensors", Reader.tensor_names(file), fixture.tensors),
        str_list_equal(fixture.name + ".tensor_types", Reader.tensor_types(file), fixture.types),
        str_list_equal(fixture.name + ".distinct_types", Reader.distinct_tensor_types(file), fixture.distinct),
        Check.ints_equal(fixture.name + ".elements", mapList(file.tensors, lambda info: info.n_elements), fixture.elements),
        Check.ints_equal(fixture.name + ".sizes", mapList(file.tensors, lambda info: info.n_bytes), fixture.sizes),
        Check.ints_equal(fixture.name + ".tensor_offsets", mapList(file.tensors, lambda info: info.data_offset), fixture.tensor_offsets),
    ]
    rows = listConcat(int_rows(fixture.name + ".dims", mapList(file.tensors, lambda info: info.dims), fixture.dims),
                      int_rows(fixture.name + ".bytes", tensor_bytes(file, fixture), fixture.tensor_data))
    return listConcat(lists, listConcat(rows, tensor_index_cases(fixture, file)))


def tensor_bytes(file, fixture):
    """Each tensor's stored bytes, read by name."""
    return toList(map(range(len(fixture.tensors)), lambda i: stored_bytes(file, ats(fixture.tensors, i))))


def stored_bytes(file, name):
    """A tensor's bytes, or the empty list a failed read falls back to."""
    match Reader.tensor_bytes(file, name):
        case Success(value=value):
            return value
        case Error(message=_problem):
            return []


def tensor_index_cases(fixture, file):
    """The tensor at each index, plus the absent ones a file reports as errors."""
    per_tensor = toList(map(range(len(fixture.tensors)), lambda i: indexed_case(fixture, file, i)))
    absent = [
        error_case(fixture.name + ".absent_index", Reader.tensor(file, len(fixture.tensors))),
        error_case(fixture.name + ".negative_index", Reader.tensor(file, 0 - 1)),
        error_case(fixture.name + ".absent_tensor", Reader.tensor_bytes(file, "no-such-tensor")),
        error_case(fixture.name + ".absent_key", Reader.field(file, "no.such.key")),
    ]
    return listConcat(per_tensor, absent)


def indexed_case(fixture, file, i):
    """One tensor found by index matches the list the file publishes."""
    tag = fixture.name + ".tensor." + ats(fixture.tensors, i)
    match Reader.tensor(file, i):
        case Success(value=info):
            return Check.equal(tag, info.name, ats(fixture.tensors, i))
        case Error(message=problem):
            return Check.failed(tag, problem)


def tokenizer_cases(fixture, file):
    """The tokenizer keys the file sets, which is what Phase 5 reads them by."""
    return [str_list_equal(fixture.name + ".tokenizer_keys", Reader.tokenizer_keys(file), fixture.tokenizer_keys)]


def file_cases(fixture):
    """Every assertion one accepted file yields."""
    file = loaded(fixture)
    return listConcat(header_cases(fixture, file), listConcat(field_cases(fixture, file), listConcat(tensor_cases(fixture, file), tokenizer_cases(fixture, file))))


def reject_case(entry):
    """One input the reference reader refuses: ours must refuse it the same way."""
    tag = "reject." + entry.name
    match Reader.read(entry.data):
        case Success(value=_file):
            return [Check.failed(tag, "read accepted an input gguf refuses")]
        case Error(message=problem):
            return listConcat([Check.passed(tag)], recorded_message(tag, problem, entry.message))


def recorded_message(tag, problem, want):
    """gguf's own message, when the oracle recorded one that numpy did not write."""
    if want == "":
        return []
    else:
        return [Check.equal(tag + ".message", problem, want)]


def table_cases():
    """The value-type and quantization tables, the tokenizer key catalogue and the
    fixed header limits, all read from the package instead of retyped."""
    return [
        Check.equal("table.magic", Constants.MAGIC, Expected.MAGIC),
        Check.equal("table.version", Constants.VERSION, Expected.VERSION),
        Check.ints_equal("table.supported_versions", Constants.SUPPORTED_VERSIONS, Expected.SUPPORTED_VERSIONS),
        Check.equal("table.default_alignment", Constants.DEFAULT_ALIGNMENT, Expected.DEFAULT_ALIGNMENT),
        Check.ints_equal("table.value_type_ids", mapList(Constants.VALUE_TYPES, lambda kind: kind.id), Expected.VALUE_TYPE_IDS),
        str_list_equal("table.value_type_names", mapList(Constants.VALUE_TYPES, lambda kind: kind.name), Expected.VALUE_TYPE_NAMES),
        Check.ints_equal("table.quant_type_ids", mapList(Constants.QUANT_TYPES, lambda quant: quant.id), Expected.QUANT_TYPE_IDS),
        str_list_equal("table.quant_type_names", mapList(Constants.QUANT_TYPES, lambda quant: quant.name), Expected.QUANT_TYPE_NAMES),
        Check.ints_equal("table.quant_type_blocks", mapList(Constants.QUANT_TYPES, lambda quant: quant.block), Expected.QUANT_TYPE_BLOCKS),
        Check.ints_equal("table.quant_type_sizes", mapList(Constants.QUANT_TYPES, lambda quant: quant.size), Expected.QUANT_TYPE_SIZES),
        str_list_equal("table.tokenizer_keys", Constants.TOKENIZER_KEYS, Expected.TOKENIZER_KEYS),
    ]








def lookup_cases():
    """Every table entry found again by its wire id, in both directions."""
    kinds = toList(map(range(len(Expected.VALUE_TYPE_IDS)), lambda i: kind_lookup(i)))
    quants = toList(map(range(len(Expected.QUANT_TYPE_IDS)), lambda i: quant_lookup(i)))
    unknown = [error_case("lookup.unknown_value_type", Constants.value_type(Expected.UNKNOWN_VALUE_TYPE)),
               error_case("lookup.unknown_quant_type", Constants.quant_type(Expected.UNKNOWN_QUANT_TYPE))]
    return listConcat(listConcat(kinds, quants), unknown)


def kind_lookup(i):
    """The value type for one wire id matches the table's row."""
    return named_lookup("lookup.value_type.", ats(Expected.VALUE_TYPE_NAMES, i), Constants.value_type(ati(Expected.VALUE_TYPE_IDS, i)))


def quant_lookup(i):
    """The tensor type for one wire id matches the table's row."""
    return named_lookup("lookup.quant_type.", ats(Expected.QUANT_TYPE_NAMES, i), Constants.quant_type(ati(Expected.QUANT_TYPE_IDS, i)))


def named_lookup(prefix, want, found):
    """The entry a wire id found carries exactly the name the oracle recorded."""
    return checked(prefix + want, found, lambda tag, entry: Check.equal(tag, entry.name, want))


class Tally:
    """A walk record for `Chain.walk_checked`: the sum so far and the first problem."""
    total: int
    problem: str


def tally_step(walk, i):
    return Tally(total=walk.total + i, problem=walk.problem)


def refusing_step(walk, i):
    return Tally(total=walk.total, problem=f"step {i} refused" if i == 2 and walk.problem == "" else walk.problem)


def chain_cases():
    """`then` continues a Success and keeps an Error; the picker takes the head."""
    continued = resultAndThen(Constants.value_type(ati(Expected.VALUE_TYPE_IDS, 2)), lambda kind: Success(value=kind.name))
    return [equal_case("chain.then", continued, ats(Expected.VALUE_TYPE_NAMES, 2)),
            error_case("chain.then_error", resultAndThen(Constants.value_type(Expected.UNKNOWN_VALUE_TYPE), lambda kind: Success(value=kind.name))),
            equal_case("chain.first", Chain.first_or_error(Expected.BASE_FIELD_OFFSETS, ""), ati(Expected.BASE_FIELD_OFFSETS, 0)),
            error_case("chain.first_empty", Chain.first_or_error([], "the selection is empty")),
            equal_case("chain.walk_checked", resultAndThen(Chain.walk_checked(4, Tally(total=0, problem=""), tally_step), lambda walk: Success(value=walk.total)), 6),
            error_case("chain.walk_checked_problem", Chain.walk_checked(4, Tally(total=0, problem=""), refusing_step))]


def quant_cases():
    """Every block type decoded from its stored bytes against ggml's own output, one
    block at a time and all 64 at once, plus every input a decoder must refuse."""
    return listConcat(quant_decodes(), listConcat(quant_rejects(), quant_refusals()))


def quant_decodes():
    """The types the port decodes, which is the list the oracle recorded."""
    types = quant_types()
    listed = [str_list_equal("decode.supported", Quant.SUPPORTED, mapList(types, lambda quant: quant.name))]
    return listConcat(listed, foldList(types, [], lambda acc, case: listConcat(acc, quant_case(case))))


def quant_case(case):
    """One type's whole fixture decoded, and its first block decoded on its own."""
    tag = "decode." + case.name
    want = expected_values(case.values)
    match Quant.decode(case.name, case.data):
        case Success(value=values):
            count = intDiv(len(case.data), case.size) * case.block
            return listConcat([Check.equal(tag + ".length", len(values), count),
                               Check.close_all(tag + ".values", values, want, 0.0, 0.0)],
                              [block_case(case, want)])
        case Error(message=problem):
            return [Check.failed(tag + ".decode", problem)]


def block_case(case, want):
    """The first stored block alone decodes to that block's own values."""
    tag = "decode." + case.name + ".block"
    match Quant.decode(case.name, head_ints(case.data, case.size)):
        case Success(value=values):
            return Check.close_all(tag, values, head_floats(want, case.block), 0.0, 0.0)
        case Error(message=problem):
            return Check.failed(tag, problem)


def quant_rejects():
    """A byte count that is not a whole number of blocks, in both directions, and none."""
    return foldList(quant_types(), [], lambda acc, case: listConcat(acc, quant_reject(case)))


def quant_reject(case):
    """One type's short, long and empty inputs: the first two are Errors, none is empty."""
    tag = "decode." + case.name
    return [error_case(tag + ".short", Quant.decode(case.name, head_ints(case.data, case.size - 1))),
            error_case(tag + ".long", Quant.decode(case.name, head_ints(case.data, case.size + 1))),
            empty_case(tag + ".empty", Quant.decode(case.name, []))]


def quant_refusals():
    """Types this port does not decode, and a name no GGUF file can hold."""
    return [error_case("decode.reject.q4_0", Quant.decode("Q4_0", [])),
            error_case("decode.reject.f16", Quant.decode("F16", [0, 0])),
            error_case("decode.reject.unknown", Quant.decode("no-such-type", []))]


def empty_case(tag, result):
    """No bytes at all is zero whole blocks, so zero values."""
    match result:
        case Success(value=values):
            return Check.equal(tag, len(values), 0)
        case Error(message=problem):
            return Check.failed(tag, problem)


def expected_values(text):
    """The oracle's values for one type, one per comma, decoded to binary64 exactly.

    `Check.float_of` makes unparseable text NaN, so a broken expectation never compares
    equal to a real value.
    """
    return mapList(unwrap_or(split(text, ","), [""]), Check.float_of)


def head(xs, n, at):
    """The first `n` elements of `xs`, `at` reading each."""
    return toList(map(range(n), lambda i: at(xs, i)))


def head_ints(xs, n):
    """The first `n` integers of `xs`."""
    return head(xs, n, ati)


def head_floats(xs, n):
    """The first `n` floats of `xs`."""
    return head(xs, n, atf)


def cases():
    """Every accepted file, every rejected input, the tables and the low-level reads."""
    files = foldList(fixtures(), [], lambda acc, fixture: listConcat(acc, file_cases(fixture)))
    rejects = foldList(REJECTED, [], lambda acc, entry: listConcat(acc, reject_case(entry)))
    suites = listConcat(table_cases(), listConcat(lookup_cases(), listConcat(chain_cases(), listConcat(quant_cases(), Units.cases()))))
    return listConcat(files, listConcat(rejects, suites))


def main():
    return Check.run(cases)
