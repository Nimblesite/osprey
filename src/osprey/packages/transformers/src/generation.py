# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Generation
"""A model-independent generate driver.

`generate` takes a `next_logits` callback (`list[int] -> list[float32]`, any model
that scores a token history), a starting token history, a list of
`LogitsProcess` processors, a list of `StoppingCriterion` rules, a sampler
(`list[float32] -> int`; `Math.argmax` for greedy, `Math.multinomial(probs, u)`
for a fixed-draw sample), and a new-token budget. Each step scores the history,
folds the processors over the logits left to right, draws the next token, and
stops as soon as a criterion fires, mirroring `transformers`' generation loop
without any model or tokenizer dependency.

[LLM-ARITH-IEEE] Every row the model scores is checked before the processors
see it: a NaN or an infinity anywhere in it ends the request with an `Error`
naming the step, instead of the garbage `torch.argmax` would pick or the
`RuntimeError` `torch.multinomial` raises on it. The processors' own `-inf`
masks (suppressed tokens, a minimum length) come after the check and are legitimate.
"""

from transformers.LogitsProcess import apply_list
from transformers.StoppingCriteria import stopped_any

__all__ = ["generate"]


def generate(next_logits, tokens, processors, stoppers, sampler, max_new_tokens) -> Result[list[int], str]:
    """Produce up to `max_new_tokens` more tokens after `tokens`, stopping when a
    criterion fires. `processors` is a `list[LogitsProcessor]`, `stoppers` a
    `list[StoppingCriterion]`, and `sampler` picks one token from a logits row.
    A non-finite logits row is an `Error` naming the step that produced it."""
    if max_new_tokens < 0:
        return Error(message="generate: negative token budget")
    else:
        return step(next_logits, tokens, processors, stoppers, sampler, max_new_tokens, 1)


def finite(logits):
    """Whether every logit is a finite number: NaN is unequal to itself and an
    infinity exceeds the largest float32 in magnitude."""
    return foldList(logits, True, lambda ok, x: ok and finite_one(x))


def finite_one(x):
    return x == x and x <= toFloat32(3.4028234663852886e38) and x >= toFloat32(-3.4028234663852886e38)


def step(next_logits, tokens, processors, stoppers, sampler, remaining, number):
    if remaining == 0:
        return Success(value=tokens)
    else:
        scored = next_logits(tokens)
        if len(scored) == 0:
            return Error(message=f"generate: step {number}: empty logits")
        elif finite(scored):
            return select(next_logits, tokens, processors, stoppers, sampler, remaining, number, apply_list(processors, tokens, scored))
        else:
            return Error(message=f"generate: step {number}: logits are not finite")


def selectable(scores):
    """[FAILURE-EXPLICIT] Masks are legal, but at least one finite score must remain."""
    return foldList(scores, False, lambda found, x: found or finite_one(x))


def valid_scores(scores):
    """Processors may mask with -infinity, never introduce NaN or +infinity."""
    return foldList(scores, True, lambda ok, x: ok and (finite_one(x) or x < toFloat32(-3.4028234663852886e38)))


def select(next_logits, tokens, processors, stoppers, sampler, remaining, number, scores):
    if not valid_scores(scores):
        return Error(message=f"generate: step {number}: invalid processed logits")
    elif not selectable(scores):
        return Error(message=f"generate: step {number}: no selectable logits")
    else:
        chosen = sampler(scores)
        match listGet(scores, chosen):
            case Success(value=score):
                if finite_one(score):
                    return advance(next_logits, listAppend(tokens, chosen), processors, stoppers, sampler, remaining, number)
                else:
                    return Error(message=f"generate: step {number}: invalid sampled token")
            case Error():
                return Error(message=f"generate: step {number}: invalid sampled token")


def advance(next_logits, grown, processors, stoppers, sampler, remaining, number):
    if stopped_any(stoppers, grown):
        return Success(value=grown)
    else:
        return step(next_logits, grown, processors, stoppers, sampler, remaining - 1, number + 1)
