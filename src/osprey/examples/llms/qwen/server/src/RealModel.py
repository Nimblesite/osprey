# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=RealModel
"""The real `qwen3_5` weights behind `Generator`: one GGUF mapped at startup, its cache
threaded across the turn.

`open` parses the file, builds the tokenizer from its own `tokenizer.ggml.*` tables,
reads the end-of-turn id the file declares and maps every tensor through
`Qwen3_5Gguf`; a missing or invalid file is the startup error, never a served 500. A
`Session` is the handle: the model, the tokenizer, the tokens the cache has seen and
the logits after the last of them. A step feeds only the tokens the history adds past
what the cache holds, so a turn costs one forward pass per token; a history that does
not extend the cache starts from a fresh one ([LLM-MODEL-CACHE], [LLM-TOKENIZER],
[LLM-WEIGHTS-MAPPED]). Sampling is `Sampler.one_token` over the host copy of the
logits, as for the fixture table, so both handlers obey the same request options.
"""

from gguf import Chain, Reader
from qwen import Qwen3_5Cache, Qwen3_5Gguf, Qwen3_5Model
from qwen_server import Inference
from qwen_server import Lists
from qwen_server import Sampler
from tokenizers import GgufVocab, Tokenizer
from tokenizers.Tokenizer import qwen_decode, qwen_encode

__all__ = ["Session", "Real", "open"]


class Session:
    """The installed model and where its cache stands."""
    model: Qwen3_5Model.Model
    tok: Tokenizer.QwenTokenizer
    eos: int
    cache: Qwen3_5Cache.Cache
    seen: list[int]
    logits: list[float32]


class Tables:
    """The three vocabulary tables a GGUF carries."""
    tokens: list[str]
    types: list[int]
    merges: list[str]


class Fed:
    """The cache after some tokens and the logits the last one produced."""
    cache: Qwen3_5Cache.Cache
    logits: GpuBuffer[float32]


@handler(Inference.Generator)
class Real:
    """The `Generator` handler over a `Session`."""

    def encode(session, text):
        return typed(Chain.then(qwen_encode(session.tok, text), lambda encoding: Success(value=encoding.ids)))

    def decode(session, token):
        return typed(qwen_decode(session.tok, [token], False))

    def eos(session):
        return session.eos

    def step(session, history, sampling):
        return typed(advanced(session, history, sampling))


def typed(result):
    """A string-error result as the `Error`-typed result the effect declares."""
    match result:
        case Success(value=value):
            return Success(value=value)
        case Error(message=message):
            return Error(message=message)


# --- opening the file -----------------------------------------------------

def strings(file, key):
    return Chain.then(Reader.field(file, key), lambda field: Reader.field_strings(field))


def ints(file, key):
    return Chain.then(Reader.field(file, key), lambda field: Reader.field_ints(field))


def tables(file):
    """`tokenizer.ggml.tokens`, `token_type` and `merges`, or the first one missing."""
    return Chain.then(strings(file, "tokenizer.ggml.tokens"), lambda tokens: Chain.then(ints(file, "tokenizer.ggml.token_type"), lambda types: Chain.then(strings(file, "tokenizer.ggml.merges"), lambda merges: Success(value=Tables(tokens=tokens, types=types, merges=merges)))))


def tokenizer(file):
    """The file's tokenizer, from its own tables."""
    return Chain.then(tables(file), lambda t: GgufVocab.qwen_from_vocabulary(t.tokens, t.types, t.merges))


def eos_of(file):
    """`tokenizer.ggml.eos_token_id`: the end-of-turn token the file declares."""
    return Chain.then(Reader.field(file, "tokenizer.ggml.eos_token_id"), lambda field: Reader.field_int(field))


def open(path):
    """The session over the GGUF at `path`, or the startup error naming what it lacks."""
    return Chain.then(Qwen3_5Gguf.open_file(path), lambda source: Chain.then(tokenizer(source.file), lambda tok: Chain.then(eos_of(source.file), lambda eos: Chain.then(Qwen3_5Gguf.load(source), lambda model: started(model, tok, eos)))))


def started(model, tok, eos):
    return Chain.then(Qwen3_5Cache.empty(model.config, model.shapes), lambda cache: Success(value=Session(model=model, tok=tok, eos=eos, cache=cache, seen=[], logits=[])))


# --- one step -------------------------------------------------------------

def extends(seen, history):
    """Whether `history` begins with `seen`, so the cache can continue from it."""
    return Lists.count(seen) <= Lists.count(history) and fold(range(Lists.count(seen)), True, lambda same, i: same and unwrap_or(listGet(seen, i), -1) == unwrap_or(listGet(history, i), -2))


def fresh(session):
    """The session over an empty cache."""
    return Chain.then(Qwen3_5Cache.empty(session.model.config, session.model.shapes), lambda cache: Success(value=Session(model=session.model, tok=session.tok, eos=session.eos, cache=cache, seen=[], logits=[])))


def resumed(session, history):
    """The session the history can continue from: this one, or a fresh one."""
    return Success(value=session) if extends(session.seen, history) else fresh(session)


def advanced(session, history, sampling):
    """One sampled token after `history`, and the session holding its cache."""
    return Chain.then(resumed(session, history), lambda base: Chain.then(fed(base, drop(history, Lists.count(base.seen))), lambda ready: Chain.then(Sampler.one_token(ready.logits, history, sampling, ready.eos), lambda token: Success(value=Inference.Stepped(token=token, model=ready)))))


def drop(items, n):
    return toList(map(range(satSub(Lists.count(items), n)), lambda i: unwrap_or(listGet(items, satAdd(n, i)), -1)))


def fed(session, tokens):
    """The session after the cache has taken `tokens`, with the logits of the last."""
    match listGet(tokens, 0):
        case Error():
            return Success(value=session) if Lists.count(session.logits) > 0 else Error(message="step: the history is empty")
        case Success(value=first):
            return Chain.then(decoded(session.model, session.cache, first), lambda start: Chain.then(fold(range(satSub(Lists.count(tokens), 1)), Success(value=start), lambda acc, i: Chain.then(acc, lambda step: decoded(session.model, step.cache, unwrap_or(listGet(tokens, satAdd(i, 1)), -1)))), lambda last: hosted(session, tokens, last)))


def decoded(model, cache, token):
    return Chain.then(Qwen3_5Model.decode(model, cache, token), lambda next: Success(value=Fed(cache=next.walk.cache, logits=next.logits)))


def hosted(session, tokens, last):
    """The session with the cache advanced over `tokens` and the last logits read back."""
    match fromGpu(last.logits):
        case Success(value=values):
            return Success(value=Session(model=session.model, tok=session.tok, eos=session.eos, cache=last.cache, seen=listConcat(session.seen, tokens), logits=values))
        case Error(message=message):
            return Error(message=f"step: {toString(message)}")
