# osprey: flavor=python
# osprey: namespace=qwen_tests
# osprey: module=PrefillCases
"""[LLM-PREFILL] The prompt path against the fixture: the cached run's whole input in
one `prefill` call, the same input in passes of 3 and of 1, a shorter prefill
continued by `decode`, and the one-token input,
each compared with the rows the per-token path already proved ([LLM-MODEL-ORACLE]).
The same runs with the KV cache sealed every 1, 2 and 3 positions put the 8 positions
across a sealed segment and a tail, the path every prompt past 256 positions takes
([LLM-STATE-OWNED])."""
from testing import Check
from qwen import Qwen3_5Model
from qwen_tests.ModelCases import CACHED, SINGLE, compare, flatten, greedy_case, input_tokens, logits_row, refusal, stage_cases, started, token_in

__all__ = ["prefill_cases"]


def prefill_cases(model):
    """Every prefill check, over the fresh cache `started` allocates."""
    return listConcat(started(model, "model_prefill", lambda cache: prefill_runs(model, cache)), flatten(mapList([1, 2, 3], lambda block: blocked_cases(model, block))))


def blocked_cases(model, block):
    """The 8-token runs with the KV cache sealed every `block` positions."""
    sealed = update(model, shapes=update(model.shapes, kv_block=block))
    label = f"model_kv_block{block}"
    return started(sealed, label, lambda cache: blocked_runs(sealed, cache, label))


def blocked_runs(model, cache, label):
    """One prefill of all 8 tokens, passes of one token each, and 7 prefilled then 1
    decoded: position 7 equals the cached run's rows, logits and argmax every way."""
    tokens = input_tokens(2)
    return listConcat(position7_cases(f"{label}_prefill8", Qwen3_5Model.prefill(model, cache, tokens), True), listConcat(position7_cases(f"{label}_by1", Qwen3_5Model.prefill_by(model, cache, tokens, 1), True), tail_cases(model, cache, label, True)))


def position7_cases(label, result, greedy):
    """Position 7 of `result`: its stage rows, its logits and its argmax (only when `greedy`)."""
    match result:
        case Error(message=message):
            return [Check.failed(label, message)]
        case Success(value=next):
            return listConcat(stage_cases(label, next, CACHED, 7), listConcat([logits_case(label, next)], [greedy_case(label, next)] if greedy else []))


def tail_cases(model, cache, label, greedy):
    """Tokens 0..6 prefilled, token 7 decoded from the cache that prefill left: the
    cache a prefill ends with is the one `decode` would have built."""
    tokens = input_tokens(2)
    first = toList(map(range(len(tokens) - 1), lambda t: token_in(tokens, t)))
    match Qwen3_5Model.prefill(model, cache, first):
        case Error(message=message):
            return [Check.failed(f"{label}_then_decode", message)]
        case Success(value=next):
            return position7_cases(f"{label}_then_decode", Qwen3_5Model.decode(model, next.walk.cache, token_in(tokens, len(tokens) - 1)), greedy)


def prefill_runs(model, cache):
    return listConcat(whole_prompt_cases(model, cache), listConcat(chunked_cases(model, cache, 3), listConcat(chunked_cases(model, cache, 1), listConcat(tail_cases(model, cache, "model_prefill7", False), listConcat(one_prompt_cases(model, cache), no_prompt_case(model, cache))))))


def chunked_cases(model, cache, chunk):
    """The 8 tokens in passes of `chunk` (3: passes of 3, 3 and 2; 1: a pass per token):
    each pass resumes from the cache the one before left, and position 7 still equals
    the cached run's rows, logits and argmax."""
    return position7_cases(f"model_prefill8_by{chunk}", Qwen3_5Model.prefill_by(model, cache, input_tokens(2), chunk), True)


def whole_prompt_cases(model, cache):
    """All 8 tokens of the cached run in one call: position 7's stage rows, logits and argmax."""
    return position7_cases("model_prefill8", Qwen3_5Model.prefill(model, cache, input_tokens(2)), True)


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
    """[FAILURE-EXPLICIT] Empty prompts and nonpositive chunk sizes return errors."""
    return [refusal("model_prefill_empty", Qwen3_5Model.prefill(model, cache, []), "prefill: no tokens"), refusal("model_prefill_zero_chunk", Qwen3_5Model.prefill_by(model, cache, input_tokens(0), 0), "prefill: chunk must be positive"), refusal("model_prefill_negative_chunk", Qwen3_5Model.prefill_by(model, cache, input_tokens(0), -1), "prefill: chunk must be positive")]
