# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=RealModel
"""The real `qwen3_5` weights behind `Generator`: one GGUF mapped at startup, its cache
kept across requests.

`open` parses the file, builds the tokenizer from its own `tokenizer.ggml.*` tables
(`Qwen3_5Vocab`), reads the end-of-turn id the file declares and maps every tensor
through `Qwen3_5Gguf`; a missing or invalid file is the startup error, never a served
500. A `Session` is the handle: the model, the tokenizer, and a one-slot channel holding
the `State` of the cache: the tokens it has seen, the logits after the last of them and
the checkpoints kept behind it. A step takes the state out of the slot, feeds only the
tokens the history adds past what the cache holds in one prefill ([LLM-PREFILL]), and
puts the state back, so the next request starts where this one ended; the
slot is the one place the server mutates, and a request waits on it for the request
before ([LLM-MODEL-CACHE], [LLM-API-QUEUE]). The KV cache could be cut back but the
recurrent state cannot, so when a history stops extending the cache (an agent that
rewrote its last turn, or a history that carries the answer without its reasoning) the
step continues from the newest checkpoint whose tokens the history still begins with,
and only from a fresh cache when none does; a checkpoint is taken after every prompt
the cache takes in and the last `CHECKPOINTS` are kept ([LLM-CACHE-CHECKPOINT]). Each
prompt prints how many tokens it continued from, so a log shows the reuse. Sampling is
`Sampler.one_token` over the host copy of the logits, as for the fixture table, so both
handlers obey the same request options ([LLM-TOKENIZER], [LLM-WEIGHTS-MAPPED]).
"""

from qwen import Qwen3_5Cache, Qwen3_5Gguf, Qwen3_5Model, Qwen3_5Vocab
from qwen_server import Inference
from qwen_server import Lists
from qwen_server import Sampler
from tokenizers import Tokenizer
from tokenizers.Tokenizer import qwen_decode

__all__ = ["CHECKPOINTS", "Mark", "State", "Session", "Real", "open"]

CHECKPOINTS = 4
"""How many prompt checkpoints a session keeps behind its live cache."""


class Mark:
    """A checkpoint: the cache after `seen`, and the logits it left."""
    seen: list[int]
    cache: Qwen3_5Cache.Cache
    logits: list[float32]


class State:
    """Where the cache stands, and the checkpoints behind it."""
    cache: Qwen3_5Cache.Cache
    seen: list[int]
    logits: list[float32]
    marks: list[Mark]


class Session:
    """The installed model, its tokenizer, and the slot holding the cache's state."""
    model: Qwen3_5Model.Model
    tok: Tokenizer.QwenTokenizer
    eos: int
    slot: Channel[State]


class Advance:
    """One sampled token and the state the step left."""
    token: int
    left: State


class Fed:
    """The cache after some tokens and the logits the last one produced."""
    cache: Qwen3_5Cache.Cache
    logits: GpuBuffer[float32]


@handler(Inference.Generator)
class Real:
    """The `Generator` handler over a `Session`."""

    def encode(session, text):
        return typed(Qwen3_5Vocab.ids_of(session.tok, text))

    def decode(session, token):
        return typed(qwen_decode(session.tok, [token], False))

    def eos(session):
        return session.eos

    def step(session, history, sampling):
        return typed(exchanged(session, recv(session.slot), history, sampling))


def typed(result):
    """A string-error result as the `Error`-typed result the effect declares."""
    match result:
        case Success(value=value):
            return Success(value=value)
        case Error(message=message):
            return Error(message=message)


def exchanged(session, state, history, sampling):
    """One step from `state`, putting the state it leaves back in the slot, or `state`
    itself after a failure, so the slot is never left empty."""
    match advanced(session.model, session.eos, state, history, sampling):
        case Success(value=advance):
            send(session.slot, advance.left)
            return Success(value=Inference.Stepped(token=advance.token, model=same(session)))
        case Error(message=message):
            send(session.slot, state)
            return Error(message=message)


def same(session):
    """The handle again. In this project a handler arm that answers with its own
    parameter, or an `update` of it, is rejected by the compiler as an unlocated
    `cannot unify int with float` (a small program doing the same compiles; open in
    plan 0049), so the step hands back a copy, which shares the slot."""
    return Session(model=session.model, tok=session.tok, eos=session.eos, slot=session.slot)


# --- opening the file -----------------------------------------------------

def open(path):
    """The session over the GGUF at `path`, or the startup error naming what it lacks."""
    return andThen(Qwen3_5Gguf.open_file(path), lambda source: andThen(Qwen3_5Vocab.tokenizer(source.file), lambda tok: andThen(Qwen3_5Vocab.eos_of(source.file), lambda eos: andThen(Qwen3_5Gguf.load(source), lambda model: started(model, tok, eos)))))


def started(model, tok, eos):
    return andThen(Qwen3_5Cache.empty(model.config, model.shapes), lambda cache: Success(value=Session(model=model, tok=tok, eos=eos, slot=filled(State(cache=cache, seen=[], logits=[], marks=[])))))


def filled(state):
    """A one-slot channel holding `state`."""
    slot = Channel(1)
    send(slot, state)
    return slot


# --- one step -------------------------------------------------------------

def extends(seen, history):
    """Whether `history` begins with `seen`, so the cache can continue from it."""
    return Lists.count(seen) <= Lists.count(history) and fold(range(Lists.count(seen)), True, lambda same, i: same and unwrap_or(listGet(seen, i), -1) == unwrap_or(listGet(history, i), -2))


def at(state, cache, seen, logits):
    """`state` standing at `cache`, keeping its checkpoints."""
    return State(cache=cache, seen=seen, logits=logits, marks=state.marks)


def fresh(model, state):
    """The state over an empty cache."""
    return andThen(Qwen3_5Cache.empty(model.config, model.shapes), lambda cache: Success(value=at(state, cache, [], [])))


def from_mark(model, state, history):
    """The state at the newest checkpoint the history still begins with, else a fresh one."""
    match listGet(filterList(state.marks, lambda mark: extends(mark.seen, history)), 0):
        case Success(value=mark):
            return Success(value=at(state, mark.cache, mark.seen, mark.logits))
        case Error():
            print(f"cache: no checkpoint of {Lists.count(state.marks)} matches; shared with the newest: {shared(state.marks, history)}")
            return fresh(model, state)


def shared(marks, history):
    """How many leading tokens the newest checkpoint and `history` have in common."""
    match listGet(marks, 0):
        case Success(value=mark):
            return fold(range(Lists.count(mark.seen)), 0, lambda n, i: satAdd(n, 1) if n == i and unwrap_or(listGet(mark.seen, i), -1) == unwrap_or(listGet(history, i), -2) else n)
        case Error():
            return 0


def resumed(model, state, history):
    """The state the history can continue from: this one, a checkpoint, or a fresh one."""
    return Success(value=state) if extends(state.seen, history) else from_mark(model, state, history)


def advanced(model, eos, state, history, sampling):
    """One sampled token after `history`, and the state holding its cache."""
    return andThen(resumed(model, state, history), lambda base: andThen(fed(model, base, drop(history, Lists.count(base.seen))), lambda ready: andThen(Sampler.one_token(ready.logits, history, sampling, eos), lambda token: Success(value=Advance(token=token, left=marked(ready, Lists.count(base.seen)))))))


def marked(state, before):
    """`state` with a checkpoint taken when this step took in more than one token (a
    prompt), newest first, keeping `CHECKPOINTS` of them."""
    if Lists.count(state.seen) - before > 1:
        print(f"cache: continued from {before} of {Lists.count(state.seen)} tokens")
        return State(cache=state.cache, seen=state.seen, logits=state.logits, marks=take(listConcat([Mark(seen=state.seen, cache=state.cache, logits=state.logits)], state.marks), CHECKPOINTS))
    else:
        return state


def take(items, n):
    """The first `n` of `items`."""
    return foldList(items, [], lambda kept, item: kept if Lists.count(kept) >= n else listAppend(kept, item))


def drop(items, n):
    """`items` after its first `n`."""
    return toList(map(range(satSub(Lists.count(items), n)), lambda i: unwrap_or(listGet(items, satAdd(n, i)), -1)))


def fed(model, state, tokens):
    """The state after the cache has taken `tokens` in one prefill ([LLM-PREFILL]),
    with the logits of the last."""
    if Lists.count(tokens) == 0:
        return Success(value=state) if Lists.count(state.logits) > 0 else Error(message="step: the history is empty")
    else:
        return andThen(Qwen3_5Model.prefill(model, state.cache, tokens), lambda next: hosted(state, tokens, Fed(cache=next.walk.cache, logits=next.logits)))


def hosted(state, tokens, last):
    """The state with the cache advanced over `tokens` and the last logits read back."""
    match fromGpu(last.logits):
        case Success(value=values):
            return Success(value=at(state, last.cache, listConcat(state.seen, tokens), values))
        case Error(message=message):
            return Error(message=f"step: {toString(message)}")
