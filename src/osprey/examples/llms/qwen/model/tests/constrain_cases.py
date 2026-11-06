# osprey: flavor=python
# osprey: namespace=qwen_tests
# osprey: module=ConstrainCases
"""The constrained-sampling mask against the 200 corpus cases: every target path
admitted, adversarial draws that still end in a schema-valid call, the unmasked
controls and the logits processor ([LLM-TOOL-CONSTRAIN])."""
from testing import Check
from qwen_tools_expected import ExpectedTools
from qwen import ToolConstrain, ToolJsonScan, ToolParser, ToolSchema
from transformers.Math import argmax
from qwen_tests.ModelCases import flatten, refusal
from qwen_tests.ToolSupport import decoded, finished, first_call, indices_of, ints_of, parsed_turn, record_at, records_of, texts_of, tool_list, turn_fingerprint, verdict_text, want_fingerprint

__all__ = ["constrain_cases", "control_cases", "logits_processor_cases"]


# Masked sampling: a schema-invalid call is unsamplable.
def constrain_cases():
    """Every Phase 0 case, sampled twice: towards its own tokens, and adversarially."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return listConcat(flatten(mapList(indices_of(records), lambda i: constrain_case(tools, record_at(records, i), i))), constrain_enum_cases())


def constrain_enum_cases():
    tools = decoded("[{\"type\":\"function\",\"function\":{\"name\":\"set_mode\",\"parameters\":{\"type\":\"object\",\"properties\":{\"mode\":{\"type\":\"string\",\"enum\":[\"run\",\"review\"]}},\"required\":[\"mode\"],\"additionalProperties\":false}}}]")
    prefix = "<tool_call>\n<function=set_mode>\n<parameter=mode>\n"
    suffix = "\n</parameter>\n</function>\n</tool_call>"
    return [
        Check.equal("constrain_enum_run", completed_enum_call(tools, prefix + "run" + suffix), True),
        Check.equal("constrain_enum_review", completed_enum_call(tools, prefix + "review" + suffix), True),
        Check.equal("constrain_enum_prefix", completed_enum_call(tools, prefix + "r"), False),
        refusal("constrain_enum_undeclared", ToolConstrain.advance(ToolConstrain.start(tools), tools, prefix + "reject"), "constrain@value: not a declared member"),
    ]


def completed_enum_call(tools, text):
    match ToolConstrain.advance(ToolConstrain.start(tools), tools, text):
        case Success(value=machine):
            return ToolConstrain.complete(machine)
        case Error():
            return False


def case_tokens(record):
    """The fixture case's token texts."""
    return texts_of(unwrap_or(jsonField(record, "tokens"), JsonNull))


def case_constraint(tools, record):
    """The constraint a server installs: the case's tokens under `tools`."""
    return ToolConstrain.Constrain(tools, case_tokens(record), toFloat32(0.0) - toFloat32(exp(1000.0)))


def constrain_case(tools, record, i):
    tokens = case_tokens(record)
    constraint = case_constraint(tools, record)
    started = spelled(tokens, ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull)))
    return [
        Check.equal(f"target[{i}]", walked(tools, constraint, record, started), want_fingerprint(record)),
        Check.equal(f"adversarial[{i}]", sampled(tools, constraint, record, started), ""),
    ]


def spelled(tokens, ids):
    return join(mapList(ids, lambda id: unwrap_or(listGet(tokens, id), "")), "")


def machine_after(tools, text):
    match ToolConstrain.advance(ToolConstrain.start(tools), tools, text):
        case Success(value=machine):
            return machine
        case Error(_rejected):
            return ToolConstrain.start(tools)


def walked(tools, constraint, record, text):
    """Greedy towards the fixture's own tokens: the mask has to allow every one."""
    return walk(tools, constraint, record, machine_after(tools, text), ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), 0, text)


def walk(tools, constraint, record, machine, want, step, text):
    if ToolConstrain.complete(machine):
        return turn_fingerprint(parsed_turn(tools, text))
    elif step >= len(want):
        return f"stalled at {machine.stage} after {toString(step)} tokens"
    else:
        return walked_step(tools, constraint, record, machine, want, step, text)


def walked_step(tools, constraint, record, machine, want, step, text):
    token = unwrap_or(listGet(constraint.vocabulary, unwrap_or(listGet(want, step), 0)), "")
    match ToolConstrain.advance(machine, tools, token):
        case Success(value=advanced):
            return walk(tools, constraint, record, advanced, want, step + 1, text + token)
        case Error(message=message):
            return f"rejected {unwrap_or(jsonQuote(token), token)} at {toString(step)}: {message}"


def sampled(tools, constraint, record, text):
    """Seeded adversarial sampling over the corpus token, a seeded draw and every closing
    token: a forbidden candidate is scored highest, so an unmasked row corrupts the
    format at once, and the mask alone keeps the call schema-valid. Returns "" when the
    turn is one schema-valid call."""
    return sample(tools, constraint, record, ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull)), text, 0)


def sample(tools, constraint, record, ids, text, step):
    machine = machine_after(tools, text)
    if ToolConstrain.complete(machine):
        return sampled_verdict(tools, parsed_turn(tools, text))
    elif step > 200:
        return f"never completed after {toString(step)} at {machine.stage} depth={toString(len(machine.stack))} inner={inner_stage(machine)}"
    else:
        return sampled_step(tools, constraint, record, ids, text, step, machine)


def sampled_step(tools, constraint, record, ids, text, step, machine):
    """One adversarial step: a seeded draw is sampled whenever the mask admits it, so a
    corrupting token is what the run takes by default; when the mask forbids it, the
    corpus token keeps the run on its schema-valid path."""
    drawn = seeded_id(constraint, step, 0)
    corpus = unwrap_or(listGet(ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), step), 0 - 1)
    chosen = choose(constraint, machine, drawn, corpus)
    if chosen < 0:
        return f"stalled at {toString(step)} in {machine.stage} inner={inner_stage(machine)} key={inner_text(machine)} seen={join(machine.seen, ',')}"
    else:
        return sample(tools, constraint, record, listAppend(ids, chosen), text + token_at(constraint, chosen), step + 1)


def choose(constraint, machine, drawn, corpus):
    """The seeded draw when the mask admits it and it moves the grammar on, else the corpus
    token when the mask admits that, else whatever moves the grammar on. A draw the mask
    rejects is where the constraint bites; an admitted draw the run keeps."""
    if progresses_with(constraint, machine, drawn):
        return drawn
    elif allowed_from(constraint, machine, corpus):
        return corpus
    else:
        return first_progressing(constraint, machine, 0, 0 - 1)


def progresses_with(constraint, machine, id):
    if id < 0:
        return False
    else:
        return moved_on(constraint, machine, token_at(constraint, id))


def moved_on(constraint, machine, token):
    match ToolConstrain.advance(machine, constraint.tools, token):
        case Success(value=advanced):
            return progresses(machine, advanced)
        case Error(_rejected):
            return False


def seeded_id(constraint, step, k):
    return (step * 7919 + k * 104729) % len(constraint.vocabulary)


def first_progressing(constraint, machine, k, found):
    """The first allowed token that moves the grammar on: a new stage or a finished JSON
    value. It keeps a seeded source from idling inside a verbatim string argument."""
    if k >= len(constraint.vocabulary):
        return found if found >= 0 else first_allowed(constraint, machine, 0)
    else:
        return progressed(constraint, machine, k, found)


def progressed(constraint, machine, k, found):
    match ToolConstrain.advance(machine, constraint.tools, token_at(constraint, k)):
        case Success(value=advanced):
            if found < 0 and moves(machine, advanced):
                return k
            else:
                return first_progressing(constraint, machine, k + 1, found)
        case Error(_rejected):
            return first_progressing(constraint, machine, k + 1, found)


def progresses(machine, advanced):
    """Whether one token moved the grammar to a new stage, finished a JSON value or
    advanced a literal: what a draw has to do to be worth taking."""
    return advanced.stage != machine.stage or advanced.mode != machine.mode or len(advanced.stack) < len(machine.stack) or advanced.progress != machine.progress


def moves(machine, advanced):
    """Whether one token moved at all, including a step inside a JSON value, so a stuck run
    always has a deterministic token to take. Text growth alone is not a move: at a closed
    stage the first allowed token already continues the name or member."""
    return progresses(machine, advanced) or top_stage(advanced) != top_stage(machine)


def top_stage(machine):
    return foldList(machine.stack, "", lambda _acc, frame: frame.stage)


def inner_text(machine):
    """The key or literal text the JSON scan has accumulated: what a stall names."""
    return foldList(machine.stack, "", lambda _acc, frame: frame.text)


def inner_stage(machine):
    """The JSON scan's own stage at the top of its stack: what a failure message names."""
    return foldList(machine.stack, "", lambda _acc, frame: frame.stage)


def first_allowed(constraint, machine, k):
    if k >= len(constraint.vocabulary):
        return 0 - 1
    elif allowed_from(constraint, machine, k):
        return k
    else:
        return first_allowed(constraint, machine, k + 1)


def allowed_from(constraint, machine, id):
    if id < 0:
        return False
    else:
        return accepts(constraint, machine, token_at(constraint, id))


def accepts(constraint, machine, token):
    match ToolConstrain.advance(machine, constraint.tools, token):
        case Success(_advanced):
            return True
        case Error(_rejected):
            return False


def token_at(constraint, i):
    return unwrap_or(listGet(constraint.vocabulary, i), "")


def sampled_verdict(tools, step):
    """"" when the sampled turn is one schema-valid call, else what is wrong with it."""
    if len(step.calls) != 1:
        return f"{toString(len(step.calls))} calls ({step.text})"
    else:
        return call_verdict(tools, first_call(step))


def call_verdict(tools, call):
    if call.error != "":
        return f"tool error {call.error}"
    else:
        return verdict_text(ToolSchema.validate(ToolSchema.parameters(ToolSchema.tool_named(tools, call.name)), call.arguments))


def control_cases():
    """Clean controls: without the mask the same adversarial row corrupts the format."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return mapList([0, 1, 2, 7], lambda i: control_case(tools, record_at(records, i), i))


def control_case(tools, record, i):
    tokens = texts_of(unwrap_or(jsonField(record, "tokens"), JsonNull))
    started = spelled(tokens, ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull)))
    return Check.equal(f"control_unmasked[{i}]", unmasked(tools, constraint_of(tools, tokens), started, 0), "corrupted")


def constraint_of(tools, tokens):
    return ToolConstrain.Constrain(tools, tokens, toFloat32(0.0) - toFloat32(exp(1000.0)))


def unmasked(tools, constraint, text, step):
    """Sample an adversarial row with no mask: the turn stops being a parseable call."""
    if step >= 12:
        return "corrupted"
    else:
        return unmasked_step(tools, constraint, text, step)


def unmasked_step(tools, constraint, text, step):
    scores = mapList(toList(range(len(constraint.vocabulary))), lambda i: adversarial_draw(i, step))
    picked = argmax(scores)
    return unmasked(tools, constraint, text + token_at(constraint, picked), step + 1)


def adversarial_draw(i, step):
    return toFloat32((step * 7919 + i * 104729) % 1000) / toFloat32(1000.0)


# The logits-processor entry point itself, over a token history.
def logits_processor_cases():
    """`apply` and `mask` over a decoded history: the processor a server installs."""
    tools = tool_list()
    records = records_of(ExpectedTools.CASES)
    return flatten(mapList([0, 1, 2, 7], lambda i: processor_case(tools, record_at(records, i), i)))


def processor_case(tools, record, i):
    constraint = case_constraint(tools, record)
    ids = ints_of(unwrap_or(jsonField(record, "prefix"), JsonNull))
    machine = machine_after(tools, ToolConstrain.decode(constraint, ids))
    scores = mapList(toList(range(len(constraint.vocabulary))), lambda k: corpus_score(record, k))
    masked = ToolConstrain.apply(constraint, ids, scores)
    corpus = unwrap_or(listGet(ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), 0), 0)
    return [
        Check.equal(f"processor_pick[{i}]", argmax(masked), corpus),
        Check.equal(f"processor_mask[{i}]", toString(len(ToolConstrain.mask(constraint, machine, scores))), toString(len(scores))),
        Check.equal(f"processor_accepts[{i}]", ToolConstrain.accepts(constraint, ToolConstrain.decode(constraint, ids) + token_at(constraint, corpus)), True),
    ]


def corpus_score(record, k):
    """The corpus token outranks everything, so the processor has to keep it."""
    return toFloat32(1.0) if k == unwrap_or(listGet(ints_of(unwrap_or(jsonField(record, "want"), JsonNull)), 0), 0) else toFloat32(0.0)
