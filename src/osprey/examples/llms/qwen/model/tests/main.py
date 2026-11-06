# osprey: flavor=python
"""The Qwen3.5 example model's suite: the tool-call parser, scanner and constraint
against the values the real chat template and jsonschema produced
(oracle/tool_calls.cpython); `FixtureCases` runs the device model against every activation
and cache state transformers 5.20.2 + torch 2.14.1 computed for a tiny checkpoint
(oracle/qwen3_5.cpython), and `GgufCases` the GGUF loader against the same checkpoint
written as tiny GGUF files (oracle/qwen3_5_gguf.cpython)."""
from testing import Check
from qwen import ToolConstrain, ToolJsonScan, ToolParser, ToolSchema
from qwen_tests import FixtureCases
from qwen_tests import GgufCases
from qwen_tests.ModelCases import flatten, refusal
from qwen_tests.ToolSupport import decoded, encoded, finished, scanned, tool_list
from qwen_tests.ParserCases import chunked_cases, coercion_cases, corner_cases, parse_cases, repair_cases, validation_cases
from qwen_tests.ConstrainCases import constrain_cases, control_cases, logits_processor_cases



def main():
    return Check.run(lambda: flatten([tool_cases(), FixtureCases.model_fixture_cases(), GgufCases.gguf_cases()]))


# The schema readers the coercion and the mask are built on.
def reader_cases():
    """Every reader of the schema sub-language, over the request's own tool list."""
    tools = tool_list()
    policy = ToolSchema.parameters(ToolSchema.tool_named(tools, "run_tests"))
    mode = ToolSchema.property_schema(ToolSchema.property_schema(policy, "policy"), "mode")
    suites = ToolSchema.property_schema(policy, "suites")
    return [
        Check.equal("reader_kind", ToolSchema.kind(policy), "object"),
        Check.equal("reader_kind_matches", ToolSchema.kind_matches("integer", JsonInt(3)), True),
        Check.equal("reader_members", join(ToolSchema.property_names(policy), ","), "suites,policy"),
        Check.equal("reader_names", join(ToolSchema.property_names(ToolSchema.parameters(ToolSchema.tool_named(tools, "write_file"))), ","), "path,content"),
        Check.equal("reader_required", join(ToolSchema.required_names(policy), ","), "suites,policy"),
        Check.equal("reader_items", ToolSchema.kind(ToolSchema.items_schema(suites)), "string"),
        Check.equal("reader_extra", ToolSchema.allows_extra(policy), False),
        Check.equal("reader_enum", join(mapList(ToolSchema.enum_values(mode), lambda value: encoded(value)), ","), "\"check\",\"apply\",\"preview\""),
        Check.equal("reader_tool", ToolSchema.tool_name(ToolSchema.tool_named(tools, "find_symbol")), "find_symbol"),
        Check.equal("reader_tools", toString(len(ToolSchema.tool_list(tools))), "5"),
        Check.equal("reader_violation", ToolSchema.violation_keyword(ToolSchema.violation("enum", "mode", "not a member")), "enum"),
        Check.equal("reader_violation_path", ToolSchema.violation_path(ToolSchema.violation("enum", "mode", "not a member")), "mode"),
        Check.equal("reader_members_of", toString(len(ToolSchema.members(policy))), "2"),
        Check.equal("reader_nonarray_enum", len(ToolSchema.enum_values(JsonNull)), 0),
        Check.equal("reader_nonobject_members", len(ToolSchema.members(JsonInt(2))), 0),
        Check.equal("reader_nonstr_kind", ToolSchema.kind(decoded("{\"type\":7}")), ""),
        Check.equal("reader_unknown_kind", ToolSchema.kind_matches("unrecognised", JsonNull), False),
        Check.equal("reader_violation_without_keyword", ToolSchema.violation_keyword("plain error"), ""),
        Check.equal("reader_violation_without_path", ToolSchema.violation_path("enum@mode"), ""),
    ]


# The JSON scanner the mask is built on, driven byte at a time.
def scanner_cases():
    """One value per schema against the scanner: what a masked token can never write."""
    policy = decoded("{\"type\":\"object\",\"properties\":{\"mode\":{\"type\":\"string\",\"enum\":[\"check\",\"apply\"]},\"n\":{\"type\":\"integer\"}},\"required\":[\"mode\",\"n\"],\"additionalProperties\":false}")
    suites = decoded("{\"type\":\"array\",\"items\":{\"type\":\"string\"}}")
    nested = decoded("{\"type\":\"object\",\"properties\":{\"files\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"path\":{\"type\":\"string\"}},\"required\":[\"path\"],\"additionalProperties\":false}}},\"required\":[\"files\"],\"additionalProperties\":false}")
    return listConcat([
        Check.equal("scan_valid", scanned(policy, "{\"mode\": \"apply\", \"n\": 1}"), "complete"),
        Check.equal("scan_enum", scanned(policy, "{\"mode\": \"delete\""), "rejected"),
        Check.equal("scan_key", scanned(policy, "{\"nosuch\": 1}"), "rejected"),
        Check.equal("scan_required", scanned(policy, "{\"mode\": \"check\"}"), "rejected"),
        Check.equal("scan_array", scanned(suites, "[\"gc\", \"arc\"]"), "complete"),
        Check.equal("scan_nested", scanned(nested, "{\"files\": [{\"path\": \"a\", \"path\": \"b\"}]}"), "rejected"),
        Check.equal("scan_bad_literal", scanned(suites, "[true]"), "rejected"),
        Check.equal("scan_prefix", scanned(policy, "{\"mode\": \"ap"), "incomplete"),
    ], scanner_boundary_cases())


def scanner_boundary_cases():
    """JSON scalar, escape and object boundaries under the same byte scanner."""
    closed_object = decoded("{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}")
    null_schema = decoded("{\"type\":\"null\"}")
    return [
        Check.equal("scan_null_schema", scanned(null_schema, "null"), "complete"),
        Check.equal("scan_true_at_end", scanned(JsonNull, "true"), "complete"),
        Check.equal("scan_literal_prefix_at_end", scanned(JsonNull, "nul"), "incomplete"),
        Check.equal("scan_nested_number_at_end", scanned(JsonNull, "[1"), "incomplete"),
        Check.equal("scan_nested_literal_at_end", scanned(JsonNull, "[null"), "incomplete"),
        Check.equal("scan_any_nested", scanned(JsonNull, "[\"text\",true,false,null,{},[],[-12.50E+23,1e-12,2e2]]"), "complete"),
        Check.equal("scan_escape_valid", scanned(JsonNull, "\"\\u0041\""), "complete"),
        Check.equal("scan_escape_invalid", scanned(JsonNull, "\"\\u00x1\""), "rejected"),
        Check.equal("scan_control_key", scanned(JsonNull, "{\"\n\":1}"), "rejected"),
        Check.equal("scan_repeated_key", scanned(JsonNull, "{\"a\":1,\"a\":2}"), "rejected"),
        Check.equal("scan_undeclared_key", scanned(closed_object, "{\"x\":1}"), "rejected"),
        Check.equal("scan_fraction_missing", scanned(JsonNull, "[1.e2]"), "rejected"),
        Check.equal("scan_exponent_missing", scanned(JsonNull, "[1e]"), "rejected"),
        Check.equal("scan_exponent_sign_missing", scanned(JsonNull, "[1e-]"), "rejected"),
        Check.equal("scan_exponent_open", scanned(JsonNull, "[1e+"), "incomplete"),
        Check.equal("scan_empty_stack_finished", finished([]), True),
        Check.equal("scan_empty_stack_no_consumption", empty_scan_unused(), True),
    ]


def empty_scan_unused():
    match ToolJsonScan.feed([], 65):
        case Success(value=step):
            return len(step.stack) == 0 and not step.consumed
        case Error():
            return False


def tool_cases():
    return listConcat(corner_cases(), listConcat(reader_cases(), listConcat(scanner_cases(), listConcat(validation_cases(), listConcat(coercion_cases(), listConcat(parse_cases(), listConcat(chunked_cases(), listConcat(repair_cases(), listConcat(constrain_cases(), listConcat(control_cases(), logits_processor_cases()))))))))))
