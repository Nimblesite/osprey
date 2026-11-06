# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Sampler
"""One sampling step over a logits row, shared by every `Generator` handler.

`one_token` asks `@osprey/transformers`' `generate` for one token: `argmax` when the
temperature is zero, else a seeded draw over the softmax of the temperature-warped
row, stopping at the end-of-turn token. The draw is deterministic in the request's
seed and the position, so a replayed request samples the same text
([LLM-API-OLLAMA]). Every integer operation here is one of the total helpers
([BUILTIN-TOTAL-ARITH]): a wrapped hash is the definition, not a fault.
"""

from qwen_server import Lists
from transformers import Generation
from transformers import LogitsProcess
from transformers import Math
from transformers import StoppingCriteria

__all__ = ["one_token", "draw"]

DRAW_ROW = 1000000


def draw(seed, at):
    """A deterministic draw in [0, 1) from the seed and the position."""
    return toFloat32(toFloat(residue_draw(wrapMul(satAdd(seed, wrapMul(at, 2654435761)), 1103515245))) / 1000000.0)


def residue_draw(value):
    """`value` modulo `DRAW_ROW`, never negative."""
    return satSub(abs(value), wrapMul(intDiv(abs(value), DRAW_ROW), DRAW_ROW))


def temperature_processors(sampling):
    """The temperature warper, or none for greedy sampling."""
    return [] if sampling.temperature <= 0.0 else [LogitsProcess.TemperatureLogitsWarper(toFloat32(sampling.temperature))]


def sampler(sampling, at):
    """`argmax` for greedy sampling, and a seeded draw over the softmax otherwise."""
    return Math.argmax if sampling.temperature <= 0.0 else (lambda scores: Math.multinomial(Math.softmax(scores), draw(sampling.seed, at)))


def one_token(scores, history, sampling, eos):
    """The token `generate` samples from `scores`, the row for the token after `history`."""
    return sampled(Generation.generate(lambda _tokens: scores, history, temperature_processors(sampling), [StoppingCriteria.EosTokenCriteria([eos])], sampler(sampling, Lists.count(history)), 1))


def sampled(result):
    """The newest token of a one-token generation, or its error."""
    match result:
        case Success(value=tokens):
            return newest_token(listGet(tokens, satSub(Lists.count(tokens), 1)))
        case Error(message=message):
            return Error(message=message)


def newest_token(token):
    """A generated token, or an error when the generation produced none."""
    match token:
        case Success(value=value):
            return Success(value=value)
        case Error(message=message):
            return Error(message=f"generate produced no token: {message}")
