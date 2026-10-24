# osprey: flavor=python
# osprey: namespace=transformers
# osprey: module=StoppingCriteria
"""Stopping criteria for token generation.

Each exported constructor returns a `Criterion` record; `stopped` checks one
over a token history, and `stopped_any` ORs a list of them, matching
`StoppingCriteriaList.__call__`. `MaxLengthCriteria` stops when the sequence
reaches a length; `EosTokenCriteria` stops when an eos id appears as the newest
token. `ends_with_any` is the stop-string suffix check: transformers'
`StopStringCriteria` precompiles stop strings against a tokenizer's vocabulary,
but Osprey's effect system cannot carry a `list[int] -> str` decode callback
through a record, so callers run their own decode and pass the text to
`ends_with_any` (see README).

`EosTokenCriteria` checks the single newest token (`new_token_length=1`, the
transformers default).
"""

from transformers.Lists import contains_int, last_n

__all__ = ["Criterion", "MaxLengthCriteria", "EosTokenCriteria", "ends_with_any", "stopped", "stopped_any"]


class Criterion:
    """One stopping rule's configuration; `kind` selects the behaviour `stopped`
    dispatches on."""
    kind: str
    max_length: int
    eos_ids: list[int]


def MaxLengthCriteria(max_length: int) -> Criterion:
    """`MaxLengthCriteria(max_length)`: done once the sequence has `max_length`
    tokens."""
    return Criterion(kind="max_length", max_length=max_length, eos_ids=[])


def EosTokenCriteria(eos_ids: list[int]) -> Criterion:
    """`EosTokenCriteria(eos_ids)`: done once the newest token is one of
    `eos_ids` (new_token_length=1, the transformers default)."""
    return Criterion(kind="eos", max_length=0, eos_ids=eos_ids)


def stopped_any(criteria, tokens):
    """`StoppingCriteriaList.__call__`: True when any criterion is done."""
    return foldList(criteria, False, lambda done, c: done or stopped(c, tokens))


def stopped(criterion, tokens):
    """Check one criterion over a token history."""
    match criterion.kind:
        case "max_length":
            return len(tokens) >= criterion.max_length
        case "eos":
            return any_eos(tokens, criterion.eos_ids, 1)
        case _:
            return False


def ends_with_any(text, stop_strings):
    """Whether `text` ends with any stop string."""
    return foldList(stop_strings, False, lambda found, s: found or endsWith(text, s))


def any_eos(tokens, eos, k):
    """Whether any of the newest `k` tokens is an eos id."""
    newest = last_n(tokens, k)
    return foldList(eos, False, lambda found, e: found or contains_int(newest, e))
