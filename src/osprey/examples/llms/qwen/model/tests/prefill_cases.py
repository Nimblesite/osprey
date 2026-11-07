# osprey: flavor=python
# osprey: namespace=qwen_tests
# osprey: module=PrefillCases
"""[LLM-PREFILL] The prompt path against the fixture: the cached run's whole input in
one `prefill` call, the same input in passes of 3 and of 1, a shorter prefill
continued by `decode`, and the one-token input,
each compared with the rows the per-token path already proved ([LLM-MODEL-ORACLE])."""
from testing import Check
from qwen import Qwen3_5Model
from qwen_tests.ModelCases import CACHED, SINGLE, compare, greedy_case, input_tokens, logits_row, refusal, stage_cases, started, token_in

__all__ = ["prefill_cases"]


def prefill_cases(model):
    """Every prefill check, over the fresh cache `started` allocates."""
    return started(model, "model_prefill", lambda cache: prefill_runs(model, cache))


def prefill_runs(model, cache):
    return listConcat(whole_prompt_cases(model, cache), listConcat(chunked_cases(model, cache, 3), listConcat(chunked_cases(model, cache, 1), listConcat(split_prompt_cases(model, cache), listConcat(one_prompt_cases(model, cache), no_prompt_case(model, cache))))))


def chunked_cases(model, cache, chunk):
    """The 8 tokens in passes of `chunk` (3: passes of 3, 3 and 2; 1: a pass per token):
    each pass resumes from the cache the one before left, and position 7 still equals
    the cached run's rows, logits and argmax."""
    label = f"model_prefill8_by{chunk}"
    match Qwen3_5Model.prefill_by(model, cache, input_tokens(2), chunk):
        case Error(message=message):
            return [Check.failed(label, message)]
        case Success(value=next):
            return listConcat(stage_cases(label, next, CACHED, 7), listConcat([logits_case(label, next)], [greedy_case(label, next)]))


def whole_prompt_cases(model, cache):
    """All 8 tokens of the cached run in one call: position 7's stage rows, logits and argmax."""
    match Qwen3_5Model.prefill(model, cache, input_tokens(2)):
        case Error(message=message):
            return [Check.failed("model_prefill8", message)]
        case Success(value=next):
            return listConcat(stage_cases("model_prefill8", next, CACHED, 7), listConcat([logits_case("model_prefill8", next)], [greedy_case("model_prefill8", next)]))


def split_prompt_cases(model, cache):
    """Tokens 0..6 prefilled, token 7 decoded from the cache that prefill left: the
    cache a prefill ends with is the one `decode` would have built."""
    tokens = input_tokens(2)
    first = toList(map(range(len(tokens) - 1), lambda t: token_in(tokens, t)))
    match Qwen3_5Model.prefill(model, cache, first):
        case Error(message=message):
            return [Check.failed("model_prefill7_then_decode", message)]
        case Success(value=next):
            return decode_tail_cases(model, next.walk.cache, token_in(tokens, len(tokens) - 1))


def decode_tail_cases(model, cache, token):
    match Qwen3_5Model.decode(model, cache, token):
        case Error(message=message):
            return [Check.failed("model_prefill7_then_decode", message)]
        case Success(value=next):
            return listConcat(stage_cases("model_prefill7_then_decode", next, CACHED, 7), [logits_case("model_prefill7_then_decode", next)])


def logits_case(label, next):
    """The prompt's last position against the cached run's logits row 7."""
    return compare(f"{label}_logits", next.logits, logits_row(7))


def one_prompt_cases(model, cache):
    """`MODEL_INPUTS[0]`, the one-token input, prefilled: the single run's rows."""
    match Qwen3_5Model.prefill(model, cache, input_tokens(0)):
        case Error(message=message):
            return [Check.failed("model_prefill1", message)]
        case Success(value=next):
            return stage_cases("model_prefill1", next, SINGLE, 0)


def no_prompt_case(model, cache):
    """The empty prompt is refused with the prefill error."""
    return [refusal("model_prefill_empty", Qwen3_5Model.prefill(model, cache, []), "prefill: no tokens")]
