# osprey: flavor=python
"""@osprey/transformers's suite: every processor, warper, sampler and stopping
criterion against values the real transformers 5.20.2 + torch 2.14.1 (float32)
computed (src/osprey/packages/transformers/oracle/transformers.cpython).

The shared writer (src/osprey/packages/testing/oracle/expected_module.cpython) writes every finite float
constant as a float literal; a constant holding a `-inf` or NaN is written with
all its floats as repr strings, decoded here with `Check.float_of`."""
from testing import Check
from transformers_expected import Expected
from transformers_tools_expected import ExpectedTools
from transformers import Generation, Lists, LogitsProcess, Qwen3_5Cache, Qwen3_5Config, Qwen3_5Hf, Qwen3_5Model, StoppingCriteria, ToolConstrain, ToolJsonScan, ToolParser, ToolSchema
from transformers.Math import argmax, log_softmax, max_f, max_i, min_f, min_i, multinomial, neg_inf, softmax

ARITH = 1e-12
EXP = 1e-9
ATOL = 1e-12


def f32_of(x):
    """One oracle value — a finite float literal or a repr string — as float32."""
    return toFloat32(Check.float_of(toString(x)))


def floats_of(xs):
    """A logits row the oracle wrote (strings for -inf/NaN, floats otherwise) as float32."""
    return mapList(xs, f32_of)


def decode_rows(xs):
    """Every row of a `list[list]` constant, decoded into float32."""
    return toList(map(range(len(xs)), lambda i: floats_of(unwrap_or(listGet(xs, i), []))))


def close_list(name, got, want, rtol, atol):
    """Float32 lists compared by exact widening to float64."""
    return Check.close_all(name, mapList(got, toFloat), mapList(want, toFloat), rtol, atol)



def applied(processor, ids, scores):
    """One `Processor` record over one row, as generate applies it."""
    return LogitsProcess.apply(processor, ids, scores)


def stops(criterion: StoppingCriteria.Criterion, tokens):
    """One `Criterion` record's verdict on a token history."""
    return StoppingCriteria.stopped(criterion, tokens)


def row_case(name, build, i, ids_rows, scores, expected, rtol, atol):
    ids = unwrap_or(listGet(ids_rows, i), [])
    got = applied(build(i), ids, unwrap_or(listGet(scores, i), []))
    want = unwrap_or(listGet(expected, i), [])
    return close_list(f"{name}[{i}]", got, want, rtol, atol)


def rows(name, build, ids_rows, scores, expected, rtol, atol):
    return toList(map(range(len(expected)), lambda i: row_case(name, build, i, ids_rows, scores, expected, rtol, atol)))


def math_cases():
    soft = toList(map(range(len(Expected.SOFTMAX_OUT)), lambda i: close_list(f"softmax[{i}]", softmax(floats_of(unwrap_or(listGet(Expected.SOFTMAX_IN, i), []))), floats_of(unwrap_or(listGet(Expected.SOFTMAX_OUT, i), [])), EXP, ATOL)))
    logsoft = toList(map(range(len(Expected.LOGSOFTMAX_OUT)), lambda i: close_list(f"log_softmax[{i}]", log_softmax(floats_of(unwrap_or(listGet(Expected.LOGSOFTMAX_IN, i), []))), floats_of(unwrap_or(listGet(Expected.LOGSOFTMAX_OUT, i), [])), EXP, ATOL)))
    amax = toList(map(range(len(Expected.ARGMAX_OUT)), lambda i: Check.equal(f"argmax[{i}]", argmax(floats_of(unwrap_or(listGet(Expected.ARGMAX_IN, i), []))), unwrap_or(listGet(Expected.ARGMAX_OUT, i), 0))))
    multi = toList(map(range(len(Expected.MULTI_OUT)), lambda i: Check.equal(f"multinomial[{i}]", multinomial(floats_of(unwrap_or(listGet(Expected.MULTI_PROBS, i), [])), toFloat32(unwrap_or(listGet(Expected.MULTI_U, i), 0.0))), unwrap_or(listGet(Expected.MULTI_OUT, i), 0))))
    empty = [close_list("softmax_empty", softmax([]), [], EXP, ATOL), close_list("log_softmax_empty", log_softmax([]), [], EXP, ATOL)]
    return listConcat(soft, listConcat(logsoft, listConcat(amax, listConcat(multi, empty))))


def build_temp(i):
    return LogitsProcess.TemperatureLogitsWarper(toFloat32(unwrap_or(listGet(Expected.TEMP_TEMP, i), 1.0)))


def build_topk(i):
    return LogitsProcess.TopKLogitsWarper(unwrap_or(listGet(Expected.TOPK_K, i), 1), neg_inf(), unwrap_or(listGet(Expected.TOPK_MIN, i), 1))


def build_topp(i):
    return LogitsProcess.TopPLogitsWarper(toFloat32(unwrap_or(listGet(Expected.TOPP_P, i), 1.0)), neg_inf(), unwrap_or(listGet(Expected.TOPP_MIN, i), 1))


def build_minp(i):
    return LogitsProcess.MinPLogitsWarper(toFloat32(unwrap_or(listGet(Expected.MINP_P, i), 0.0)), neg_inf(), unwrap_or(listGet(Expected.MINP_MIN, i), 1))


def build_typ(i):
    return LogitsProcess.TypicalLogitsWarper(toFloat32(unwrap_or(listGet(Expected.TYP_MASS, i), 0.9)), neg_inf(), unwrap_or(listGet(Expected.TYP_MIN, i), 1))


def build_eps(i):
    return LogitsProcess.EpsilonLogitsWarper(toFloat32(unwrap_or(listGet(Expected.EPS_EPS, i), 0.1)), neg_inf(), unwrap_or(listGet(Expected.EPS_MIN, i), 1))


def build_eta(i):
    return LogitsProcess.EtaLogitsWarper(toFloat32(unwrap_or(listGet(Expected.ETA_EPS, i), 0.05)), neg_inf(), unwrap_or(listGet(Expected.ETA_MIN, i), 1))


def build_repp(i):
    return LogitsProcess.RepetitionPenaltyLogitsProcessor(toFloat32(unwrap_or(listGet(Expected.REPP_PEN, i), 1.0)), unwrap_or(listGet(Expected.REPP_IGNORE, i), 0))


def build_nrn(i):
    return LogitsProcess.NoRepeatNGramLogitsProcessor(unwrap_or(listGet(Expected.NRN_N, i), 2))


def build_minl(i):
    return LogitsProcess.MinLengthLogitsProcessor(unwrap_or(listGet(Expected.MINL_LEN, i), 1), unwrap_or(listGet(Expected.MINL_EOS, i), []))


def build_mnt(i):
    return LogitsProcess.MinNewTokensLengthLogitsProcessor(unwrap_or(listGet(Expected.MNT_SKIP, i), 0), unwrap_or(listGet(Expected.MNT_NEW, i), 1), unwrap_or(listGet(Expected.MNT_EOS, i), []))


def build_fbos(i):
    return LogitsProcess.ForcedBOSTokenLogitsProcessor(unwrap_or(listGet(Expected.FBOS_BOS, i), 0))


def build_feos(i):
    return LogitsProcess.ForcedEOSTokenLogitsProcessor(unwrap_or(listGet(Expected.FEOS_MAX, i), 1), unwrap_or(listGet(Expected.FEOS_EOS, i), []))


def build_supp(i):
    return LogitsProcess.SuppressTokensLogitsProcessor(unwrap_or(listGet(Expected.SUPP_TOKENS, i), []))


def build_bad(i):
    return LogitsProcess.NoBadWordsLogitsProcessor(unwrap_or(listGet(Expected.BAD_BADWORDS, i), []), [5])


def processor_cases():
    families = [
        rows("temperature", build_temp, Expected.TEMP_IDS, decode_rows(Expected.TEMP_SCORES), decode_rows(Expected.TEMP_EXPECTED), ARITH, ATOL),
        rows("top_k", build_topk, Expected.TOPK_IDS, decode_rows(Expected.TOPK_SCORES), decode_rows(Expected.TOPK_EXPECTED), ARITH, ATOL),
        rows("top_p", build_topp, Expected.TOPP_IDS, decode_rows(Expected.TOPP_SCORES), decode_rows(Expected.TOPP_EXPECTED), EXP, ATOL),
        rows("min_p", build_minp, Expected.MINP_IDS, decode_rows(Expected.MINP_SCORES), decode_rows(Expected.MINP_EXPECTED), EXP, ATOL),
        rows("typical", build_typ, Expected.TYP_IDS, decode_rows(Expected.TYP_SCORES), decode_rows(Expected.TYP_EXPECTED), EXP, ATOL),
        rows("epsilon", build_eps, Expected.EPS_IDS, decode_rows(Expected.EPS_SCORES), decode_rows(Expected.EPS_EXPECTED), EXP, ATOL),
        rows("eta", build_eta, Expected.ETA_IDS, decode_rows(Expected.ETA_SCORES), decode_rows(Expected.ETA_EXPECTED), EXP, ATOL),
        rows("repetition_penalty", build_repp, Expected.REPP_IDS, decode_rows(Expected.REPP_SCORES), decode_rows(Expected.REPP_EXPECTED), ARITH, ATOL),
        rows("no_repeat_ngram", build_nrn, Expected.NRN_IDS, decode_rows(Expected.NRN_SCORES), decode_rows(Expected.NRN_EXPECTED), ARITH, ATOL),
        rows("min_length", build_minl, Expected.MINL_IDS, decode_rows(Expected.MINL_SCORES), decode_rows(Expected.MINL_EXPECTED), ARITH, ATOL),
        rows("min_new_tokens", build_mnt, Expected.MNT_IDS, decode_rows(Expected.MNT_SCORES), decode_rows(Expected.MNT_EXPECTED), ARITH, ATOL),
        rows("forced_bos", build_fbos, Expected.FBOS_IDS, decode_rows(Expected.FBOS_SCORES), decode_rows(Expected.FBOS_EXPECTED), ARITH, ATOL),
        rows("forced_eos", build_feos, Expected.FEOS_IDS, decode_rows(Expected.FEOS_SCORES), decode_rows(Expected.FEOS_EXPECTED), ARITH, ATOL),
        rows("suppress_tokens", build_supp, Expected.SUPP_IDS, decode_rows(Expected.SUPP_SCORES), decode_rows(Expected.SUPP_EXPECTED), ARITH, ATOL),
        rows("bad_words", build_bad, Expected.BAD_IDS, decode_rows(Expected.BAD_SCORES), decode_rows(Expected.BAD_EXPECTED), ARITH, ATOL),
    ]
    return foldList(families, [], lambda acc, x: listConcat(acc, x))


def composition_cases():
    chain = [
        LogitsProcess.TemperatureLogitsWarper(2.0),
        LogitsProcess.TopKLogitsWarper(3, neg_inf(), 1),
        LogitsProcess.TopPLogitsWarper(0.9, neg_inf(), 1),
    ]
    a = [LogitsProcess.TopKLogitsWarper(2, neg_inf(), 1), LogitsProcess.TemperatureLogitsWarper(2.0)]
    b = [LogitsProcess.TemperatureLogitsWarper(2.0), LogitsProcess.TopKLogitsWarper(2, neg_inf(), 1)]
    chain_cases = toList(map(range(len(Expected.COMPOSE_EXPECTED)), lambda i: close_list(f"compose[{i}]", LogitsProcess.apply_list(chain, [], unwrap_or(listGet(decode_rows(Expected.COMPOSE_SCORES), i), [])), floats_of(unwrap_or(listGet(Expected.COMPOSE_EXPECTED, i), [])), EXP, ATOL)))
    order_a = toList(map(range(len(Expected.ORDER_A_EXPECTED)), lambda i: close_list(f"order_a[{i}]", LogitsProcess.apply_list(a, [], unwrap_or(listGet(decode_rows(Expected.ORDER_SCORES), i), [])), floats_of(unwrap_or(listGet(Expected.ORDER_A_EXPECTED, i), [])), EXP, ATOL)))
    order_b = toList(map(range(len(Expected.ORDER_B_EXPECTED)), lambda i: close_list(f"order_b[{i}]", LogitsProcess.apply_list(b, [], unwrap_or(listGet(decode_rows(Expected.ORDER_SCORES), i), [])), floats_of(unwrap_or(listGet(Expected.ORDER_B_EXPECTED, i), [])), EXP, ATOL)))
    return listConcat(chain_cases, listConcat(order_a, order_b))


def stopping_cases():
    ml = toList(map(range(len(Expected.STOP_MAXLEN_DONE)), lambda i: Check.equal(f"max_length[{i}]", stops(StoppingCriteria.MaxLengthCriteria(unwrap_or(listGet(Expected.STOP_MAXLEN_MAX, i), 0)), unwrap_or(listGet(Expected.STOP_IDS, i % len(Expected.STOP_IDS)), [])), unwrap_or(listGet(Expected.STOP_MAXLEN_DONE, i), False))))
    eos = toList(map(range(len(Expected.STOP_EOS_DONE)), lambda i: Check.equal(f"eos[{i}]", stops(StoppingCriteria.EosTokenCriteria(unwrap_or(listGet(Expected.STOP_EOS_EOS, i), [])), unwrap_or(listGet(Expected.STOP_IDS, i % len(Expected.STOP_IDS)), [])), unwrap_or(listGet(Expected.STOP_EOS_DONE, i), False))))
    ss = toList(map(range(len(Expected.STOPSTR_DONE)), lambda i: Check.equal(f"stop_string[{i}]", StoppingCriteria.ends_with_any(unwrap_or(listGet(Expected.STOPSTR_TEXTS, i), ""), unwrap_or(listGet(Expected.STOPSTR_STRS, i), [])), unwrap_or(listGet(Expected.STOPSTR_DONE, i), False))))
    return listConcat(ml, listConcat(eos, ss))


def helper_cases():
    """`Lists.contains_int`/`last_n` against Python's `in` and tail slice, and
    `stopped_any` against a StoppingCriteriaList of both criteria."""
    contains = toList(map(range(len(Expected.HELP_CONTAINS_OUT)), lambda i: Check.equal(f"contains_int[{i}]", Lists.contains_int(unwrap_or(listGet(Expected.HELP_CONTAINS_XS, i), []), unwrap_or(listGet(Expected.HELP_CONTAINS_X, i), 0)), unwrap_or(listGet(Expected.HELP_CONTAINS_OUT, i), False))))
    tails = toList(map(range(len(Expected.HELP_LASTN_OUT)), lambda i: Check.ints_equal(f"last_n[{i}]", Lists.last_n(unwrap_or(listGet(Expected.HELP_LASTN_XS, i), []), unwrap_or(listGet(Expected.HELP_LASTN_N, i), 0)), unwrap_or(listGet(Expected.HELP_LASTN_OUT, i), []))))
    stoppers = [StoppingCriteria.MaxLengthCriteria(5), StoppingCriteria.EosTokenCriteria([4])]
    any_done = toList(map(range(len(Expected.STOPANY_DONE)), lambda i: Check.equal(f"stopped_any[{i}]", StoppingCriteria.stopped_any(stoppers, unwrap_or(listGet(Expected.STOP_IDS, i), [])), unwrap_or(listGet(Expected.STOPANY_DONE, i), False))))
    extrema = [
        Check.equal("min_i", min_i(3, 7), 3),
        Check.equal("max_i", max_i(3, 7), 7),
        Check.equal("min_f", min_f(0.5, -2.0), -2.0),
        Check.equal("max_f", max_f(0.5, -2.0), 0.5),
    ]
    return listConcat(contains, listConcat(tails, listConcat(any_done, extrema)))


def model_logits(tokens):
    idx = min_i(len(tokens) - len(Expected.GEN_PROMPT), len(Expected.GEN_STEP_LOGITS) - 1)
    return floats_of(unwrap_or(listGet(Expected.GEN_STEP_LOGITS, idx), []))


def generate_case():
    processors = [
        LogitsProcess.MinLengthLogitsProcessor(3, Expected.GEN_EOS),
        LogitsProcess.SuppressTokensLogitsProcessor([1]),
    ]
    stoppers = [
        StoppingCriteria.MaxLengthCriteria(7),
        StoppingCriteria.EosTokenCriteria(Expected.GEN_EOS),
    ]
    match Generation.generate(model_logits, Expected.GEN_PROMPT, processors, stoppers, argmax, 5):
        case Success(value=got):
            return Check.ints_equal("generate", got, Expected.GEN_EXPECTED)
        case Error(message=message):
            return Check.failed("generate", message)


def broken_logits(tokens):
    """A model whose second row carries an infinity and a NaN."""
    return [toFloat32(0.5), toFloat32(1.0), toFloat32(2.5)] if len(tokens) == len(Expected.GEN_PROMPT) else [toFloat32(0.5), float32FromBits(toInt32(2139095040)), toFloat32(2.5), float32FromBits(toInt32(2143289344)), toFloat32(3.0)]


def nonfinite_case():
    """[LLM-ARITH-IEEE] A non-finite logits row ends the request with an Error naming
    its step; torch's own argmax would have picked the infinity and its multinomial
    raised, which the oracle recorded."""
    match Generation.generate(broken_logits, Expected.GEN_PROMPT, [], [StoppingCriteria.MaxLengthCriteria(7)], argmax, 5):
        case Success(value=got):
            return Check.failed("non-finite logits", Check.ints_text(got))
        case Error(message=message):
            return Check.truth("non-finite logits", message == "generate: step 2: logits are not finite" and Expected.GEN_NONFINITE_MULTINOMIAL == "RuntimeError", message + " (torch argmax picked index " + toString(Expected.GEN_NONFINITE_ARGMAX) + ")")


def all_cases():
    return listConcat(math_cases(), listConcat(processor_cases(), listConcat(composition_cases(), listConcat(stopping_cases(), listConcat(helper_cases(), listConcat([generate_case(), nonfinite_case()], listConcat(tool_cases(), model_fixture_cases())))))))


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
# --- tool calls: [LLM-TOOL-PARSE], [LLM-TOOL-CONSTRAIN], [LLM-TOOL-REPAIR] ---

def decoded(text):
    return unwrap_or(jsonDecode(text), JsonNull)


def text_of(document, name):
    return unwrap_or(jsonText(unwrap_or(jsonField(document, name), JsonNull)), "")


def flag_of(document, name):
    return unwrap_or(jsonBool(unwrap_or(jsonField(document, name), JsonNull)), False)


def array_of(value):
    match value:
        case JsonArray(items):
            return items
        case _:
            return []


def texts_of(value):
    return mapList(array_of(value), lambda item: unwrap_or(jsonText(item), ""))


def ints_of(value):
    return mapList(array_of(value), lambda item: unwrap_or(jsonInt(item), 0))


def tool_list():
    return decoded(ExpectedTools.TOOLS)


def records_of(constant):
    return mapList(constant, decoded)


def indices_of(values):
    return toList(range(len(values)))


def record_at(records, i):
    return unwrap_or(listGet(records, i), JsonNull)


def verdict_text(verdict):
    """A schema verdict as `keyword/path`, or "" when it held."""
    match verdict:
        case Success(_accepted):
            return ""
        case Error(message=message):
            return f"{ToolSchema.violation_keyword(message)}/{ToolSchema.violation_path(message)}"


def want_text(record):
    """A fixture record's expected verdict: "" when valid, else `keyword/path`."""
    return "" if flag_of(record, "ok") else f"{text_of(record, 'keyword')}/{text_of(record, 'path')}"


def encoded(value):
    return unwrap_or(jsonEncode(value), "?")


def flatten(lists):
    """One list of every outcome in a list of outcome lists."""
    return foldList(lists, [], lambda acc, outcomes: listConcat(acc, outcomes))


# Schema validation against jsonschema's own verdicts.

def validation_cases():
    """Every mutation of a real argument, judged by the real `jsonschema`."""
    tools = tool_list()
    records = records_of(ExpectedTools.VALIDATE)
    return listConcat([Check.equal("validate_count", len(records), 280)], mapList(indices_of(records), lambda i: validation_case(tools, record_at(records, i), i)))


def validation_case(tools, record, i):
    schema = ToolSchema.parameters(ToolSchema.tool_named(tools, text_of(record, "tool")))
    return Check.equal(f"validate[{i}]", verdict_text(ToolSchema.validate(schema, decoded(text_of(record, "value")))), want_text(record))


# Parameter coercion against the template's own parameter texts.

def coercion_cases():
    """Every parameter text the template wrote, plus the boundary texts a model emits."""
    records = records_of(ExpectedTools.COERCE)
    return listConcat([Check.equal("coerce_count", len(records), 130)], mapList(indices_of(records), lambda i: coercion_case(record_at(records, i), i)))


def coercion_case(record, i):
    return Check.equal(f"coerce[{i}]", coercion_text(record), coercion_want(record))


def coercion_text(record):
    match ToolSchema.coerce(decoded(text_of(record, "schema")), text_of(record, "text")):
        case Success(value=value):
            return encoded(value)
        case Error(message=message):
            return f"{ToolSchema.violation_keyword(message)}/{ToolSchema.violation_path(message)}"


def coercion_want(record):
    return text_of(record, "want") if flag_of(record, "ok") else want_text(record)


# Parsing the template's own turns, whole and in chunks.

def call_fingerprint(call):
    return f"{call.name}|{encoded(call.arguments)}|{call.error}"


def turn_fingerprint(step):
    """One string standing for a whole parsed turn: its deltas and its calls."""
    return f"{step.text}|{step.think}|" + join(mapList(step.calls, call_fingerprint), ";")


def want_fingerprint(record):
    return f"{text_of(record, 'text')}|{text_of(record, 'think')}|{text_of(record, 'name')}|{text_of(record, 'args')}|"


def parsed_turn(tools, wire):
    match ToolParser.parse(tools, wire):
        case Success(value=step):
            return step
        case Error(message=message):
            return ToolParser.Step(state=ToolParser.start(tools), text=message, think="", calls=[])


def parse_cases():
    """Every Phase 0 turn, parsed whole."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return listConcat([Check.equal("case_count", len(records), 200)], mapList(indices_of(records), lambda i: parse_case(tools, record_at(records, i), i)))


def parse_case(tools, record, i):
    return Check.equal(f"parse[{i}]", turn_fingerprint(parsed_turn(tools, text_of(record, "wire"))), want_fingerprint(record))


def chunked_cases():
    """The same turns with chunks split at 1, 2, 5 and 11 bytes: any split, same turn."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return flatten(mapList([1, 2, 5, 11], lambda size: mapList(indices_of(records), lambda i: chunked_case(tools, record_at(records, i), i, size))))


def chunked_case(tools, record, i, size):
    fed = chunked_parse(tools, ToolParser.start(tools), text_of(record, "wire"), size, "", "", [])
    return Check.equal(f"chunked[{size}][{i}]", turn_fingerprint(fed), want_fingerprint(record))


def chunked_parse(tools, state, rest, size, text, think, calls):
    """Feed `rest` in `size`-byte chunks, accumulating the turn's deltas and calls."""
    if len(rest) == 0:
        return flushed(ToolParser.Step(state=state, text=text, think=think, calls=calls), ToolParser.finish(state))
    else:
        return carried(tools, rest, size, fed_once(state, take(rest, size), text, think, calls))


def fed_once(state, chunk, text, think, calls):
    match ToolParser.feed(state, chunk):
        case Success(value=step):
            return ToolParser.Step(state=step.state, text=text + step.text, think=think + step.think, calls=listConcat(calls, step.calls))
        case Error(message=message):
            return ToolParser.Step(state=state, text=f"error {message}", think=think, calls=calls)


def carried(tools, rest, size, so_far):
    return chunked_parse(tools, so_far.state, drop(rest, size), size, so_far.text, so_far.think, so_far.calls)


def flushed(so_far, finishing):
    match finishing:
        case Success(value=last):
            return ToolParser.Step(state=last.state, text=so_far.text + last.text, think=so_far.think + last.think, calls=listConcat(so_far.calls, last.calls))
        case Error(message=message):
            return ToolParser.Step(state=so_far.state, text=f"error {message}", think=so_far.think, calls=so_far.calls)


# A schema-invalid call comes back as a tool error the harness can recover from.

def repair_cases():
    """Calls the template would never write, each rejected by `jsonschema`."""
    tools = tool_list()
    records = records_of(ExpectedTools.REPAIR)
    return listConcat(flatten(mapList(indices_of(records), lambda i: repair_case(tools, record_at(records, i), i))), truncated_cases(tools, records))


def repair_case(tools, record, i):
    step = parsed_turn(tools, text_of(record, "wire"))
    call = first_call(step)
    want_keyword = text_of(record, "keyword")
    kind = "unknown_function" if want_keyword == "unknown_function" else "schema"
    return [
        Check.equal(f"repair_calls[{i}]", toString(len(step.calls)), "1"),
        Check.equal(f"repair_kind[{i}]", error_field(call, "kind"), kind),
        Check.equal(f"repair_keyword[{i}]", error_field(call, "keyword"), "" if kind == "unknown_function" else want_keyword),
        Check.equal(f"repair_arguments[{i}]", encoded(call.arguments), "null"),
        Check.equal(f"repair_recoverable[{i}]", error_field(call, "recoverable"), "true"),
    ]


def first_call(step):
    return unwrap_or(listGet(step.calls, 0), ToolParser.Call(name="", arguments=JsonNull, error=""))


def error_field(call, name):
    """One field of a call's tool error, or "" when the call carries none."""
    match ToolParser.error_field(call.error, name):
        case Success(value=field):
            return field
        case Error(_rejected):
            return ""


def truncated_cases(tools, records):
    """A turn that stops inside `<tool_call>` is an error, never a partial call."""
    wires = mapList(records, lambda record: text_of(record, "wire"))
    return mapList(indices_of(wires), lambda i: truncated_case(tools, unwrap_or(listGet(wires, i), ""), i))


def truncated_case(tools, wire, i):
    return Check.equal(f"truncated[{i}]", truncated_text(tools, take(wire, len(wire) - 5)), "truncated/call")


def truncated_text(tools, wire):
    match ToolParser.parse(tools, wire):
        case Success(value=step):
            return f"parsed {toString(len(step.calls))}"
        case Error(message=message):
            return f"{ToolSchema.violation_keyword(message)}/{ToolSchema.violation_path(message)}"


# Masked sampling: a schema-invalid call is unsamplable.

def constrain_cases():
    """Every Phase 0 case, sampled twice: towards its own tokens, and adversarially."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return flatten(mapList(indices_of(records), lambda i: constrain_case(tools, record_at(records, i), i)))


def constrain_case(tools, record, i):
    tokens = texts_of(unwrap_or(jsonField(record, "tokens"), JsonNull))
    constraint = ToolConstrain.Constrain(tools, tokens, toFloat32(0.0) - toFloat32(exp(1000.0)))
    started = spelled(tokens, ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull)))
    return [
        Check.equal(f"target[{i}]", walked(tools, constraint, record, started), want_fingerprint(record)),
        Check.equal(f"adversarial[{i}]", sampled(tools, constraint, record, started), ""),
    ]


def spelled(tokens, ids):
    return join(mapList(ids, lambda id: unwrap_or(listGet(tokens, id), "")), "")


def machine_after(tools, text):
    match ToolConstrain.advance(ToolConstrain.start(tools), tools, text):
        case Success(value=machine):
            return machine
        case Error(_rejected):
            return ToolConstrain.start(tools)


def walked(tools, constraint, record, text):
    """Greedy towards the fixture's own tokens: the mask has to allow every one."""
    return walk(tools, constraint, record, machine_after(tools, text), ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), 0, text)


def walk(tools, constraint, record, machine, want, step, text):
    if ToolConstrain.complete(machine):
        return turn_fingerprint(parsed_turn(tools, text))
    elif step >= len(want):
        return f"stalled at {machine.stage} after {toString(step)} tokens"
    else:
        return walked_step(tools, constraint, record, machine, want, step, text)


def walked_step(tools, constraint, record, machine, want, step, text):
    token = unwrap_or(listGet(constraint.vocabulary, unwrap_or(listGet(want, step), 0)), "")
    match ToolConstrain.advance(machine, tools, token):
        case Success(value=advanced):
            return walk(tools, constraint, record, advanced, want, step + 1, text + token)
        case Error(message=message):
            return f"rejected {unwrap_or(jsonQuote(token), token)} at {toString(step)}: {message}"


def sampled(tools, constraint, record, text):
    """Seeded adversarial sampling over the corpus token, a seeded draw and every closing
    token: a forbidden candidate is scored highest, so an unmasked row corrupts the
    format at once, and the mask alone keeps the call schema-valid. Returns "" when the
    turn is one schema-valid call."""
    return sample(tools, constraint, record, ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull)), text, 0)


def sample(tools, constraint, record, ids, text, step):
    machine = machine_after(tools, text)
    if ToolConstrain.complete(machine):
        return sampled_verdict(tools, parsed_turn(tools, text))
    elif step > 200:
        return f"never completed after {toString(step)} at {machine.stage} depth={toString(len(machine.stack))} inner={inner_stage(machine)}"
    else:
        return sampled_step(tools, constraint, record, ids, text, step, machine)


def sampled_step(tools, constraint, record, ids, text, step, machine):
    """One adversarial step: a seeded draw is sampled whenever the mask admits it, so a
    corrupting token is what the run takes by default; when the mask forbids it, the
    corpus token keeps the run on its schema-valid path."""
    drawn = seeded_id(constraint, step, 0)
    corpus = unwrap_or(listGet(ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), step), 0 - 1)
    chosen = choose(constraint, machine, drawn, corpus)
    if chosen < 0:
        return f"stalled at {toString(step)} in {machine.stage} inner={inner_stage(machine)} key={inner_text(machine)} seen={join(machine.seen, ',')}"
    else:
        return sample(tools, constraint, record, listAppend(ids, chosen), text + token_at(constraint, chosen), step + 1)


def choose(constraint, machine, drawn, corpus):
    """The seeded draw when the mask admits it and it moves the grammar on, else the corpus
    token when the mask admits that, else whatever moves the grammar on. A draw the mask
    rejects is where the constraint bites; an admitted draw the run keeps."""
    if progresses_with(constraint, machine, drawn):
        return drawn
    elif allowed_from(constraint, machine, corpus):
        return corpus
    else:
        return first_progressing(constraint, machine, 0, 0 - 1)


def progresses_with(constraint, machine, id):
    if id < 0:
        return False
    else:
        return moved_on(constraint, machine, token_at(constraint, id))


def moved_on(constraint, machine, token):
    match ToolConstrain.advance(machine, constraint.tools, token):
        case Success(value=advanced):
            return progresses(machine, advanced)
        case Error(_rejected):
            return False


def seeded_id(constraint, step, k):
    return (step * 7919 + k * 104729) % len(constraint.vocabulary)


def first_progressing(constraint, machine, k, found):
    """The first allowed token that moves the grammar on: a new stage or a finished JSON
    value. It keeps a seeded source from idling inside a verbatim string argument."""
    if k >= len(constraint.vocabulary):
        return found if found >= 0 else first_allowed(constraint, machine, 0)
    else:
        return progressed(constraint, machine, k, found)


def progressed(constraint, machine, k, found):
    match ToolConstrain.advance(machine, constraint.tools, token_at(constraint, k)):
        case Success(value=advanced):
            if found < 0 and moves(machine, advanced):
                return k
            else:
                return first_progressing(constraint, machine, k + 1, found)
        case Error(_rejected):
            return first_progressing(constraint, machine, k + 1, found)


def progresses(machine, advanced):
    """Whether one token moved the grammar to a new stage, finished a JSON value or
    advanced a literal: what a draw has to do to be worth taking."""
    return advanced.stage != machine.stage or advanced.mode != machine.mode or len(advanced.stack) < len(machine.stack) or advanced.progress != machine.progress


def moves(machine, advanced):
    """Whether one token moved at all, including a step inside a JSON value, so a stuck run
    always has a deterministic token to take. Text growth alone is not a move: at a closed
    stage the first allowed token already continues the name or member."""
    return progresses(machine, advanced) or top_stage(advanced) != top_stage(machine)


def top_stage(machine):
    return foldList(machine.stack, "", lambda _acc, frame: frame.stage)


def inner_text(machine):
    """The key or literal text the JSON scan has accumulated: what a stall names."""
    return foldList(machine.stack, "", lambda _acc, frame: frame.text)


def inner_stage(machine):
    """The JSON scan's own stage at the top of its stack: what a failure message names."""
    return foldList(machine.stack, "", lambda _acc, frame: frame.stage)


def first_allowed(constraint, machine, k):
    if k >= len(constraint.vocabulary):
        return 0 - 1
    elif allowed_from(constraint, machine, k):
        return k
    else:
        return first_allowed(constraint, machine, k + 1)


def allowed_from(constraint, machine, id):
    if id < 0:
        return False
    else:
        return accepts(constraint, machine, token_at(constraint, id))


def accepts(constraint, machine, token):
    match ToolConstrain.advance(machine, constraint.tools, token):
        case Success(_advanced):
            return True
        case Error(_rejected):
            return False


def token_at(constraint, i):
    return unwrap_or(listGet(constraint.vocabulary, i), "")


def sampled_verdict(tools, step):
    """"" when the sampled turn is one schema-valid call, else what is wrong with it."""
    if len(step.calls) != 1:
        return f"{toString(len(step.calls))} calls ({step.text})"
    else:
        return call_verdict(tools, first_call(step))


def call_verdict(tools, call):
    if call.error != "":
        return f"tool error {call.error}"
    else:
        return verdict_text(ToolSchema.validate(ToolSchema.parameters(ToolSchema.tool_named(tools, call.name)), call.arguments))


def control_cases():
    """Clean controls: without the mask the same adversarial row corrupts the format."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return mapList([0, 1, 2, 7], lambda i: control_case(tools, record_at(records, i), i))


def control_case(tools, record, i):
    tokens = texts_of(unwrap_or(jsonField(record, "tokens"), JsonNull))
    started = spelled(tokens, ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull)))
    return Check.equal(f"control_unmasked[{i}]", unmasked(tools, constraint_of(tools, tokens), started, 0), "corrupted")


def constraint_of(tools, tokens):
    return ToolConstrain.Constrain(tools, tokens, toFloat32(0.0) - toFloat32(exp(1000.0)))


def unmasked(tools, constraint, text, step):
    """Sample an adversarial row with no mask: the turn stops being a parseable call."""
    if step >= 12:
        return "corrupted"
    else:
        return unmasked_step(tools, constraint, text, step)


def unmasked_step(tools, constraint, text, step):
    scores = mapList(toList(range(len(constraint.vocabulary))), lambda i: adversarial_draw(i, step))
    picked = argmax(scores)
    return unmasked(tools, constraint, text + token_at(constraint, picked), step + 1)


def adversarial_draw(i, step):
    return toFloat32((step * 7919 + i * 104729) % 1000) / toFloat32(1000.0)


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


def scanned(schema, text):
    """Feed `text` to the scanner and say whether it completed, stalled or was rejected."""
    return scanning(ToolJsonScan.begin(schema), text, 0)


def scanning(stack, text, i):
    """One byte of `text` through the scanner's `Frame` stack, from the start of the text."""
    if i >= len(text):
        return "complete" if finished(stack) else "incomplete"
    else:
        return scan_fed(text, i, ToolJsonScan.feed(stack, unwrap_or(byteAt(text, i), -1)))


def finished(stack):
    match ToolJsonScan.finished(stack):
        case Success(_accepted):
            return True
        case Error(_rejected):
            return False


def scan_fed(text, i, outcome):
    """Apply the `ScanStep` one `feed` returned, which says whether the byte was taken: a
    stage that finished without taking it hands the same byte to the next stage."""
    match outcome:
        case Success(value=stepped):
            return handed(stepped, text, i)
        case Error(_rejected):
            return "rejected"


def handed(stepped, text, i):
    if len(stepped.stack) == 0:
        return "complete" if i + 1 >= len(text) else "trailing"
    elif stepped.consumed:
        return scanning(stepped.stack, text, i + 1)
    else:
        return scanning(stepped.stack, text, i)


# The logits-processor entry point itself, over a token history.

def logits_processor_cases():
    """`apply` and `mask` over a decoded history: the processor a server installs."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return flatten(mapList([0, 1, 2, 7], lambda i: processor_case(tools, record_at(records, i), i)))


def processor_case(tools, record, i):
    tokens = texts_of(unwrap_or(jsonField(record, "tokens"), JsonNull))
    constraint = ToolConstrain.Constrain(tools, tokens, toFloat32(0.0) - toFloat32(exp(1000.0)))
    ids = ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull))
    machine = machine_after(tools, ToolConstrain.decode(constraint, ids))
    scores = mapList(toList(range(len(constraint.vocabulary))), lambda k: corpus_score(record, k))
    masked = ToolConstrain.apply(constraint, ids, scores)
    corpus = unwrap_or(listGet(ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), 0), 0)
    return [
        Check.equal(f"processor_pick[{i}]", argmax(masked), corpus),
        Check.equal(f"processor_mask[{i}]", toString(len(ToolConstrain.mask(constraint, machine, scores))), toString(len(scores))),
        Check.equal(f"processor_accepts[{i}]", ToolConstrain.accepts(constraint, ToolConstrain.decode(constraint, ids) + token_at(constraint, corpus)), True),
    ]


def corpus_score(record, k):
    """The corpus token outranks everything, so the processor has to keep it."""
    return toFloat32(1.0) if k == unwrap_or(listGet(ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), 0), 0) else toFloat32(0.0)


# The corners the corpus leaves out: numbers with exponents, string escapes, an
# unconstrained object, and a turn that carries two calls.

def corner_cases():
    """Scanner and parser corners: what the 200 turns happen not to exercise."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    number = decoded("{\"type\":\"number\"}")
    text_schema = decoded("{\"type\":\"string\"}")
    free = decoded("{\"type\":\"object\",\"properties\":{\"x\":{\"type\":\"integer\"}}}")
    return [
        Check.equal("corner_number", scanned(number, "-1.5e3"), "complete"),
        Check.equal("corner_number_bad", scanned(number, "1.5e"), "incomplete"),
        Check.equal("corner_escapes", scanned(text_schema, "\"a\\u00e9\\n\""), "complete"),
        Check.equal("corner_extra", scanned(free, "{\"x\": 1, \"y\": 2}"), "complete"),
        Check.equal("corner_two_calls", two_call_fingerprint(tools, records), two_call_want(records)),
    ]


def two_call_fingerprint(tools, records):
    """One turn with the call blocks of the first two cases: two calls, in order."""
    first = text_of(record_at(records, 0), "wire")
    second = text_of(record_at(records, 1), "wire")
    wire = keep_from(first, "<tool_call>") + keep_from(second, "<tool_call>")
    return turn_fingerprint(parsed_turn(tools, "<think>\n</think>\n\n" + wire))


def keep_from(wire, marker):
    """`wire` from `marker` on: the call block without the think-and-text prefix."""
    return drop(wire, unwrap_or(indexOf(wire, marker), 0))


def two_call_want(records):
    return "\n\n|\n|" + call_fingerprint_of(record_at(records, 0)) + ";" + call_fingerprint_of(record_at(records, 1))


def call_fingerprint_of(record):
    return f"{text_of(record, 'name')}|{text_of(record, 'args')}|"


def tool_cases():
    return listConcat(corner_cases(), listConcat(reader_cases(), listConcat(scanner_cases(), listConcat(validation_cases(), listConcat(coercion_cases(), listConcat(parse_cases(), listConcat(chunked_cases(), listConcat(repair_cases(), listConcat(constrain_cases(), listConcat(control_cases(), logits_processor_cases()))))))))))


# --- the tiny Qwen3_5TextConfig checkpoint: [LLM-MODEL-ORACLE] ---

def shape_size(shape):
    """The element count of one row-major shape."""
    return foldList(shape, 1, lambda total, extent: total * extent)


def shape_total(shapes):
    """The element count of every shape in a manifest."""
    return foldList(shapes, 0, lambda total, shape: total + shape_size(shape))


def manifest_cases(parameter_values):
    """The fixture's own counts: the declared parameter count is the sum of the recorded
    shapes, and every tensor has one manifest row per value list (so the fixture cannot
    describe a shape it does not carry)."""
    return [
        Check.equal("model_param_count", shape_total(Expected.MODEL_PARAM_SHAPES), Expected.MODEL_PARAM_COUNT),
        Check.equal("model_param_tensors", len(Expected.MODEL_PARAM_NAMES), len(Expected.MODEL_PARAM_SHAPES)),
        Check.equal("model_param_dtypes", len(Expected.MODEL_PARAM_DTYPES), len(parameter_values)),
        Check.equal("model_cache_tensors", len(Expected.MODEL_CACHE_NAMES), len(Expected.MODEL_CACHE_VALUES)),
        Check.equal("model_cache_dtypes", len(Expected.MODEL_CACHE_DTYPES), len(Expected.MODEL_CACHE_SHAPES)),
        Check.equal("model_hidden_stages", len(Expected.MODEL_HIDDEN_STAGES), Expected.MODEL_LAYERS + 1),
        Check.equal("model_hidden_states", len(Expected.MODEL_HIDDEN_VALUES), len(Expected.MODEL_INPUT_TOKENS) * len(Expected.MODEL_HIDDEN_STAGES)),
        Check.equal("model_inputs", len(Expected.MODEL_INPUTS), len(Expected.MODEL_INPUT_TOKENS)),
        Check.equal("model_layer_types", len(Expected.MODEL_LAYER_TYPES), Expected.MODEL_LAYERS),
        Check.equal("model_layer_pattern", join(Expected.MODEL_LAYER_TYPES, ","), "linear_attention,linear_attention,linear_attention,full_attention"),
        Check.equal("model_hook_names", len(Expected.MODEL_HOOK_NAMES), len(Expected.MODEL_HOOK_SHAPES)),
        Check.equal("model_hook_dtypes", len(Expected.MODEL_HOOK_DTYPES), len(Expected.MODEL_HOOK_SHAPES)),
        Check.equal("model_dtype_path", len(Expected.MODEL_DTYPE_PATH), 8),
    ]


def param_cases(parameter_values):
    """One assertion per recorded parameter tensor: its values fill its shape."""
    return toList(map(range(len(parameter_values)), lambda k: Check.equal(
        "model_param[" + name_at(Expected.MODEL_PARAM_NAMES, k) + "]",
        len(unwrap_or(listGet(parameter_values, k), [])),
        shape_size(unwrap_or(listGet(Expected.MODEL_PARAM_SHAPES, k), [])),
    )))


def hidden_cases():
    """One assertion per recorded hidden state: one value per token per hidden channel."""
    return toList(map(range(len(Expected.MODEL_HIDDEN_VALUES)), lambda k: Check.equal(
        "model_hidden[" + name_at(Expected.MODEL_HIDDEN_STAGES, k % len(Expected.MODEL_HIDDEN_STAGES)) + "]",
        len(unwrap_or(listGet(Expected.MODEL_HIDDEN_VALUES, k), [])),
        unwrap_or(listGet(Expected.MODEL_HIDDEN_TOKENS, k), 0) * Expected.MODEL_HIDDEN,
    )))


def cache_cases():
    """One assertion per cache state tensor: its values fill its shape."""
    return toList(map(range(len(Expected.MODEL_CACHE_VALUES)), lambda k: Check.equal(
        "model_cache[" + name_at(Expected.MODEL_CACHE_NAMES, k) + "]",
        len(unwrap_or(listGet(Expected.MODEL_CACHE_VALUES, k), [])),
        shape_size(unwrap_or(listGet(Expected.MODEL_CACHE_SHAPES, k), [])),
    )))


def name_at(names, k):
    return unwrap_or(listGet(names, k), "")


def model_fixture_cases():
    """[LLM-MODEL-ORACLE] The tiny `Qwen3_5TextConfig` fixture transformers wrote is
    self-consistent (its counts and shapes), and the device model built from its
    parameters reproduces its activations and cache."""
    parameter_values = Expected.MODEL_PARAM_VALUES
    return listConcat(manifest_cases(parameter_values), listConcat(param_cases(parameter_values), listConcat(hidden_cases(), listConcat(cache_cases(), model_layer_cases(fixture_params(parameter_values))))))


# --- the qwen3_5 model on device buffers: [LLM-MODEL-DELTANET], [LLM-MODEL-ATTENTION], [LLM-MODEL-ROPE], [LLM-MODEL-CACHE] ---

MODEL_RTOL = 1e-5
MODEL_ATOL = 1e-6
"""The port's tolerance against the fixture: the path runs binary32 arithmetic whose
dot products reduce in a different order from torch's CPU GEMM and whose `exp`/`ln`
come from the device library, so [PORT-TEST-ORACLE]'s exact-arithmetic tolerances do
not apply."""
SINGLE = 0
"""`MODEL_HIDDEN_VALUES` index of the one-token input's embedding; its stages follow."""
CACHED = 10
"""`MODEL_HIDDEN_VALUES` index of the cached run's embeddings; its stages follow."""


def index_in(names, name):
    """The position of `name` in a fixture name list, or -1 when it is absent."""
    return fold(range(len(names)), -1, lambda hit, i: i if hit < 0 and unwrap_or(listGet(names, i), "") == name else hit)


def param_shape(name):
    """One recorded parameter's shape, or `[]` when the manifest has no such name."""
    return unwrap_or(listGet(Expected.MODEL_PARAM_SHAPES, index_in(Expected.MODEL_PARAM_NAMES, name)), [])


def extent(shape, axis):
    return unwrap_or(listGet(shape, axis), 0)


def model_config():
    """The fixture's `Qwen3_5TextConfig`: the attention and MLP widths come from the
    recorded parameter shapes; the linear-attention head split (two key heads of eight,
    four value heads of eight) and the rope settings from the config the oracle's
    docstring records, since `[64, 64]` alone does not say how `key_dim` splits."""
    head_dim = extent(param_shape("layers.3.self_attn.q_norm.weight"), 0)
    conv = param_shape("layers.0.linear_attn.conv1d.weight")
    return Qwen3_5Config.Config(
        hidden_size=Expected.MODEL_HIDDEN,
        intermediate_size=extent(param_shape("layers.3.mlp.gate_proj.weight"), 0),
        vocab_size=Expected.MODEL_VOCAB,
        num_hidden_layers=Expected.MODEL_LAYERS,
        num_attention_heads=extent(param_shape("layers.3.self_attn.q_proj.weight"), 0) // (2 * head_dim),
        num_key_value_heads=extent(param_shape("layers.3.self_attn.k_proj.weight"), 0) // head_dim,
        head_dim=head_dim,
        partial_rotary_factor=0.25,
        rope_theta=10000.0,
        rms_norm_eps=0.000001,
        linear_num_key_heads=2,
        linear_num_value_heads=extent(param_shape("layers.0.linear_attn.A_log"), 0),
        linear_key_head_dim=8,
        linear_value_head_dim=extent(param_shape("layers.0.linear_attn.norm.weight"), 0),
        linear_conv_kernel_dim=extent(conv, 2),
        layer_types=Expected.MODEL_LAYER_TYPES,
    )


def to_floats(xs):
    """A numeric `MODEL_*` row as float32 (the fixture writes the shortest decimal that
    reads back as the same binary32)."""
    return mapList(xs, toFloat32)


def fixture_params(parameter_values):
    return Qwen3_5Hf.Params(names=Expected.MODEL_PARAM_NAMES, values=mapList(parameter_values, to_floats))


def hidden_row(k, t):
    """Row `t` of `MODEL_HIDDEN_VALUES[k]`."""
    return take_row(to_floats(unwrap_or(listGet(Expected.MODEL_HIDDEN_VALUES, k), [])), t * Expected.MODEL_HIDDEN, Expected.MODEL_HIDDEN)


def hook_row(name, t):
    """Row `t` of one `MODEL_HOOK_*` activation, or `[]` when the fixture has no such hook."""
    return take_row(to_floats(unwrap_or(listGet(Expected.MODEL_HOOK_VALUES, index_in(Expected.MODEL_HOOK_NAMES, name)), [])), t * Expected.MODEL_HIDDEN, Expected.MODEL_HIDDEN)


def cache_value(name):
    """One `MODEL_CACHE_*` state tensor, flattened, or `[]` when the cache has no such name."""
    return to_floats(unwrap_or(listGet(Expected.MODEL_CACHE_VALUES, index_in(Expected.MODEL_CACHE_NAMES, name)), []))


def take_row(xs, start, width):
    return toList(map(range(width), lambda i: unwrap_or(listGet(xs, start + i), toFloat32(0.0))))


def compare(name, buffer, want):
    """A device buffer read back and compared with a fixture row."""
    match fromGpu(buffer):
        case Success(value=got):
            return close_list(name, got, want, MODEL_RTOL, MODEL_ATOL)
        case Error(message=message):
            return Check.failed(name, toString(message))


def compare_rows(name, buffer, start, count, want):
    """`count` values of a buffer from `start` against a fixture tensor."""
    match fromGpu(buffer):
        case Success(value=got):
            return close_list(name, take_row(got, start, count), want, MODEL_RTOL, MODEL_ATOL)
        case Error(message=message):
            return Check.failed(name, toString(message))


def model_layer_cases(parameters):
    """The device model from the fixture's parameters: the one-token input and the
    eight tokens fed one at a time through the cache reproduce every recorded hidden
    stage, the first cached token every layer hook, the final cache every state tensor,
    and the loader refuses a missing or mis-sized parameter."""
    match Qwen3_5Hf.load(parameters, model_config()):
        case Error(message=message):
            return [Check.failed("model_load", message)]
        case Success(value=model):
            return listConcat([Check.passed("model_load")], listConcat(single_cases(model), listConcat(cached_cases(model), listConcat(tiled_cases(model, parameters), refusal_cases()))))


def token_in(tokens, t):
    return unwrap_or(listGet(tokens, t), 0)


def input_tokens(i):
    return unwrap_or(listGet(Expected.MODEL_INPUTS, i), [])


def started(model, label, run):
    """`run` over a fresh cache, or the failure to allocate one."""
    match Qwen3_5Cache.empty(model.config, model.shapes):
        case Success(value=cache):
            return run(cache)
        case Error(message=message):
            return [Check.failed(f"{label}_cache", message)]


def single_cases(model):
    """`MODEL_INPUTS[0]`, one token at position 0, from a fresh cache."""
    return started(model, "model_single", lambda cache: single_run(model, cache))


def single_run(model, cache):
    match Qwen3_5Model.decode(model, cache, token_in(input_tokens(0), 0)):
        case Error(message=message):
            return [Check.failed("model_single", message)]
        case Success(value=next):
            return listConcat(stage_cases("model_single", next, SINGLE, 0), [greedy_case("model_single", next)])


def stage_cases(label, next, base, t):
    """The embedding, the three recorded layer outputs and the final norm of row `t`."""
    return listConcat(
        [compare(f"{label}_embed", next.row, hidden_row(base, t))],
        listConcat(
            toList(map(range(len(Expected.MODEL_HIDDEN_STAGES) - 2), lambda i: stage_case(label, next, base, t, i))),
            [compare(f"{label}_final_norm", next.normed, hidden_row(base + len(Expected.MODEL_HIDDEN_STAGES) - 1, t))],
        ),
    )


def stage_case(label, next, base, t, i):
    match listGet(next.walk.stages, i):
        case Success(value=stage):
            return compare(f"{label}_layer{i}", stage, hidden_row(base + 1 + i, t))
        case Error(message=message):
            return Check.failed(f"{label}_layer{i}", toString(message))


def greedy_case(label, next):
    """The device argmax agrees with the host's over the same logits."""
    match fromGpu(next.logits):
        case Success(value=values):
            return Check.equal(f"{label}_greedy", next.best, argmax(values))
        case Error(message=message):
            return Check.failed(f"{label}_greedy", toString(message))


def cached_cases(model):
    """`MODEL_INPUTS[2]` fed one token at a time, each through the cache the earlier tokens left."""
    return started(model, "model_cached", lambda cache: walk_tokens(model, input_tokens(2), 0, cache, []))


def walk_tokens(model, tokens, t, cache, acc):
    if t == len(tokens):
        return listConcat(acc, cache_tensor_cases(model, cache))
    else:
        match Qwen3_5Model.decode(model, cache, token_in(tokens, t)):
            case Error(message=message):
                return listAppend(acc, Check.failed(f"model_cached{t}", message))
            case Success(value=next):
                return walk_tokens(model, tokens, t + 1, next.walk.cache, listConcat(acc, listConcat(stage_cases(f"model_cached{t}", next, CACHED, t), hook_cases(next, t))))


def hook_cases(next, t):
    """At position 0 the batched run and the cached run agree, so the first token also
    checks every layer's output hook, including layer 3's, which no hidden stage records."""
    if t == 0:
        return toList(map(range(Expected.MODEL_LAYERS), lambda i: hook_case(next, i)))
    else:
        return []


def hook_case(next, i):
    match listGet(next.walk.stages, i):
        case Success(value=stage):
            return compare(f"model_hook_layer{i}", stage, hook_row(f"layers.{i}", 0))
        case Error(message=message):
            return Check.failed(f"model_hook_layer{i}", toString(message))


def cache_tensor_cases(model, cache):
    """Every slot of the final cache against the fixture's `layers.N.*` state tensors."""
    return flatten(toList(map(range(model.config.num_hidden_layers), lambda i: slot_cases(model, cache, i))))


def slot_cases(model, cache, i):
    match Qwen3_5Cache.slot_at(cache, i):
        case Error(message=message):
            return [Check.failed(f"model_cache_slot{i}", toString(message))]
        case Success(value=slot):
            return held_cases(model, slot, i)


def held_cases(model, slot, i):
    match slot:
        case Qwen3_5Cache.Delta(held=held):
            return [
                compare(f"model_cache{i}_conv_states", held.conv, cache_value(f"layers.{i}.conv_states.0")),
                compare_rows(f"model_cache{i}_recurrent_states", held.state, 0, Qwen3_5Config.state_cells(model.config), cache_value(f"layers.{i}.recurrent_states.0")),
            ]
        case Qwen3_5Cache.Attention(held=held):
            return [
                compare_heads(f"model_cache{i}_keys", held.keys, held.positions, model.config, cache_value(f"layers.{i}.keys.0")),
                compare_heads(f"model_cache{i}_values", held.values, held.positions, model.config, cache_value(f"layers.{i}.values.0")),
            ]


def compare_heads(name, buffer, positions, cfg, want):
    """A position-major KV buffer against the fixture's `[heads, positions, head_dim]` tensor."""
    match fromGpu(buffer):
        case Success(value=got):
            return close_list(name, head_major(got, positions, cfg.num_key_value_heads, cfg.head_dim), want, MODEL_RTOL, MODEL_ATOL)
        case Error(message=message):
            return Check.failed(name, toString(message))


def head_major(flat, positions, heads, width):
    return toList(map(range(heads * positions * width), lambda j: unwrap_or(listGet(flat, ((j // width) % positions) * heads * width + (j // (positions * width)) * width + j % width), toFloat32(0.0))))


def tiled_cases(model, parameters):
    """The GGUF value-head order: the same checkpoint with every per-value-head
    parameter permuted into llama.cpp's tiled order, loaded as `tiled`, yields the same
    one-token activations as Transformers' grouped order."""
    match Qwen3_5Hf.load(tiled_params(model.config, parameters), model.config):
        case Error(message=message):
            return [Check.failed("model_tiled_load", message)]
        case Success(value=grouped):
            return tiled_run(Qwen3_5Model.Model(config=grouped.config, shapes=grouped.shapes, embedding=grouped.embedding, layers=grouped.layers, final_norm=grouped.final_norm, head=grouped.head, tiled=1))


def tiled_run(model):
    return started(model, "model_tiled", lambda cache: tiled_decode(model, cache))


def tiled_decode(model, cache):
    match Qwen3_5Model.decode(model, cache, token_in(input_tokens(0), 0)):
        case Error(message=message):
            return [Check.failed("model_tiled", message)]
        case Success(value=next):
            return stage_cases("model_tiled", next, SINGLE, 0)


def tiled_params(cfg, parameters):
    """`fixture_params` with the linear-attention value heads in tiled order, as
    `convert_hf_to_gguf.py`'s `_reorder_v_heads` writes them."""
    values = toList(map(range(len(parameters.names)), lambda k: tiled_param(cfg, name_at(parameters.names, k), unwrap_or(listGet(parameters.values, k), []))))
    return Qwen3_5Hf.Params(names=parameters.names, values=values)


def tiled_param(cfg, name, values):
    hk = cfg.linear_num_key_heads
    r = cfg.linear_num_value_heads // hk
    dv = cfg.linear_value_head_dim
    hidden = cfg.hidden_size
    if contains(name, "in_proj_qkv") or contains(name, "conv1d"):
        return reordered_rows(values, 2 * Qwen3_5Config.key_dim(cfg), hk, r, dv, len(values) // Qwen3_5Config.conv_dim(cfg))
    elif contains(name, "in_proj_z"):
        return reordered_rows(values, 0, hk, r, dv, hidden)
    elif contains(name, "in_proj_b") or contains(name, "in_proj_a"):
        return reordered_rows(values, 0, hk, r, 1, hidden)
    elif contains(name, "A_log") or contains(name, "dt_bias"):
        return reordered_rows(values, 0, hk, r, 1, 1)
    elif contains(name, "out_proj"):
        return reordered_columns(values, hk, r, dv, hidden)
    else:
        return values


def grouped_of(tiled, hk, r):
    """The grouped value head that sits at tiled position `tiled`: `(t % hk) * r + t // hk`."""
    return (tiled % hk) * r + tiled // hk


def reordered_rows(values, skip, hk, r, dim, width):
    """Rows of `width` from `skip` on, grouped by value head of `dim` rows, in tiled order."""
    return toList(map(range(len(values)), lambda j: unwrap_or(listGet(values, j if j < skip * width else source_row(j - skip * width, hk, r, dim, width) + skip * width), toFloat32(0.0))))


def source_row(j, hk, r, dim, width):
    row = j // width
    return (grouped_of(row // dim, hk, r) * dim + row % dim) * width + j % width


def reordered_columns(values, hk, r, dim, rows):
    """`[rows, hk * r * dim]` with its columns in tiled value-head order."""
    cols = hk * r * dim
    return toList(map(range(len(values)), lambda j: unwrap_or(listGet(values, (j // cols) * cols + grouped_of((j % cols) // dim, hk, r) * dim + j % dim), toFloat32(0.0))))


def refusal_cases():
    """A checkpoint missing a parameter, or holding one of the wrong size, is refused by name."""
    cfg = model_config()
    return [
        refusal("model_load_missing", Qwen3_5Hf.load(Qwen3_5Hf.Params(names=["norm.weight"], values=[[toFloat32(1.0)]]), cfg), "parameter embed_tokens.weight is not in the checkpoint"),
        refusal("model_load_sized", Qwen3_5Hf.load(Qwen3_5Hf.Params(names=["embed_tokens.weight"], values=[[toFloat32(1.0)]]), cfg), "parameter embed_tokens.weight holds 1 values, not 8192"),
    ]


def refusal(name, result, want):
    match result:
        case Success(value=_):
            return Check.failed(name, "loaded a checkpoint that should have been refused")
        case Error(message=message):
            return Check.equal(name, message, want)
