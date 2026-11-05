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
    return step(next_logits, tokens, processors, stoppers, sampler, max_new_tokens, 1)


def finite(logits) -> bool:
    """Whether every logit is a finite number: NaN is unequal to itself and an
    infinity exceeds the largest float32 in magnitude."""
    return fold(range(len(logits)), True, lambda ok, i: ok and finite_one(unwrap_or(listGet(logits, i), toFloat32(0.0))))


def finite_one(x) -> bool:
    return x == x and x <= toFloat32(3.4028234663852886e38) and x >= toFloat32(-3.4028234663852886e38)


def step(next_logits, tokens, processors, stoppers, sampler, remaining, number) -> Result[list[int], str]:
    if remaining == 0:
        return Success(value=tokens)
    else:
        scored = next_logits(tokens)
        if finite(scored):
            return advance(next_logits, listAppend(tokens, sampler(apply_list(processors, tokens, scored))), processors, stoppers, sampler, remaining, number)
        else:
            return Error(message=f"generate: step {number}: logits are not finite")


def advance(next_logits, grown, processors, stoppers, sampler, remaining, number) -> Result[list[int], str]:
    if stopped_any(stoppers, grown):
        return Success(value=grown)
    else:
        return step(next_logits, grown, processors, stoppers, sampler, remaining - 1, number + 1)
