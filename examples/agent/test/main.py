# osprey: flavor=python
"""Osprey Agent's turn logic and transcript, without a window or a network: a
whole two-round tool conversation over canned replies, the commands each tool
becomes on each platform, what the transcript shows at each step, and the
Markdown the model writes. Live runs are driven through the window by drive/."""
from agent import Engine
from agent import Transcript
from pydantic_ai import JsonUtil as J
from testing import Check

MODEL = "deepseek-chat"
FOLDER = "/work/project"
FIRST = "{\"id\":\"r1\",\"model\":\"deepseek-flash\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Looking.\",\"tool_calls\":[{\"id\":\"c1\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"a.py\\\"}\"}},{\"id\":\"c2\",\"type\":\"function\",\"function\":{\"name\":\"run_command\",\"arguments\":\"{\\\"command\\\":\\\"make test\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":100,\"completion_tokens\":10,\"total_tokens\":110}}"
SECOND = "{\"id\":\"r2\",\"model\":\"deepseek-flash\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"\",\"tool_calls\":[{\"id\":\"c3\",\"type\":\"function\",\"function\":{\"name\":\"write_file\",\"arguments\":\"{\\\"path\\\":\\\"src/a.py\\\",\\\"content\\\":\\\"x = 1\\\\n\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}],\"usage\":{\"prompt_tokens\":200,\"completion_tokens\":20,\"total_tokens\":220}}"
THIRD = "{\"id\":\"r3\",\"model\":\"deepseek-flash\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Fixed `a.py`.\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":300,\"completion_tokens\":30,\"total_tokens\":330}}"


def at(items, i, fallback):
    return unwrap_or(listGet(items, i), fallback)


def repeat_text(text, count):
    """`text` `count` times, by doubling, so the depth stays small."""
    if count <= 0:
        return ""
    else:
        half = repeat_text(text, count // 2)
        return half + half + (text if count % 2 == 1 else "")


def result(call_id, name, text):
    return Engine.ToolResult(resultId=call_id, resultName=name, resultContent=text)


def conversation_cases():
    opened = Engine.add_user(Engine.new_history(), FOLDER, "Fix it")
    body = Engine.request_body(opened, MODEL)
    first = Engine.accept(opened, MODEL, 200, FIRST)
    owed = Engine.pending(first.replyHistory, [])
    partial = [result("c1", "read_file", "print(1)")]
    still = Engine.pending(first.replyHistory, partial)
    answered = Engine.add_returns(first.replyHistory, listAppend(partial, result("c2", "run_command", Engine.tool_result_text("run_command", 2, "boom", ""))))
    second = Engine.accept(answered, MODEL, 200, SECOND)
    third_history = Engine.add_returns(second.replyHistory, [result("c3", "write_file", "wrote src/a.py (6 bytes)")])
    third = Engine.accept(third_history, MODEL, 200, THIRD)
    totals = Engine.usage_of(third.replyHistory)
    items = Transcript.items_of(third.replyHistory)
    tools = at(items, 2, Transcript.plain_item("", "", ""))
    return [
        Check.truth("turn.body_names_model", contains(body, "\"model\":\"deepseek-chat\""), body),
        Check.truth("turn.body_has_system_prompt_once", contains(body, "/work/project"), body),
        Check.truth("turn.body_declares_all_tools", contains(body, "list_files") and contains(body, "read_file") and contains(body, "write_file") and contains(body, "run_command"), body),
        Check.equal("turn.first_ok", first.replyOk, True),
        Check.equal("turn.first_calls", len(first.replyCalls), 2),
        Check.equal("turn.owed_two", len(owed), 2),
        Check.equal("turn.owed_one_after_partial", at(still, 0, Engine.ToolCall(callId="", callName="", callArgs="")).callId, "c2"),
        Check.equal("turn.nothing_owed_after_returns", len(Engine.pending(answered, [])), 0),
        Check.equal("turn.second_owes_write", len(Engine.pending(second.replyHistory, [])), 1),
        Check.equal("turn.third_no_calls", len(third.replyCalls), 0),
        Check.equal("usage.requests", totals.totalRequests, 3),
        Check.equal("usage.tools", totals.totalTools, 3),
        Check.equal("usage.input", totals.totalInput, 600),
        Check.equal("usage.output", totals.totalOutput, 60),
        Check.equal("usage.last_input", totals.lastInput, 300),
        Check.equal("files.changed", join(Engine.files_changed(third.replyHistory), ","), "src/a.py"),
        Check.equal("files.failed_write_not_changed", len(Engine.files_changed(Engine.add_returns(second.replyHistory, [result("c3", "write_file", "failed with status 1")]))), 0),
        Check.equal("files.unanswered_write_not_changed", len(Engine.files_changed(second.replyHistory)), 0),
        Check.equal("cut.title_keeps_characters", Transcript.title_of(repeat_text("a", 41) + "é" + "bbb"), repeat_text("a", 41) + "…"),
        Check.equal("cut.output_keeps_characters", take(Engine.tool_result_text("read_file", 0, repeat_text("a", Engine.OUTPUT_LIMIT - 1) + "éz", ""), Engine.OUTPUT_LIMIT), repeat_text("a", Engine.OUTPUT_LIMIT - 1) + "\n"),
        Check.equal("cut.whole_character_fits", Engine.cut_text("aé", 3), "aé"),
        Check.equal("cut.nothing", Engine.cut_text("é", 1), ""),
        Check.equal("transcript.kinds", join(mapList(items, lambda i: i.itemKind), ","), "user,answer,tools,answer"),
        Check.equal("transcript.user", at(items, 0, tools).itemText, "Fix it"),
        Check.equal("transcript.rounds_merge", len(tools.itemSteps), 3),
        Check.equal("transcript.title", Transcript.step_title(tools.itemSteps), "Edited 1 file, Ran 1 command, Read 1 file"),
        Check.equal("transcript.summary", at(tools.itemSteps, 1, step_blank()).stepSummary, "make test"),
        Check.equal("transcript.failed_command", at(tools.itemSteps, 1, step_blank()).stepFailed, True),
        Check.equal("transcript.read_ok", at(tools.itemSteps, 0, step_blank()).stepFailed, False),
        Check.equal("transcript.answer_code", at(at(at(items, 3, tools).itemBlocks, 0, Transcript.block("", 0, "")).blockSpans, 1, Transcript.Span(spanText="", spanKind="")).spanKind, "code"),
    ]


def step_blank():
    return Transcript.Step(stepId="", stepTool="", stepSummary="", stepOutput="", stepDone=False, stepFailed=False)


def in_flight_cases():
    opened = Engine.add_user(Engine.new_history(), FOLDER, "Go")
    first = Engine.accept(opened, MODEL, 200, FIRST)
    tools = at(Transcript.items_of(first.replyHistory), 2, Transcript.plain_item("", "", ""))
    return [
        Check.equal("flight.not_done", at(tools.itemSteps, 0, step_blank()).stepDone, False),
        Check.equal("flight.no_failure", at(tools.itemSteps, 0, step_blank()).stepFailed, False),
    ]


def failure_cases():
    opened = Engine.add_user(Engine.new_history(), FOLDER, "Go")
    limited = Engine.accept(opened, MODEL, 429, "{\"error\":\"busy\"}")
    garbage = Engine.accept(opened, MODEL, 200, "not json")
    empty = Engine.accept(opened, MODEL, 200, "{\"choices\":[]}")
    return [
        Check.equal("fail.http_ok", limited.replyOk, False),
        Check.truth("fail.http_words", contains(limited.replyProblem, "status_code: 429"), limited.replyProblem),
        Check.equal("fail.history_kept", J.encode(limited.replyHistory), J.encode(opened)),
        Check.equal("fail.garbage", garbage.replyOk, False),
        Check.equal("fail.empty_choices", empty.replyOk, False),
        Check.equal("fail.tool_error_words", Engine.tool_result_text("read_file", 1, "", "no such file"), "The tool could not run: no such file"),
        Check.equal("fail.command_status", Engine.tool_result_text("run_command", 3, "out", ""), "exit 3\nout"),
        Check.equal("fail.read_status", Engine.tool_result_text("read_file", 1, "nope", ""), "failed with status 1\nnope"),
        Check.equal("ok.read_plain", Engine.tool_result_text("read_file", 0, "text", ""), "text"),
        Check.truth("cut.long_output", contains(Engine.tool_result_text("read_file", 0, fold(range(5000), "", lambda acc, _i: acc + "0123456789"), ""), "[output cut at"), "not cut"),
    ]


def command_cases():
    args = "{\"path\":\"src/a b.py\",\"content\":\"it's $HOME\",\"command\":\"echo hi\"}"
    write = Engine.tool_command("macos", FOLDER, "write_file", args)
    run = Engine.tool_command("macos", FOLDER, "run_command", args)
    win = Engine.tool_command("windows", FOLDER, "write_file", args)
    win_run = Engine.tool_command("windows", FOLDER, "run_command", args)
    return [
        Check.equal("command.posix_shell", write.executable, "/bin/sh"),
        Check.equal("command.values_are_arguments", join(mapList(toList(map(range(3), lambda i: i + 2)), lambda i: at(write.arguments, i, "")), "|"), "/work/project|src/a b.py|it's $HOME"),
        Check.truth("command.script_has_no_values", not contains(at(write.arguments, 1, ""), "HOME"), at(write.arguments, 1, "")),
        Check.equal("command.run_value", at(run.arguments, 3, ""), "echo hi"),
        Check.truth("command.windows_powershell", endsWith(win.executable, "powershell.exe"), win.executable),
        Check.equal("command.windows_values", at(win.arguments, 5, ""), "src/a b.py"),
        Check.truth("command.windows_read_reports_errors", contains(at(Engine.tool_command("windows", FOLDER, "read_file", args).arguments, 3, ""), "-ErrorAction Stop"), "read_file must fail on a missing file"),
        Check.truth("command.windows_run", contains(at(win_run.arguments, 3, ""), "cmd.exe"), at(win_run.arguments, 3, "")),
        Check.equal("command.unknown_tool", at(Engine.tool_command("linux", FOLDER, "nope", "{}").arguments, 2, ""), "nope"),
        Check.equal("summary.list", Engine.tool_summary("list_files", "{}"), "project files"),
        Check.equal("summary.path", Engine.tool_summary("read_file", args), "src/a b.py"),
    ]


def kinds(text):
    return join(mapList(Transcript.blocks_of(text), lambda b: b.blockKind), ",")


def span_text(spans):
    return join(mapList(spans, lambda s: s.spanKind + ":" + s.spanText), "|")


def markdown_cases():
    table = Transcript.blocks_of("Before\n\n| File | Change |\n|---|:-:|\n| `a.py` | **fixed** |\n| b.py | none |\n\nAfter")
    rows = at(table, 1, Transcript.block("", 0, "")).blockRows
    code = at(Transcript.blocks_of("```python\nx = 1\n  y = 2\n```"), 0, Transcript.block("", 0, ""))
    unclosed = at(Transcript.blocks_of("```\nkept"), 0, Transcript.block("", 0, ""))
    return [
        Check.equal("md.kinds", kinds("# H\n\npara one\nstill one\n\n- a\n* b\n1. c\n12. d\n> q\n\n```\ncode\n```"), "heading,paragraph,bullet,bullet,numbered,numbered,quote,code"),
        Check.equal("md.heading_level", at(Transcript.blocks_of("### deep"), 0, Transcript.block("", 0, "")).blockLevel, 3),
        Check.equal("md.number", at(Transcript.blocks_of("12. twelve"), 0, Transcript.block("", 0, "")).blockLevel, 12),
        Check.equal("md.zero_padded_number", span_text(at(Transcript.blocks_of("001. item"), 0, Transcript.block("", 0, "")).blockSpans), ":item"),
        Check.equal("md.paragraph_joins", span_text(at(Transcript.blocks_of("one\ntwo"), 0, Transcript.block("", 0, "")).blockSpans), ":one two"),
        Check.equal("md.code_language", code.blockLanguage, "python"),
        Check.equal("md.code_keeps_indent", code.blockCode, "x = 1\n  y = 2"),
        Check.equal("md.unclosed_fence_shows", unclosed.blockCode, "kept"),
        Check.equal("md.table_kinds", join(mapList(table, lambda b: b.blockKind), ","), "paragraph,table,paragraph"),
        Check.equal("md.table_rows", len(rows), 3),
        Check.equal("md.table_cells", len(at(rows, 0, [])), 2),
        Check.equal("md.table_code_cell", span_text(at(at(rows, 1, []), 0, [])), "code:a.py"),
        Check.equal("md.table_strong_cell", span_text(at(at(rows, 1, []), 1, [])), "strong:fixed"),
        Check.equal("md.barred_without_rule_is_prose", kinds("| not\n| a table"), "paragraph"),
        Check.equal("md.inline", span_text(Transcript.spans_of("a `b` **c** *d* e")), ":a |code:b|: |strong:c|: |em:d|: e"),
        Check.truth("md.unclosed_marker", contains(join(mapList(Transcript.spans_of("2 * 3"), lambda s: s.spanText), ""), "2 * 3"), "lost text"),
        Check.equal("md.strong_code", span_text(Transcript.spans_of("**`a.py`** — x")), "code:a.py|: — x"),
        Check.equal("md.empty", len(Transcript.blocks_of("")), 0),
        Check.equal("title.first_line", Transcript.title_of("  Fix it\nplease"), "Fix it"),
        Check.truth("title.cut", endsWith(Transcript.title_of(fold(range(10), "", lambda acc, _i: acc + "0123456789")), "…"), "not cut"),
    ]


def suite():
    return listConcat(listConcat(listConcat(conversation_cases(), in_flight_cases()), listConcat(failure_cases(), command_cases())), markdown_cases())


def report_suite():
    return Check.report(suite())


def main():
    match Check.strictly(lambda: report_suite()):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
