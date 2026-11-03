# osprey: flavor=python
"""Osprey Agent's turn logic and transcript, without a window or a network: a
whole two-round tool conversation over canned replies, the commands each tool
becomes on each platform, what the transcript shows at each step, and the
Markdown the model writes. Live runs are driven through the window by drive/."""
from agent import Engine
from agent import Highlight
from agent import Transcript
from pydantic_ai import JsonUtil as J
from pydantic_ai import Messages as M
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
    body = Engine.request_body(opened, MODEL, "")
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


def system_text(part):
    match part:
        case M.SystemPromptPart(content=content):
            return content
        case _:
            return ""


def last_request(history):
    requests = M.requests(unwrap_or(M.validate_messages(history), []))
    return at(requests, len(requests) - 1, M.ModelRequest(parts=[], instructions=""))


def last_systems(history):
    return filterList(mapList(last_request(history).parts, system_text), lambda text: text != "")


def folder_prompt_cases():
    other = "/work/other"
    empty = Engine.add_user(Engine.new_history(), FOLDER, "one")
    same = Engine.add_user(empty, FOLDER, "two")
    moved = Engine.add_user(same, other, "three")
    unchanged = Engine.add_user(moved, other, "four")
    back = Engine.add_user(unchanged, FOLDER, "five")
    corrupt = Engine.add_user(JsonString("broken"), FOLDER, "fresh")
    wire = J.list_at(unwrap_or(J.decode(Engine.request_body(moved, MODEL, "")), JsonNull()), "messages")
    prompt = J.text_at(at(wire, len(wire) - 2, JsonNull()), "content", "")
    return [
        Check.equal("folder.empty_gets_prompt", len(last_systems(empty)), 1),
        Check.truth("folder.empty_names_path", contains(at(last_systems(empty), 0, ""), FOLDER), J.encode(empty)),
        Check.equal("folder.same_adds_no_part", len(last_request(same).parts), 1),
        Check.equal("folder.changed_adds_one_prompt", len(last_systems(moved)), 1),
        Check.truth("folder.changed_names_new_not_old", contains(at(last_systems(moved), 0, ""), other) and not contains(at(last_systems(moved), 0, ""), FOLDER), J.encode(moved)),
        Check.equal("folder.only_last_prompt_counts", len(last_request(unchanged).parts), 1),
        Check.equal("folder.changing_back_adds_prompt", len(last_systems(back)), 1),
        Check.truth("folder.changing_back_names_original", contains(at(last_systems(back), 0, ""), FOLDER), J.encode(back)),
        Check.equal("folder.corrupt_reads_empty", len(M.requests(unwrap_or(M.validate_messages(corrupt), []))), 1),
        Check.equal("folder.corrupt_gets_prompt", len(last_systems(corrupt)), 1),
        Check.equal("folder.wire_system_role", J.text_at(at(wire, len(wire) - 2, JsonNull()), "role", ""), "system"),
        Check.equal("folder.wire_user_role", J.text_at(at(wire, len(wire) - 1, JsonNull()), "role", ""), "user"),
        Check.equal("folder.wire_user_words", J.text_at(at(wire, len(wire) - 1, JsonNull()), "content", ""), "three"),
        Check.truth("folder.wire_prompt_before_user", contains(prompt, other) and not contains(prompt, FOLDER), J.encode(JsonArray(wire))),
    ]


def in_flight_cases():
    opened = Engine.add_user(Engine.new_history(), FOLDER, "Go")
    first = Engine.accept(opened, MODEL, 200, FIRST)
    tools = at(Transcript.items_of(first.replyHistory), 2, Transcript.plain_item("", "", ""))
    return [
        Check.equal("flight.not_done", at(tools.itemSteps, 0, step_blank()).stepDone, False),
        Check.equal("flight.no_failure", at(tools.itemSteps, 0, step_blank()).stepFailed, False),
    ]


def prov_pairs():
    """Every `provider/model` in the catalogue, in order."""
    return foldList(Engine.provider_ids(), [], lambda acc, pid: listConcat(acc, mapList(Engine.model_ids(pid), lambda mid: Engine.full_model(pid, mid))))


def prov_zai(model):
    pid = Engine.provider_id_of(model)
    return pid == "zai" or pid == "zai-coding"


def prov_strict(model):
    pid = Engine.provider_id_of(model)
    return pid == "deepseek" or pid == "zai" or pid == "zai-coding"


def prov_effort(model, level):
    return "" if level == "" or level == "off" else level


def prov_body_checks(model, level):
    opened = Engine.add_user(Engine.new_history(), FOLDER, "Go")
    text = Engine.request_body(opened, model, level)
    doc = unwrap_or(J.decode(text), JsonNull())
    pid = Engine.provider_id_of(model)
    mid = Engine.model_id_of(model)
    suffix = pid + "." + mid + "." + (level if level != "" else "default")
    zai = prov_zai(model)
    effort = prov_effort(model, level)
    kind = "disabled" if level == "off" else ("enabled" if level != "" else "")
    base = [
        Check.equal("prov.url." + suffix, Engine.url(model), Engine.provider_base(pid) + "/chat/completions"),
        Check.equal("prov.model." + suffix, J.text_at(doc, "model", ""), mid),
        Check.truth("prov.strict." + suffix, contains(text, "\"strict\":true") == prov_strict(model), text),
        Check.equal("prov.effort." + suffix, J.text_at(doc, "reasoning_effort", ""), effort),
        Check.equal("prov.tool_choice." + suffix, J.text_at(doc, "tool_choice", ""), "auto"),
        Check.equal("prov.stream." + suffix, J.bool_at(doc, "stream", True), False),
    ]
    thinking = [Check.equal("prov.thinking.type." + suffix, J.text_at(J.object_at(doc, "thinking"), "type", ""), kind)] if zai else [Check.equal("prov.no_thinking." + suffix, J.has(doc, "thinking"), False)]
    return listConcat(base, thinking)


def prov_level_checks(model):
    return foldList(listPrepend(Engine.model_levels(model), ""), [], lambda acc, level: listConcat(acc, prov_body_checks(model, level)))


def prov_cases():
    bodies = foldList(prov_pairs(), [], lambda acc, model: listConcat(acc, prov_level_checks(model)))
    roundtrip = prov_roundtrip_cases()
    return listConcat(bodies, roundtrip)


def prov_roundtrip_cases():
    opened = Engine.add_user(Engine.new_history(), FOLDER, "Think")
    reply = "{\"id\":\"r\",\"model\":\"glm-5.3\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"Done.\",\"reasoning_content\":\"I reasoned.\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}"
    accepted = Engine.accept(opened, "zai/glm-5.3", 200, reply)
    body = unwrap_or(J.decode(Engine.request_body(accepted.replyHistory, "zai/glm-5.3", "high")), JsonNull())
    messages = J.list_at(body, "messages")
    assistant = at(messages, len(messages) - 1, JsonNull())
    return [
        Check.equal("prov.roundtrip_ok", accepted.replyOk, True),
        Check.equal("prov.roundtrip_reasoning_back", J.text_at(assistant, "reasoning_content", ""), "I reasoned."),
        Check.equal("prov.old_deepseek_resolves", Engine.provider_id_of("deepseek-chat"), "deepseek"),
        Check.equal("prov.old_deepseek_url", Engine.url("deepseek-chat"), "https://api.deepseek.com/chat/completions"),
        Check.equal("prov.old_deepseek_label", Engine.model_label("deepseek-chat"), "DeepSeek Chat"),
        Check.truth("prov.unknown_provider_words", contains(Engine.provider_problem("nosuch/glm-5.3"), "Unknown provider"), Engine.provider_problem("nosuch/glm-5.3")),
        Check.truth("prov.unknown_model_words", contains(Engine.provider_problem("deepseek/nosuch"), "no model"), Engine.provider_problem("deepseek/nosuch")),
    ]


def failure_cases():
    opened = Engine.add_user(Engine.new_history(), FOLDER, "Go")
    limited = Engine.accept(opened, MODEL, 429, "{\"error\":\"busy\"}")
    garbage = Engine.accept(opened, MODEL, 200, "not json")
    empty = Engine.accept(opened, MODEL, 200, "{\"choices\":[]}")
    blank = Engine.accept(opened, MODEL, 200, "{\"id\":\"r\",\"model\":\"deepseek-chat\",\"choices\":[{\"index\":0,\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}")
    return [
        Check.equal("fail.http_ok", limited.replyOk, False),
        Check.truth("fail.http_words", contains(limited.replyProblem, "status_code: 429"), limited.replyProblem),
        Check.equal("fail.history_kept", J.encode(limited.replyHistory), J.encode(opened)),
        Check.equal("fail.garbage", garbage.replyOk, False),
        Check.equal("fail.empty_choices", empty.replyOk, False),
        Check.equal("fail.blank_no_text", blank.replyOk, False),
        Check.truth("fail.blank_words", contains(blank.replyProblem, "no answer"), blank.replyProblem),
        Check.equal("fail.blank_history_kept", J.encode(blank.replyHistory), J.encode(opened)),
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
        Check.equal("md.strong_mixed_code", span_text(Transcript.spans_of("**Fix (`a.py`, `b`):** x")), "strong:Fix (|code:a.py|strong:, |code:b|strong:):|: x"),
        Check.equal("md.heading_code_first", span_text(Transcript.spans_of("**`a` fix**")), "code:a|strong: fix"),
        Check.equal("md.empty", len(Transcript.blocks_of("")), 0),
        Check.equal("title.first_line", Transcript.title_of("  Fix it\nplease"), "Fix it"),
        Check.truth("title.cut", endsWith(Transcript.title_of(fold(range(10), "", lambda acc, _i: acc + "0123456789")), "…"), "not cut"),
    ]


# Syntax colouring: each language's snippet, then the checks on it and the
# round-trip law over everything malformed.

class Snip:
    """One language's real snippet."""
    snipName: str
    snipCode: str


HL_SNIPPETS = [
    Snip(snipName="python", snipCode="class Point:\n    def add(self, first):\n        \"\"\"Sum.\"\"\"\n        return first + 1  # forty two"),
    Snip(snipName="javascript", snipCode="export class Counter {\n  run(name) {\n    // once\n    return `hi ${name}` + 1;\n  }\n}"),
    Snip(snipName="typescript", snipCode="interface Shape {\n  area(): number;\n}\nconst make = (n: number): string => `a${n}`;\n// done"),
    Snip(snipName="rust", snipCode="struct Span {\n    start: usize,\n}\n\nfn len<'a>(s: &'a str) -> usize {\n    // size\n    s.len() + 0x1F\n}"),
    Snip(snipName="go", snipCode="type Node struct {\n    Name string\n}\n\nfunc main() {\n    // go\n    fmt.Println(\"hi\", 42)\n}"),
    Snip(snipName="swift", snipCode="class View {\n    func draw() -> Int {\n        // once\n        return 7\n    }\n}"),
    Snip(snipName="c", snipCode="#include <stdio.h>\n/* add */\nint add(int a) {\n    // sum\n    return a + 2;\n}"),
    Snip(snipName="cpp", snipCode="class Engine {\npublic:\n    virtual int run() = 0;  // pure\n};\n/* block */"),
    Snip(snipName="java", snipCode="public class Main {\n    int run(String arg) {\n        // once\n        return 9;\n    }\n}"),
    Snip(snipName="kotlin", snipCode="data class User(val name: String)\n\nfun main() {\n    // start\n    println(\"hi\", 3)\n}"),
    Snip(snipName="csharp", snipCode="public class Server {\n    int Run() {\n        // once\n        return 5;\n    }\n}"),
    Snip(snipName="ruby", snipCode="class Gem\n  def name\n    # plain\n    \"gem\" + 1.to_s\n  end\nend"),
    Snip(snipName="php", snipCode="<?php\nclass Site {\n  function run($path) {\n    // once\n    return 'home' . 2;\n  }\n}"),
    Snip(snipName="bash", snipCode="run_job() {\n  if [ -f \"$TARGET\" ]; then\n    # present\n    echo \"${HOME}/ok\" 3\n  fi\n}"),
    Snip(snipName="sql", snipCode="CREATE TABLE users (id INT, name VARCHAR(10));\nSELECT id, COUNT(*) AS total\nFROM users -- picks\nWHERE name = 'bob' AND age > 21"),
    Snip(snipName="json", snipCode="{\"schema\": \"v2\", \"n\": 12, \"ok\": true}\n// note"),
    Snip(snipName="yaml", snipCode="name: agent\n# note\nlevel: 3\nalive: true\npath: \"src/a.py\""),
    Snip(snipName="toml", snipCode="[package]\nname = \"agent\"  # note\nversion = 2"),
    Snip(snipName="css", snipCode=".menu {\n  color: #1E1E24;  /* dark */\n  width: rgb(4, 8, 16);\n}"),
    Snip(snipName="html", snipCode="<div class=\"card\">\n  <!-- note -->\n  <span>4</span>\n</div>"),
    Snip(snipName="markdown", snipCode="# Title\n\n> quoted\n\n1. first\n\nUse `agent`."),
    Snip(snipName="diff", snipCode="+++ b/new\n--- a/old\n@@ -1 +1 @@\n+added line\n-removed line\n context"),
    Snip(snipName="osprey", snipCode="type Point\nmodule Math\n\nexport fn add (a, b) =\n    // sum\n    a + b + 2"),
]

HL_MALFORMED = ["", "\n\n\n", "\t\t", "a\r\nb\r\n", "héllo → wörld ✓", "'", "\"", "'''", "\"unclosed", "/* unclosed block", "def", "0x", "${", "#[", "<", "</a>", "<!--", "`x`", "@word", "\"\"\"\"\"\"", "```\"'''<!--/*/#@${!:`"]


def hl_flow(language, name, code, words):
    """Every control-flow word of a language draws as a keyword."""
    marked = hl_marked(language, code)
    return Check.truth("hl." + language + ".flow." + name, len(filterList(words, lambda word: not contains(marked, "|keyword:" + word + "|"))) == 0, marked)


def hl_marked(language, code):
    """The tokens as `kind:text`, fenced, so one token can be asserted exactly."""
    return "|" + join(mapList(Highlight.tokens_of(language, code), lambda t: t.tokenKind + ":" + t.tokenText), "|") + "|"


def hl_code(name):
    found = filterList(HL_SNIPPETS, lambda s: s.snipName == name)
    return unwrap_or(listGet(found, 0), Snip(snipName="", snipCode="")).snipCode


def hl_has(name, code, kind, text):
    marked = hl_marked(name, code)
    return Check.truth("hl." + name + "." + kind, contains(marked, "|" + kind + ":" + text + "|"), marked)


def hl_snip(name, kind, text):
    return hl_has(name, hl_code(name), kind, text)


def hl_roundtrip_checks():
    """Joining every token's text gives the code back, for every snippet and every malformed input in every language."""
    codes = listConcat(HL_MALFORMED, mapList(HL_SNIPPETS, lambda s: s.snipCode))
    pairs = foldList(HL_SNIPPETS, [], lambda acc, s: listConcat(acc, mapList(codes, lambda code: Snip(snipName=s.snipName, snipCode=code))))
    return mapList(pairs, lambda p: Check.truth("hl.roundtrip." + p.snipName, join(mapList(Highlight.tokens_of(p.snipName, p.snipCode), lambda t: t.tokenText), "") == p.snipCode, hl_marked(p.snipName, p.snipCode)))


def highlight_cases():
    kinds = [
        hl_snip("python", "type", "Point"), hl_snip("python", "keyword", "def"), hl_snip("python", "function", "add"), hl_snip("python", "builtin", "self"),
        hl_snip("python", "string", "\"\"\"Sum.\"\"\""), hl_snip("python", "number", "1"), hl_snip("python", "comment", "# forty two"),
        hl_snip("javascript", "keyword", "export"), hl_snip("javascript", "type", "Counter"), hl_snip("javascript", "function", "run"),
        hl_snip("javascript", "string", "`hi ${name}`"), hl_snip("javascript", "number", "1"), hl_snip("javascript", "comment", "// once"),
        hl_snip("typescript", "type", "Shape"), hl_snip("typescript", "type", "number"), hl_snip("typescript", "function", "area"),
        hl_snip("typescript", "string", "`a${n}`"), hl_snip("typescript", "comment", "// done"), hl_snip("typescript", "keyword", "const"),
        hl_snip("rust", "type", "Span"), hl_snip("rust", "type", "'a"), hl_snip("rust", "function", "len"), hl_snip("rust", "keyword", "fn"),
        hl_snip("rust", "number", "0x1F"), hl_snip("rust", "comment", "// size"), hl_snip("rust", "type", "usize"),
        hl_snip("go", "keyword", "func"), hl_snip("go", "type", "Node"), hl_snip("go", "string", "\"hi\""), hl_snip("go", "number", "42"),
        hl_snip("go", "comment", "// go"), hl_snip("go", "function", "Println"), hl_snip("go", "type", "string"),
        hl_snip("swift", "keyword", "class"), hl_snip("swift", "type", "View"), hl_snip("swift", "function", "draw"),
        hl_snip("swift", "number", "7"), hl_snip("swift", "comment", "// once"), hl_snip("swift", "type", "Int"),
        hl_snip("c", "meta", "#include"), hl_snip("c", "comment", "/* add */"), hl_snip("c", "function", "add"),
        hl_snip("c", "number", "2"), hl_snip("c", "comment", "// sum"), hl_snip("c", "keyword", "return"),
        hl_snip("cpp", "keyword", "class"), hl_snip("cpp", "type", "Engine"), hl_snip("cpp", "function", "run"),
        hl_snip("cpp", "comment", "// pure"), hl_snip("cpp", "comment", "/* block */"), hl_snip("cpp", "number", "0"),
        hl_snip("java", "keyword", "public"), hl_snip("java", "type", "Main"), hl_snip("java", "function", "run"),
        hl_snip("java", "comment", "// once"), hl_snip("java", "number", "9"), hl_snip("java", "type", "int"),
        hl_snip("kotlin", "keyword", "data"), hl_snip("kotlin", "type", "User"), hl_snip("kotlin", "function", "main"),
        hl_snip("kotlin", "string", "\"hi\""), hl_snip("kotlin", "number", "3"), hl_snip("kotlin", "comment", "// start"),
        hl_snip("csharp", "keyword", "public"), hl_snip("csharp", "type", "Server"), hl_snip("csharp", "function", "Run"),
        hl_snip("csharp", "comment", "// once"), hl_snip("csharp", "number", "5"), hl_snip("csharp", "type", "int"),
        hl_snip("ruby", "keyword", "class"), hl_snip("ruby", "type", "Gem"), hl_snip("ruby", "function", "name"),
        hl_snip("ruby", "string", "\"gem\""), hl_snip("ruby", "number", "1"), hl_snip("ruby", "comment", "# plain"),
        hl_snip("php", "keyword", "class"), hl_snip("php", "type", "Site"), hl_snip("php", "function", "run"), hl_snip("php", "meta", "$path"),
        hl_snip("php", "string", "'home'"), hl_snip("php", "number", "2"), hl_snip("php", "comment", "// once"),
        hl_snip("bash", "function", "run_job"), hl_snip("bash", "keyword", "if"), hl_snip("bash", "keyword", "fi"),
        hl_snip("bash", "string", "\"${HOME}/ok\""), hl_snip("bash", "number", "3"), hl_snip("bash", "comment", "# present"),
        hl_snip("sql", "keyword", "CREATE"), hl_snip("sql", "type", "INT"), hl_snip("sql", "type", "VARCHAR"),
        hl_snip("sql", "function", "COUNT"), hl_snip("sql", "string", "'bob'"), hl_snip("sql", "number", "21"), hl_snip("sql", "comment", "-- picks"),
        hl_snip("json", "meta", "\"schema\""), hl_snip("json", "string", "\"v2\""), hl_snip("json", "number", "12"),
        hl_snip("json", "builtin", "true"), hl_snip("json", "comment", "// note"),
        hl_snip("yaml", "meta", "name"), hl_snip("yaml", "comment", "# note"), hl_snip("yaml", "number", "3"),
        hl_snip("yaml", "builtin", "true"), hl_snip("yaml", "string", "\"src/a.py\""),
        hl_snip("toml", "type", "[package]"), hl_snip("toml", "meta", "name"), hl_snip("toml", "string", "\"agent\""),
        hl_snip("toml", "comment", "# note"), hl_snip("toml", "number", "2"),
        hl_snip("css", "keyword", ".menu"), hl_snip("css", "meta", "color"), hl_snip("css", "number", "#1E1E24"),
        hl_snip("css", "comment", "/* dark */"), hl_snip("css", "function", "rgb"),
        hl_snip("html", "keyword", "div"), hl_snip("html", "meta", "class"), hl_snip("html", "string", "\"card\""),
        hl_snip("html", "comment", "<!-- note -->"), hl_snip("html", "number", "4"),
        hl_snip("markdown", "keyword", "# Title"), hl_snip("markdown", "comment", "> quoted"), hl_snip("markdown", "number", "1."),
        hl_snip("markdown", "string", "`agent`"),
        hl_snip("diff", "meta", "+++ b/new"), hl_snip("diff", "meta", "--- a/old"), hl_snip("diff", "meta", "@@ -1 +1 @@"),
        hl_snip("diff", "inserted", "+added line"), hl_snip("diff", "removed", "-removed line"),
        hl_snip("osprey", "keyword", "type"), hl_snip("osprey", "function", "add"), hl_snip("osprey", "comment", "// sum"),
        hl_snip("osprey", "number", "2"), hl_snip("osprey", "keyword", "export"),
    ]
    rules = [
        hl_has("python", "formatted classy", "", "formatted"),
        hl_flow("python", "return", "if x:\n    return y\nelse:\n    for i in z:\n        while i:\n            break\n    continue\n", ["if", "return", "else", "for", "while", "break", "continue"]),
        Check.truth("hl.python.whole_words", not contains(hl_marked("python", "formatted classy"), "keyword:"), hl_marked("python", "formatted classy")),
        hl_has("python", "x = \"def class 42 # no\"", "string", "\"def class 42 # no\""),
        Check.truth("hl.python.hash_in_string", not contains(hl_marked("python", "y = \"a # b\""), "comment:"), hl_marked("python", "y = \"a # b\"")),
        hl_has("python", "# say \"hi\"", "comment", "# say \"hi\""),
        hl_has("python", "s = '''\ndocs\n'''", "string", "'''\ndocs\n'''"),
        hl_has("c", "/* a\nb */", "comment", "/* a\nb */"),
        hl_has("bash", "cd ${HOME}\necho $USER", "meta", "${HOME}"),
        hl_has("bash", "cd ${HOME}\necho $USER", "meta", "$USER"),
        Check.truth("hl.bash.hash_not_comment", not contains(hl_marked("bash", "echo a#b"), "comment:"), hl_marked("bash", "echo a#b")),
        hl_has("css", "a:hover {\n}", "type", ":hover"),
        hl_has("sql", "select * from t", "keyword", "select"),
        hl_has("markdown", "```\nfence\n```", "meta", "```"),
        Check.equal("hl.unknown.one_plain", len(Highlight.tokens_of("brainfuck", "x = 1")), 1),
        Check.equal("hl.unknown.plain_kind", unwrap_or(listGet(Highlight.tokens_of("brainfuck", "x = 1"), 0), Highlight.Token(tokenText="", tokenKind="x")).tokenKind, ""),
        Check.equal("hl.empty_language.one_plain", len(Highlight.tokens_of("", "x = 1")), 1),
    ]
    alias_pairs = [["py", "python"], ["js", "javascript"], ["jsx", "javascript"], ["ts", "typescript"], ["tsx", "typescript"], ["rs", "rust"], ["kt", "kotlin"], ["cs", "csharp"], ["c#", "csharp"], ["c++", "cpp"], ["h", "cpp"], ["hpp", "cpp"], ["rb", "ruby"], ["sh", "bash"], ["shell", "bash"], ["zsh", "bash"], ["console", "bash"], ["yml", "yaml"], ["scss", "css"], ["xml", "html"], ["svg", "html"], ["md", "markdown"], ["patch", "diff"], ["ospml", "osprey"], ["osp", "osprey"], ["golang", "go"]]
    aliases = mapList(alias_pairs, lambda p: Check.equal("hl.alias." + unwrap_or(listGet(p, 0), ""), hl_marked(unwrap_or(listGet(p, 0), ""), hl_code(unwrap_or(listGet(p, 1), ""))), hl_marked(unwrap_or(listGet(p, 1), ""), hl_code(unwrap_or(listGet(p, 1), "")))))
    same = mapList(["Python", "PYTHON", "Rust", "JSON", "Markdown"], lambda n: Check.equal("hl.case." + n, hl_marked(n, hl_code("python")), hl_marked(toLowerCase(n), hl_code("python"))))
    paths = [
        Check.equal("hl.path.py", Highlight.language_of_path("a.py"), "python"),
        Check.equal("hl.path.case", Highlight.language_of_path("lib/x.TS"), "typescript"),
        Check.equal("hl.path.rs", Highlight.language_of_path("main.rs"), "rust"),
        Check.equal("hl.path.html", Highlight.language_of_path("index.html"), "html"),
        Check.equal("hl.path.svg", Highlight.language_of_path("x.svg"), "html"),
        Check.equal("hl.path.scss", Highlight.language_of_path("styles.scss"), "css"),
        Check.equal("hl.path.md", Highlight.language_of_path("doc.md"), "markdown"),
        Check.equal("hl.path.go", Highlight.language_of_path("g.go"), "go"),
        Check.equal("hl.path.swift", Highlight.language_of_path("m.swift"), "swift"),
        Check.equal("hl.path.h", Highlight.language_of_path("x.h"), "cpp"),
        Check.equal("hl.path.hpp", Highlight.language_of_path("x.hpp"), "cpp"),
        Check.equal("hl.path.sh", Highlight.language_of_path("run.sh"), "bash"),
        Check.equal("hl.path.sql", Highlight.language_of_path("db.sql"), "sql"),
        Check.equal("hl.path.toml", Highlight.language_of_path("c.toml"), "toml"),
        Check.equal("hl.path.yml", Highlight.language_of_path("v.yml"), "yaml"),
        Check.equal("hl.path.ospml", Highlight.language_of_path("a.ospml"), "osprey"),
        Check.equal("hl.path.osp", Highlight.language_of_path("o.osp"), "osprey"),
        Check.equal("hl.path.patch", Highlight.language_of_path("p.patch"), "diff"),
        Check.equal("hl.path.kt", Highlight.language_of_path("k.kt"), "kotlin"),
        Check.equal("hl.path.cs", Highlight.language_of_path("c.cs"), "csharp"),
        Check.equal("hl.path.rb", Highlight.language_of_path("r.rb"), "ruby"),
        Check.equal("hl.path.php", Highlight.language_of_path("p.php"), "php"),
        Check.equal("hl.path.tsx", Highlight.language_of_path("t.tsx"), "typescript"),
        Check.equal("hl.path.jsx", Highlight.language_of_path("j.jsx"), "javascript"),
        Check.equal("hl.path.no_extension", Highlight.language_of_path("Makefile"), ""),
        Check.equal("hl.path.dotfile", Highlight.language_of_path(".gitignore"), ""),
        Check.equal("hl.path.dotfolder", Highlight.language_of_path("dir/.env"), ""),
        Check.equal("hl.path.unknown_extension", Highlight.language_of_path("archive.tar.gz"), ""),
    ]
    huge = repeat_text("# line\n", 28572)
    just_under = repeat_text("# line\n", 28571)
    size = [
        Check.equal("hl.max_bytes.one_plain", len(Highlight.tokens_of("python", huge)), 1),
        Check.equal("hl.max_bytes.whole_text", unwrap_or(listGet(Highlight.tokens_of("python", huge), 0), Highlight.Token(tokenText="", tokenKind="")).tokenText, huge),
        Check.truth("hl.max_bytes.still_scans_under", len(Highlight.tokens_of("python", just_under)) > 1, toString(len(Highlight.tokens_of("python", just_under)))),
        Check.equal("hl.max_bytes.constant", Highlight.MAX_BYTES, 200000),
    ]
    return listConcat(listConcat(listConcat(listConcat(kinds, rules), aliases), listConcat(same, paths)), listConcat(size, hl_roundtrip_checks()))


def suite():
    return listConcat(listConcat(listConcat(listConcat(listConcat(listConcat(conversation_cases(), folder_prompt_cases()), in_flight_cases()), listConcat(failure_cases(), command_cases())), prov_cases()), markdown_cases()), highlight_cases())


def report_suite():
    return Check.report(suite())


def main():
    match Check.strictly(lambda: report_suite()):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
