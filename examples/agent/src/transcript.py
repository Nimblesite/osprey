# osprey: flavor=python
# osprey: namespace=agent
# osprey: module=Transcript
"""What a conversation shows: the wire history read into display items, and the
model's Markdown read into typed blocks and inline runs, so every host draws the
same document with its own controls. Pure, and tested without a window.
"""
from pydantic_ai import JsonUtil as J
from pydantic_ai import Messages as M
from agent import Engine

__all__ = ["Item", "Step", "Block", "Span", "items_of", "blocks_of", "spans_of", "title_of", "step_title", "plain_item", "block"]


class Span:
    """One inline run: its text and emphasis (empty, `strong`, `em` or `code`)."""
    spanText: str
    spanKind: str


class Block:
    """One Markdown block: `paragraph`, `heading`, `bullet`, `numbered`, `quote`, `code` or `table`, with its level or number, its spans, for code its language and text, and for a table its rows of cells, the header first."""
    blockKind: str
    blockLevel: int
    blockSpans: list[Span]
    blockLanguage: str
    blockCode: str
    blockRows: list[list[list[Span]]]


class Step:
    """One tool call with its answer: the id, the tool, a one-line summary, the result and whether it has one yet."""
    stepId: str
    stepTool: str
    stepSummary: str
    stepOutput: str
    stepDone: bool
    stepFailed: bool


class Item:
    """One thing the transcript shows: a `user` message, an `answer`, a `thought`, or a group of `tools`."""
    itemKind: str
    itemKey: str
    itemText: str
    itemBlocks: list[Block]
    itemSteps: list[Step]


def plain_item(kind, key, text):
    return Item(itemKind=kind, itemKey=key, itemText=text, itemBlocks=[], itemSteps=[])


def answer_item(key, text):
    return Item(itemKind="answer", itemKey=key, itemText=text, itemBlocks=blocks_of(text), itemSteps=[])


def returns_of(messages):
    """Every tool's answer in a conversation, keyed by call id."""
    return foldList(M.requests(messages), {}, lambda acc, request: foldList(request.parts, acc, return_step))


def return_step(acc, part):
    match part:
        case M.ToolReturnPart(content=content, tool_call_id=tool_call_id):
            return mapSet(acc, tool_call_id, content)
        case M.RetryPromptPart(content=content, tool_call_id=tool_call_id):
            return mapSet(acc, tool_call_id, content)
        case _:
            return acc


def failed_result(output):
    return not startsWith(output, "exit 0") and (startsWith(output, "exit ") or startsWith(output, "failed with status") or startsWith(output, "The tool could not run"))


def step_of(returns, part):
    call_id = M.tool_call_id_of(part)
    output = unwrap_or(mapGet(returns, call_id), "")
    return Step(stepId=call_id, stepTool=M.tool_name_of(part), stepSummary=Engine.tool_summary(M.tool_name_of(part), M.args_as_json_str(part)), stepOutput=output, stepDone=mapContains(returns, call_id), stepFailed=failed_result(output))


def thought_text(part):
    match part:
        case M.ThinkingPart(content=content):
            return content
        case _:
            return ""


def response_items(returns, index, response):
    key = toString(index)
    thought = join(filterList(mapList(response.parts, thought_text), lambda t: t != ""), "\n\n")
    text = trim(M.text(response))
    calls = M.tool_calls(response)
    thoughts = [] if thought == "" else [plain_item("thought", "t" + key, thought)]
    answers = [] if text == "" else [answer_item("a" + key, text)]
    tools = [] if len(calls) == 0 else [Item(itemKind="tools", itemKey="s" + M.tool_call_id_of(unwrap_or(listGet(calls, 0), M.TextPart(content=""))), itemText="", itemBlocks=[], itemSteps=mapList(calls, lambda part: step_of(returns, part)))]
    return listConcat(listConcat(thoughts, answers), tools)


def user_text(part):
    match part:
        case M.UserPromptPart(content=content):
            return content
        case _:
            return ""


def request_items(index, request):
    texts = filterList(mapList(request.parts, user_text), lambda t: t != "")
    return [] if len(texts) == 0 else [plain_item("user", "u" + toString(index), join(texts, "\n\n"))]


def message_items(returns, messages, index):
    match listGet(messages, index):
        case Success(value=message):
            match message:
                case M.Request(request=request):
                    return request_items(index, request)
                case M.Response(response=response):
                    return response_items(returns, index, response)
        case Error(message=_m):
            return []


def merge_tools(acc, item):
    """Consecutive tool groups read as one activity, as `Ran 3 commands` does."""
    match listGet(acc, len(acc) - 1):
        case Success(value=last):
            if last.itemKind == "tools" and item.itemKind == "tools":
                merged = Item(itemKind="tools", itemKey=last.itemKey, itemText="", itemBlocks=[], itemSteps=listConcat(last.itemSteps, item.itemSteps))
                return listAppend(filterIndex(acc, len(acc) - 1), merged)
            else:
                return listAppend(acc, item)
        case Error(message=_m):
            return listAppend(acc, item)


def filterIndex(items, count):
    """The first `count` items."""
    return toList(map(range(count), lambda i: unwrap_or(listGet(items, i), unwrap_or(listGet(items, 0), plain_item("", "", "")))))


def items_of(history):
    """Everything the transcript shows, in order."""
    messages = unwrap_or(M.validate_messages(history), [])
    returns = returns_of(messages)
    flat = fold(range(len(messages)), [], lambda acc, index: listConcat(acc, message_items(returns, messages, index)))
    return foldList(flat, [], merge_tools)


def title_of(text):
    """A conversation's title: its first line, cut to fit a sidebar."""
    first = trim(unwrap_or(listGet(unwrap_or(split(text, "\n"), [text]), 0), text))
    return Engine.cut_text(first, 42) + "…" if length(first) > 42 else first


def step_title(steps):
    """The disclosure's words: what was done, in the past tense."""
    count = len(steps)
    commands = len(filterList(steps, lambda s: s.stepTool == "run_command"))
    edits = len(filterList(steps, lambda s: s.stepTool == "write_file"))
    reads = count - commands - edits
    parts = filterList([count_words(edits, "Edited", "file", "files"), count_words(commands, "Ran", "command", "commands"), count_words(reads, "Read", "file", "files")], lambda t: t != "")
    return join(parts, ", ")


def count_words(count, verb, one, many):
    return "" if count == 0 else verb + " " + toString(count) + " " + (one if count == 1 else many)


# Markdown: the subset models write. A line starts a block; a fence runs to its closing fence.

class Reading:
    """The blocks read so far, an open paragraph's lines and an open fence."""
    readBlocks: list[Block]
    readLines: list[str]
    inFence: bool
    fenceLanguage: str
    fenceLines: list[str]


def block(kind, level, text):
    return Block(blockKind=kind, blockLevel=level, blockSpans=spans_of(text), blockLanguage="", blockCode="", blockRows=[])


def table_row(line):
    """The cells of a `| a | b |` line, without its outer bars."""
    inner = trim(line)
    opened = drop(inner, 1) if startsWith(inner, "|") else inner
    closed = take(opened, length(opened) - 1) if endsWith(opened, "|") else opened
    return mapList(unwrap_or(split(closed, "|"), [closed]), lambda cell: spans_of(trim(cell)))


def is_rule(line):
    """Whether a line is a table's `|---|:--|` rule: only bars, dashes, colons and spaces."""
    return startsWith(line, "|") and contains(line, "-") and fold(range(length(line)), True, lambda ok, i: ok and contains("|-: ", unwrap_or(substring(line, i, i + 1), "x")))


def is_table(lines):
    """Whether an open run of lines is a Markdown table: every line barred, the second a rule."""
    return len(lines) >= 2 and is_rule(unwrap_or(listGet(lines, 1), "")) and len(filterList(lines, lambda l: startsWith(l, "|"))) == len(lines)


def table_block(lines):
    rows = filterList(lines, lambda l: not is_rule(l))
    return Block(blockKind="table", blockLevel=0, blockSpans=[], blockLanguage="", blockCode="", blockRows=mapList(rows, table_row))


def flush(reading):
    """Closes an open paragraph or table."""
    if len(reading.readLines) == 0:
        return reading
    elif is_table(reading.readLines):
        return Reading(readBlocks=listAppend(reading.readBlocks, table_block(reading.readLines)), readLines=[], inFence=False, fenceLanguage="", fenceLines=[])
    else:
        return Reading(readBlocks=listAppend(reading.readBlocks, block("paragraph", 0, join(reading.readLines, " "))), readLines=[], inFence=False, fenceLanguage="", fenceLines=[])


def adding(reading, item):
    closed = flush(reading)
    return Reading(readBlocks=listAppend(closed.readBlocks, item), readLines=[], inFence=False, fenceLanguage="", fenceLines=[])


def heading_level(line):
    if startsWith(line, "### "):
        return 3
    elif startsWith(line, "## "):
        return 2
    elif startsWith(line, "# "):
        return 1
    else:
        return 0


def ordered_number(line):
    """The number of a `12. ` item, 0 otherwise."""
    match indexOf(line, ". "):
        case Success(value=at):
            return unwrap_or(parseInt(take(line, at)), 0) if at > 0 and at < 5 else 0
        case Error(message=_m):
            return 0


def read_line(reading, raw):
    line = trim(raw)
    if reading.inFence:
        if startsWith(line, "```"):
            code = Block(blockKind="code", blockLevel=0, blockSpans=[], blockLanguage=reading.fenceLanguage, blockCode=join(reading.fenceLines, "\n"), blockRows=[])
            return Reading(readBlocks=listAppend(reading.readBlocks, code), readLines=[], inFence=False, fenceLanguage="", fenceLines=[])
        else:
            return Reading(readBlocks=reading.readBlocks, readLines=[], inFence=True, fenceLanguage=reading.fenceLanguage, fenceLines=listAppend(reading.fenceLines, raw))
    elif startsWith(line, "```"):
        closed = flush(reading)
        return Reading(readBlocks=closed.readBlocks, readLines=[], inFence=True, fenceLanguage=drop(line, 3), fenceLines=[])
    elif line == "":
        return flush(reading)
    elif heading_level(line) > 0:
        return adding(reading, block("heading", heading_level(line), drop(line, heading_level(line) + 1)))
    elif startsWith(line, "- ") or startsWith(line, "* "):
        return adding(reading, block("bullet", 0, drop(line, 2)))
    elif ordered_number(line) > 0:
        return adding(reading, block("numbered", ordered_number(line), drop(line, unwrap_or(indexOf(line, ". "), 0) + 2)))
    elif startsWith(line, "> "):
        return adding(reading, block("quote", 0, drop(line, 2)))
    else:
        return Reading(readBlocks=reading.readBlocks, readLines=listAppend(reading.readLines, line), inFence=False, fenceLanguage="", fenceLines=[])


def blocks_of(text):
    """The Markdown document in `text`; an unclosed fence still shows its code."""
    start = Reading(readBlocks=[], readLines=[], inFence=False, fenceLanguage="", fenceLines=[])
    read = foldList(unwrap_or(split(text, "\n"), [text]), start, read_line)
    if read.inFence:
        return listAppend(read.readBlocks, Block(blockKind="code", blockLevel=0, blockSpans=[], blockLanguage=read.fenceLanguage, blockCode=join(read.fenceLines, "\n"), blockRows=[]))
    else:
        return flush(read).readBlocks


def next_marker(text):
    """The earliest inline marker in `text` and where it is."""
    return earliest(text, ["`", "**", "*"], 0, -1, "")


class Marker:
    """An inline marker and its byte position, -1 for none."""
    markerAt: int
    markerText: str


def earliest(text, markers, index, best_at, best):
    match listGet(markers, index):
        case Success(value=marker):
            match indexOf(text, marker):
                case Success(value=at):
                    return earliest(text, markers, index + 1, at, marker) if best_at < 0 or at < best_at else earliest(text, markers, index + 1, best_at, best)
                case Error(message=_m):
                    return earliest(text, markers, index + 1, best_at, best)
        case Error(message=_e):
            return Marker(markerAt=best_at, markerText=best)


def span_kind(marker):
    if marker == "`":
        return "code"
    elif marker == "**":
        return "strong"
    else:
        return "em"


def plain_span(text):
    return [] if text == "" else [Span(spanText=text, spanKind="")]


def inner_span(inner, marker):
    """A marked run; code inside emphasis, as in **`file`**, reads as code."""
    if marker != "`" and length(inner) > 2 and startsWith(inner, "`") and endsWith(inner, "`"):
        return Span(spanText=take(drop(inner, 1), length(inner) - 2), spanKind="code")
    else:
        return Span(spanText=inner, spanKind=span_kind(marker))


def spans_of(text):
    """The inline runs of `text`: `code`, **strong** and *em*; an unclosed marker is plain text."""
    found = next_marker(text)
    if found.markerAt < 0:
        return plain_span(text)
    else:
        before = take(text, found.markerAt)
        after = drop(text, found.markerAt + length(found.markerText))
        match indexOf(after, found.markerText):
            case Success(value=end):
                inner = take(after, end)
                rest = drop(after, end + length(found.markerText))
                return listConcat(listAppend(plain_span(before), inner_span(inner, found.markerText)), spans_of(rest))
            case Error(message=_m):
                return listConcat(plain_span(before + found.markerText), spans_of(after))
