# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=Completions
"""One assistant turn: tokenize, sample, stream through the tool parser.

The loop is the whole of [LLM-API-OLLAMA]'s generation: the prompt's tokens go
to the `Generator` effect, each token comes back and is decoded, and the turn's
text so far goes straight into the Qwen tool parser, so a caller sees free text,
reasoning and `tool_calls` in the order the model produced them
([LLM-TOOL-PARSE]). A turn whose call does not satisfy the advertised tool's
schema is an error naming the violation: a harness never receives invented
arguments ([LLM-TOOL-REPAIR]).

The parser is re-entered from its public `start`/`feed`/`parse` entry points
with the turn's whole text, because its incremental state is not part of that
API; a turn is short enough that the replay is free, and the pieces it has
already emitted are dropped from each replay by length.
"""

from qwen_server import Inference
from qwen_server import Lists
from qwen import ToolParser

__all__ = ["TEXT", "THINKING", "complete", "pieces_text"]

TEXT = "text"
THINKING = "thinking"


class Piece:
    """One decoded token's text, as free text or as reasoning."""
    kind: str
    text: str


class Turn:
    """A finished assistant turn: what it said, what it called, and what it cost."""
    pieces: list[Piece]
    calls: list[ToolParser.Call]
    prompt_tokens: int
    eval_tokens: int
    reason: str


class Draft[M]:
    """The turn in progress: the handle to step from, its raw text, its history and what it has emitted."""
    model: M
    prompt_tokens: int
    tools: Json
    raw: str
    history: list[int]
    pieces: list[Piece]
    calls: list[ToolParser.Call]
    text_length: int
    think_length: int
    done: bool


def complete(model, prompt, sampling, tools):
    """Run one turn over `prompt`, or report why the model could not produce it."""
    match perform(Inference.Generator.encode(model, prompt)):
        case Success(value=tokens):
            return bounded(model, tokens, sampling, tools)
        case Error(message=message):
            return Error(message=message)


def bounded(model, tokens, sampling, tools):
    """A turn for a prompt that fits the context, and an error for one that does not."""
    room = satSub(sampling.num_ctx, Lists.count(tokens))
    if Lists.count(tokens) > sampling.num_ctx:
        return Error(message=f"prompt is {Lists.count(tokens)} tokens, over num_ctx {sampling.num_ctx}; a prompt is never truncated")
    elif room == 0:
        return Error(message=f"prompt is {Lists.count(tokens)} tokens and fills num_ctx {sampling.num_ctx}; no room to generate")
    else:
        start = Draft(model=model, prompt_tokens=Lists.count(tokens), tools=tools, raw="", history=tokens, pieces=[], calls=[], text_length=0, think_length=0, done=False)
        return run(start, sampling, budget(sampling, room))


def budget(sampling, room):
    """The turn's token budget: `num_predict`, never past the context ceiling."""
    return sampling.num_predict if sampling.num_predict < room else room


def run(start, sampling, limit):
    """Fold one sampling step per token, stopping early once the turn is done."""
    match fold(range(limit), Success(value=start), lambda result, _step: next_step(result, sampling)):
        case Success(value=draft):
            return finished_turn(draft)
        case Error(message=message):
            return Error(message=message)


def continued(result, step):
    """The next step's answer on success, or the reason this step already
    ended the turn."""
    match result:
        case Success(value=value):
            return step(value)
        case Error(message=message):
            return Error(message=message)


def next_step(result, sampling):
    """One step of the fold, or the failure that already ended the turn."""
    return continued(result, lambda draft: Success(value=draft) if draft.done else one_step(draft, sampling))


def one_step(draft, sampling):
    """Sample one token, keep the handle it leaves, and add the token to the turn."""
    return continued(perform(Inference.Generator.step(draft.model, draft.history, sampling)), lambda stepped: consumed(update(draft, model=stepped.model), stepped.token))


def consumed(draft, token):
    """A turn that ends at the model's end-of-turn token, or one that keeps going."""
    return continued(perform(Inference.Generator.eos(draft.model)), lambda eos: Success(value=update(draft, done=True)) if token == eos else decoded(draft, token))


def decoded(draft, token):
    """Feed one decoded token into the tool parser."""
    return continued(perform(Inference.Generator.decode(draft.model, token)), lambda piece: fed(draft, token, piece))


def parses(tools):
    """Whether this turn's text is tool syntax: an `/api/generate` turn carries none."""
    match tools:
        case JsonNull:
            return False
        case _:
            return True


def fed(draft, token, piece):
    """The draft with the parser's newest step folded in, or with `piece` as plain text."""
    grown = update(draft, raw=draft.raw + piece, history=listAppend(draft.history, token))
    if parses(draft.tools):
        return fed_parsed(grown)
    else:
        return Success(value=update(grown, pieces=listConcat(grown.pieces, kept(piece, TEXT))))


def fed_parsed(draft):
    """The draft over the parser's newest read of its whole text."""
    return continued(ToolParser.feed(ToolParser.start(draft.tools), draft.raw), lambda step: grown_pieces(draft, step))


def grown_pieces(draft, step):
    """The draft over one parser step's new text, reasoning and calls."""
    text = drop(step.text, draft.text_length)
    think = drop(step.think, draft.think_length)
    return continued(after_calls(step.calls, Lists.count(draft.calls)), lambda fresh: Success(value=update(
        draft,
        pieces=listConcat(draft.pieces, listConcat(kept(think, THINKING), kept(text, TEXT))),
        calls=listConcat(draft.calls, fresh),
        text_length=len(step.text),
        think_length=len(step.think),
    )))


def pieces_text(turn, kind):
    """The turn's pieces of one kind, joined in order."""
    return join(mapList(filterList(turn.pieces, lambda piece: piece.kind == kind), lambda piece: piece.text), "")


def kept(text, kind):
    """One piece for a non-empty `text`, and nothing for an empty one."""
    return [] if text == "" else [Piece(kind=kind, text=text)]


def after_calls(calls, seen):
    """The calls after the first `seen` of `calls`, or the gap that made one unreadable."""
    return collected(calls, seen, 0, [])


def collected(calls, seen, at, kept_calls):
    """Collect call `seen + at` onward until the parse's call list ends."""
    match listGet(calls, satAdd(seen, at)):
        case Success(value=call):
            return collected(calls, seen, satAdd(at, 1), listAppend(kept_calls, call))
        case Error(message=message):
            return Success(value=kept_calls) if satAdd(seen, at) >= Lists.count(calls) else Error(message=message)


def finished_turn(draft):
    """Close the parser, reject an invalid call, and report the turn."""
    if not parses(draft.tools):
        return calls_checked(draft)
    else:
        return closed(draft)


def closed(draft):
    """The turn over the parser's closed read of the turn's whole text."""
    return continued(ToolParser.parse(draft.tools, draft.raw), lambda step: settled(draft, step))


def settled(draft, step):
    """The turn over the closed parse, unless one of its calls failed its schema."""
    return continued(after_calls(step.calls, Lists.count(draft.calls)), lambda fresh: calls_checked(update(draft, pieces=listConcat(draft.pieces, listConcat(kept(step.think, THINKING), kept(step.text, TEXT))), calls=listConcat(draft.calls, fresh))))


def calls_checked(draft):
    """The finished turn, or an error naming the call that failed its tool's schema."""
    match first_failed(draft.calls):
        case Success(value=call):
            return Error(message=tool_failure(call))
        case Error():
            return Success(value=Turn(pieces=draft.pieces, calls=draft.calls, prompt_tokens=draft.prompt_tokens, eval_tokens=satSub(Lists.count(draft.history), draft.prompt_tokens), reason=termination(draft)))


def first_failed(calls):
    """The first call carrying a tool error, or an error when every call is valid."""
    match listGet(filterList(calls, lambda call: call.error != ""), 0):
        case Success(value=call):
            return Success(value=call)
        case Error(message=message):
            return Error(message=message)


def tool_failure(call):
    """The message for a call the advertised tools do not accept."""
    match ToolParser.error_field(call.error, "detail"):
        case Success(value=detail):
            return f"tool call {call.name}: {detail}"
        case Error(message=message):
            return f"tool call {call.name}: {call.error} ({message})"


def termination(draft):
    """Why the turn ended, in the words both protocols use."""
    return "stop" if draft.done else "length"
