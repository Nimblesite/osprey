# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=Generation
"""A model-independent generate driver.

`generate` takes a `next_logits` callback (`list[int] -> list[float]`, any model
that scores a token history), a starting token history, a list of
`LogitsProcess` processors, a list of `StoppingCriterion` rules, a sampler
(`list[float] -> int`; `Math.argmax` for greedy, `Math.multinomial(probs, u)`
for a fixed-draw sample), and a new-token budget. Each step scores the history,
folds the processors over the logits left to right, draws the next token, and
stops as soon as a criterion fires, mirroring `transformers`' generation loop
without any model or tokenizer dependency.
"""

from transformers.LogitsProcess import apply_list
from transformers.StoppingCriteria import stopped_any

__all__ = ["generate"]


def generate(next_logits, tokens, processors, stoppers, sampler, max_new_tokens):
    """Produce up to `max_new_tokens` more tokens after `tokens`, stopping when a
    criterion fires. `processors` is a `list[LogitsProcessor]`, `stoppers` a
    `list[StoppingCriterion]`, and `sampler` picks one token from a logits row."""
    return step(next_logits, tokens, processors, stoppers, sampler, max_new_tokens)


def step(next_logits, tokens, processors, stoppers, sampler, remaining):
    if remaining == 0:
        return tokens
    else:
        logits = apply_list(processors, tokens, next_logits(tokens))
        token = sampler(logits)
        grown = listAppend(tokens, token)
        return grown if stopped_any(stoppers, grown) else step(next_logits, grown, processors, stoppers, sampler, remaining - 1)
