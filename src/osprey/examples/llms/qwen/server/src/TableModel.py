# osprey: flavor=python
# osprey: namespace=qwen_server
# osprey: module=TableModel
"""The fixture model behind `Generator`: twelve-token context counts over a transcript.

It is the CI stand-in for the real weights (`RealModel.py`), which the server
installs when started with a GGUF path; every route is exercised against a model
that is real but tiny. Its handle
(`Model`) carries the vocabulary and the token ids of `CORPUS`, a transcript in
the served chat framing: a step counts, for the twelve-token context at the end of
the history, how often each token followed that context in the corpus, and asks
`@osprey/transformers`' `generate` for one token from those counts. Nothing here
is a canned reply — the text a turn produces is the model's own continuation of
its prompt, and `tests/model_reference.cpython` recomputes that continuation
independently from the same corpus. The step itself is `Sampler.one_token` over the
counts, the same sampler the real weights use.

A context the corpus never showed falls back to a seeded row, so a prompt
outside the training text still yields a deterministic token rather than an
empty or zero answer. Ties break by token id, as `Math.argmax` breaks them, so
the compiled model and the CPython reference agree exactly.

Every integer operation here is one of the total helpers
([BUILTIN-TOTAL-ARITH]): a server has no `Arith` policy to consult, and a
wrapped hash or a clamped count is the definition rather than a fault.
"""

from qwen_server import Inference
from qwen_server import Lists
from qwen_server import Sampler

__all__ = ["CORPUS", "Table", "installed"]

WINDOW = 12
EOS_CHAR = "~"
SEEDED_ROW = 1024

CORPUS = """<|im_start|>system
# Tools

You have access to the following functions:

<tools>
{"function": {"description": "Read one file from the workspace", "name": "read_file", "parameters": {"properties": {"path": {"type": "string"}}, "required": ["path"], "type": "object"}}, "type": "function"}
{"function": {"description": "Run one test suite", "name": "run_tests", "parameters": {"properties": {"suite": {"type": "string"}}, "required": ["suite"], "type": "object"}}, "type": "function"}
</tools>

If you choose to call a function ONLY reply in the following format with NO suffix:

<tool_call>
<function=example_function_name>
<parameter=example_parameter_1>
value_1
</parameter>
<parameter=example_parameter_2>
This is the value for the second parameter
that can span
multiple lines
</parameter>
</function>
</tool_call>

<IMPORTANT>
Reminder:
- Function calls MUST follow the specified format: an inner <function=...></function> block must be nested within <tool_call></tool_call> XML tags
- Required parameters MUST be specified
- You may provide optional reasoning for your function call in natural language BEFORE the function call, but NOT after
- If there is no function call available, answer the question like normal with your current knowledge and do not tell the user about function calls
</IMPORTANT><|im_end|>
<|im_start|>user
open the readme<|im_end|>
<|im_start|>assistant
<tool_call>
<function=read_file>
<parameter=path>
README.md
</parameter>
</function>
</tool_call>
~
<|im_start|>user
<tool_response>
{"status": "ok", "tests": 12}
</tool_response><|im_end|>
<|im_start|>assistant
<tool_call>
<function=read_file>
<parameter=path>
README.md
</parameter>
</function>
</tool_call>
~
<|im_start|>user
run the unit tests<|im_end|>
<|im_start|>assistant
<tool_call>
<function=run_tests>
<parameter=suite>
unit
</parameter>
</function>
</tool_call>
~
<|im_start|>user
open the readme<|im_end|>
<|im_start|>assistant
<tool_call>
<function=read_file>
<parameter=path>
README.md
</parameter>
</function>
</tool_call>
~
<|im_start|>user
<tool_response>
{"status": "ok", "tests": 12}
</tool_response><|im_end|>
<|im_start|>assistant
<tool_call>
<function=read_file>
<parameter=path>
README.md
</parameter>
</function>
</tool_call>
~
<|im_start|>user
run the unit tests<|im_end|>
<|im_start|>assistant
<tool_call>
<function=run_tests>
<parameter=suite>
unit
</parameter>
</function>
</tool_call>
~
"""
"""The transcript the fixture counts twelve-token contexts over, in the served chat framing.
Its system turn is the template's own tools turn (`src/Prompts.py`), because a request that
advertises tools renders those bytes and the fixture's vocabulary is the transcript's; the
assistant turns end at `~`, the fixture's end-of-turn token ([LLM-TOOL-PARSE])."""


class Model:
    """The fixture model's whole state: its vocabulary and its tokenized transcript."""
    vocab: list[int]
    ids: list[int]
    corpus: list[int]
    eos: int


def installed():
    """The model this server serves; Phase 3 returns the real weights' session here."""
    return fixture(CORPUS)


def fixture(corpus):
    """The fixture model for `corpus`, or the tokenizer error its own text produced."""
    vocab = vocabulary(corpus)
    match tokenize(token_ids(vocab), corpus):
        case Success(value=tokens):
            return Success(value=Model(vocab=vocab, ids=token_ids(vocab), corpus=tokens, eos=len(vocab)))
        case Error(message=message):
            return Error(message=message)


@handler(Inference.Generator)
class Table:
    """The `Generator` handler: a byte tokenizer over the model's vocabulary."""

    def encode(model, text):
        return tokenize(model.ids, text)

    def decode(model, token):
        return detokenize(model.vocab, model.eos, token)

    def eos(model):
        return model.eos

    def step(model, history, sampling):
        return andThen(Sampler.one_token(logits(model, history, sampling), history, sampling, model.eos), lambda token: Success(value=Inference.Stepped(token=token, model=model)))


# --- the vocabulary -------------------------------------------------------

def bytes_of(text):
    """The UTF-8 bytes of `text`, in order."""
    return toList(map(range(len(text)), lambda at: unwrap_or(byteAt(text, at), -1)))


def marker():
    """The end-of-turn marker's byte."""
    return unwrap_or(byteAt(EOS_CHAR, 0), -1)


def distinct(values):
    """`values` without repeats, in first-seen order."""
    return foldList(values, [], lambda kept, value: kept if listContains(kept, value) else listAppend(kept, value))


def ascending(values):
    """`values` in ascending order."""
    return listSortBy(values, lambda left, right: -1 if left < right else (1 if left > right else 0))


def vocabulary(corpus):
    """Every byte the corpus uses except the end-of-turn marker, ascending."""
    return ascending(filterList(distinct(bytes_of(corpus)), lambda byte: byte != marker()))


def token_ids(vocab):
    """A token id for each byte value: -1 outside the vocabulary, `len(vocab)` for the marker."""
    return toList(map(range(256), lambda byte: token_of(vocab, byte)))


def token_of(vocab, byte):
    """`byte`'s token id in `vocab`."""
    return len(vocab) if byte == marker() else index_of(vocab, byte, 0)


def index_of(vocab, byte, at):
    """The index of `byte` in `vocab` from `at`, or -1."""
    match listGet(vocab, at):
        case Success(value=value):
            return at if value == byte else index_of(vocab, byte, satAdd(at, 1))
        case Error():
            return -1


def tokenize(ids, text):
    """The token ids of `text`, or an error naming the first byte outside the vocabulary.
    A fold, not a recursion per byte: an agent's prompt runs to tens of thousands of
    bytes, and a frame per byte overflowed the fiber's stack at about 3,000 ([LLM-API-LIMIT]
    rejects a prompt by its count, never by crashing)."""
    bytes_ = bytes_of(text)
    tokens = mapList(bytes_, lambda byte: unwrap_or(listGet(ids, byte), -1))
    at = first_missing(tokens)
    if at < 0:
        return Success(value=tokens)
    else:
        return missing(unwrap_or(listGet(bytes_, at), -1), at)


def first_missing(tokens):
    """The offset of the first byte the vocabulary does not hold (its token id is -1), or -1."""
    return fold(range(len(tokens)), -1, lambda found, at: found if found >= 0 else (at if unwrap_or(listGet(tokens, at), 0) < 0 else -1))


def missing(byte, at):
    """The tokenizer's error for a byte the fixture vocabulary does not hold."""
    return Error(message=f"tokenizer: byte {byte} at offset {at} is not in the fixture model's vocabulary")


def detokenize(vocab, eos, token):
    """The one-character text of `token`; the end-of-turn token has no text."""
    if token == eos:
        return Success(value="")
    else:
        return fromCodePoint(unwrap_or(listGet(vocab, token), -1))


# --- the context counts ---------------------------------------------------

def zeros(eos):
    """One zero count per token id."""
    return toList(map(range(satAdd(eos, 1)), lambda _token: 0))


def bumped(counts, successor):
    """`counts` with one more occurrence of `successor`."""
    return toList(map(range(Lists.count(counts)), lambda at: satAdd(count_at(counts, at), 1) if at == successor else count_at(counts, at)))


def count_at(counts, at):
    """The count recorded at `at`, and zero where the row has none."""
    return unwrap_or(listGet(counts, at), 0)


def total(counts):
    """The sum of one counts row."""
    return foldList(counts, 0, lambda sum_so_far, count: satAdd(sum_so_far, count))


def row(model, window):
    """How often each token followed `window` in the corpus, one count per token id."""
    return fold(range(Lists.count(model.corpus)), zeros(model.eos), lambda counts, at: bumped(counts, successor(model.corpus, at)) if follows(model.corpus, at, window) else counts)


def follows(corpus, at, window):
    """Whether the `WINDOW` tokens ending at `at` are `window`, which is nearest-first."""
    return at >= WINDOW and fold(range(WINDOW), True, lambda same, back: same and unwrap_or(listGet(corpus, satSub(satSub(at, 1), back)), -1) == unwrap_or(listGet(window, back), -2))


def successor(corpus, at):
    """The token at `at`; the end-of-turn token where the corpus ends."""
    return unwrap_or(listGet(corpus, at), -1)


def newest(history):
    """The newest `WINDOW` tokens of `history`, nearest first."""
    return toList(map(range(WINDOW), lambda back: unwrap_or(listGet(history, satSub(satSub(Lists.count(history), 1), back)), -1)))


# --- one sampling step ----------------------------------------------------

def logits(model, history, sampling):
    """One row of scores for the next token after `history`."""
    if Lists.count(history) < WINDOW:
        return seeded(model, history, sampling)
    else:
        counted = row(model, newest(history))
        return seeded(model, history, sampling) if total(counted) == 0 else mapList(counted, lambda count: toFloat32(count))


def seeded(model, history, sampling):
    """The deterministic row for a context the corpus never showed."""
    anchor = foldList(newest(history), sampling.seed, lambda state, token: satAdd(wrapMul(state, 131), token)) if Lists.count(history) >= WINDOW else sampling.seed
    return toList(map(range(satAdd(model.eos, 1)), lambda token: toFloat32(bias(anchor, token))))


def bias(anchor, token):
    """A non-negative score below `SEEDED_ROW` from the context anchor and the token."""
    return residue_seeded(satAdd(wrapMul(satAdd(anchor, token), 1103515245), 12345))


def residue_seeded(value):
    """`value` modulo `SEEDED_ROW`, never negative."""
    return satSub(abs(value), wrapMul(intDiv(abs(value), SEEDED_ROW), SEEDED_ROW))
