# osprey: flavor=python
"""@osprey/transformers's suite: every processor, warper, sampler and stopping
criterion against values the real transformers 5.20.2 + torch 2.14.1 (float64)
computed (scripts/oracles/transformers.py).

The shared writer (scripts/oracles/expected_module.py) writes every finite float
constant as a float literal; a constant holding a `-inf` or NaN is written with
all its floats as repr strings, decoded here with `Check.float_of`."""
from testing import Check
from transformers_expected import Expected
from transformers import Generation, Lists, LogitsProcess, StoppingCriteria
from transformers.Math import argmax, log_softmax, max_f, max_i, min_f, min_i, multinomial, neg_inf, softmax

ARITH = 1e-12
EXP = 1e-9
ATOL = 1e-12


def floats_of(xs):
    """A logits row the oracle wrote as text (it holds -inf/NaN) as floats."""
    return mapList(xs, Check.float_of)


def decode_rows(xs):
    """Every row of a `list[list[str]]` constant, decoded into floats."""
    return toList(map(range(len(xs)), lambda i: floats_of(unwrap_or(listGet(xs, i), []))))



def applied(processor: LogitsProcess.Processor, ids, scores):
    """One `Processor` record over one row, as generate applies it."""
    return LogitsProcess.apply(processor, ids, scores)


def stops(criterion: StoppingCriteria.Criterion, tokens) -> bool:
    """One `Criterion` record's verdict on a token history."""
    return StoppingCriteria.stopped(criterion, tokens)


def row_case(name, build, i, ids_rows, scores, expected, rtol, atol):
    ids = unwrap_or(listGet(ids_rows, i), [])
    got = applied(build(i), ids, unwrap_or(listGet(scores, i), []))
    want = unwrap_or(listGet(expected, i), [])
    return Check.close_all(f"{name}[{i}]", got, want, rtol, atol)


def rows(name, build, ids_rows, scores, expected, rtol, atol):
    return toList(map(range(len(expected)), lambda i: row_case(name, build, i, ids_rows, scores, expected, rtol, atol)))


def math_cases():
    soft = toList(map(range(len(Expected.SOFTMAX_OUT)), lambda i: Check.close_all(f"softmax[{i}]", softmax(floats_of(unwrap_or(listGet(Expected.SOFTMAX_IN, i), []))), floats_of(unwrap_or(listGet(Expected.SOFTMAX_OUT, i), [])), EXP, ATOL)))
    logsoft = toList(map(range(len(Expected.LOGSOFTMAX_OUT)), lambda i: Check.close_all(f"log_softmax[{i}]", log_softmax(floats_of(unwrap_or(listGet(Expected.LOGSOFTMAX_IN, i), []))), floats_of(unwrap_or(listGet(Expected.LOGSOFTMAX_OUT, i), [])), EXP, ATOL)))
    amax = toList(map(range(len(Expected.ARGMAX_OUT)), lambda i: Check.equal(f"argmax[{i}]", argmax(floats_of(unwrap_or(listGet(Expected.ARGMAX_IN, i), []))), unwrap_or(listGet(Expected.ARGMAX_OUT, i), 0))))
    multi = toList(map(range(len(Expected.MULTI_OUT)), lambda i: Check.equal(f"multinomial[{i}]", multinomial(unwrap_or(listGet(Expected.MULTI_PROBS, i), []), unwrap_or(listGet(Expected.MULTI_U, i), 0.0)), unwrap_or(listGet(Expected.MULTI_OUT, i), 0))))
    return listConcat(soft, listConcat(logsoft, listConcat(amax, multi)))


def build_temp(i):
    return LogitsProcess.TemperatureLogitsWarper(unwrap_or(listGet(Expected.TEMP_TEMP, i), 1.0))


def build_topk(i):
    return LogitsProcess.TopKLogitsWarper(unwrap_or(listGet(Expected.TOPK_K, i), 1), neg_inf(), unwrap_or(listGet(Expected.TOPK_MIN, i), 1))


def build_topp(i):
    return LogitsProcess.TopPLogitsWarper(unwrap_or(listGet(Expected.TOPP_P, i), 1.0), neg_inf(), unwrap_or(listGet(Expected.TOPP_MIN, i), 1))


def build_minp(i):
    return LogitsProcess.MinPLogitsWarper(unwrap_or(listGet(Expected.MINP_P, i), 0.0), neg_inf(), unwrap_or(listGet(Expected.MINP_MIN, i), 1))


def build_typ(i):
    return LogitsProcess.TypicalLogitsWarper(unwrap_or(listGet(Expected.TYP_MASS, i), 0.9), neg_inf(), unwrap_or(listGet(Expected.TYP_MIN, i), 1))


def build_eps(i):
    return LogitsProcess.EpsilonLogitsWarper(unwrap_or(listGet(Expected.EPS_EPS, i), 0.1), neg_inf(), unwrap_or(listGet(Expected.EPS_MIN, i), 1))


def build_eta(i):
    return LogitsProcess.EtaLogitsWarper(unwrap_or(listGet(Expected.ETA_EPS, i), 0.05), neg_inf(), unwrap_or(listGet(Expected.ETA_MIN, i), 1))


def build_repp(i):
    return LogitsProcess.RepetitionPenaltyLogitsProcessor(unwrap_or(listGet(Expected.REPP_PEN, i), 1.0), unwrap_or(listGet(Expected.REPP_IGNORE, i), 0))


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
        rows("temperature", build_temp, Expected.TEMP_IDS, Expected.TEMP_SCORES, Expected.TEMP_EXPECTED, ARITH, ATOL),
        rows("top_k", build_topk, Expected.TOPK_IDS, decode_rows(Expected.TOPK_SCORES), decode_rows(Expected.TOPK_EXPECTED), ARITH, ATOL),
        rows("top_p", build_topp, Expected.TOPP_IDS, Expected.TOPP_SCORES, decode_rows(Expected.TOPP_EXPECTED), EXP, ATOL),
        rows("min_p", build_minp, Expected.MINP_IDS, Expected.MINP_SCORES, decode_rows(Expected.MINP_EXPECTED), EXP, ATOL),
        rows("typical", build_typ, Expected.TYP_IDS, Expected.TYP_SCORES, decode_rows(Expected.TYP_EXPECTED), EXP, ATOL),
        rows("epsilon", build_eps, Expected.EPS_IDS, Expected.EPS_SCORES, decode_rows(Expected.EPS_EXPECTED), EXP, ATOL),
        rows("eta", build_eta, Expected.ETA_IDS, decode_rows(Expected.ETA_SCORES), decode_rows(Expected.ETA_EXPECTED), EXP, ATOL),
        rows("repetition_penalty", build_repp, Expected.REPP_IDS, Expected.REPP_SCORES, Expected.REPP_EXPECTED, ARITH, ATOL),
        rows("no_repeat_ngram", build_nrn, Expected.NRN_IDS, Expected.NRN_SCORES, decode_rows(Expected.NRN_EXPECTED), ARITH, ATOL),
        rows("min_length", build_minl, Expected.MINL_IDS, Expected.MINL_SCORES, decode_rows(Expected.MINL_EXPECTED), ARITH, ATOL),
        rows("min_new_tokens", build_mnt, Expected.MNT_IDS, Expected.MNT_SCORES, decode_rows(Expected.MNT_EXPECTED), ARITH, ATOL),
        rows("forced_bos", build_fbos, Expected.FBOS_IDS, Expected.FBOS_SCORES, decode_rows(Expected.FBOS_EXPECTED), ARITH, ATOL),
        rows("forced_eos", build_feos, Expected.FEOS_IDS, Expected.FEOS_SCORES, decode_rows(Expected.FEOS_EXPECTED), ARITH, ATOL),
        rows("suppress_tokens", build_supp, Expected.SUPP_IDS, Expected.SUPP_SCORES, decode_rows(Expected.SUPP_EXPECTED), ARITH, ATOL),
        rows("bad_words", build_bad, Expected.BAD_IDS, Expected.BAD_SCORES, decode_rows(Expected.BAD_EXPECTED), ARITH, ATOL),
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
    chain_cases = toList(map(range(len(Expected.COMPOSE_EXPECTED)), lambda i: Check.close_all(f"compose[{i}]", LogitsProcess.apply_list(chain, [], unwrap_or(listGet(Expected.COMPOSE_SCORES, i), [])), floats_of(unwrap_or(listGet(Expected.COMPOSE_EXPECTED, i), [])), EXP, ATOL)))
    order_a = toList(map(range(len(Expected.ORDER_A_EXPECTED)), lambda i: Check.close_all(f"order_a[{i}]", LogitsProcess.apply_list(a, [], unwrap_or(listGet(Expected.ORDER_SCORES, i), [])), floats_of(unwrap_or(listGet(Expected.ORDER_A_EXPECTED, i), [])), EXP, ATOL)))
    order_b = toList(map(range(len(Expected.ORDER_B_EXPECTED)), lambda i: Check.close_all(f"order_b[{i}]", LogitsProcess.apply_list(b, [], unwrap_or(listGet(Expected.ORDER_SCORES, i), [])), floats_of(unwrap_or(listGet(Expected.ORDER_B_EXPECTED, i), [])), EXP, ATOL)))
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
    return unwrap_or(listGet(Expected.GEN_STEP_LOGITS, idx), [])


def generate_case():
    processors = [
        LogitsProcess.MinLengthLogitsProcessor(3, Expected.GEN_EOS),
        LogitsProcess.SuppressTokensLogitsProcessor([1]),
    ]
    stoppers = [
        StoppingCriteria.MaxLengthCriteria(7),
        StoppingCriteria.EosTokenCriteria(Expected.GEN_EOS),
    ]
    got = Generation.generate(model_logits, Expected.GEN_PROMPT, processors, stoppers, argmax, 5)
    return Check.ints_equal("generate", got, Expected.GEN_EXPECTED)


def all_cases():
    return listConcat(math_cases(), listConcat(processor_cases(), listConcat(composition_cases(), listConcat(stopping_cases(), listConcat(helper_cases(), [generate_case()])))))


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
