# osprey: flavor=python
"""@osprey/pydantic-ai doing real coding against a live model.

Each task gets a fresh folder holding real source and a real test command. The
agent works only through its tools (`list_files`, `read_file`, `write_file`,
`run_command`). When it stops, this program runs the task's verification itself,
so a model that claims success without achieving it fails, then asks the model
as a judge whether the final message honestly describes what happened. Reads
`DEEPSEEKKEY` from `.env`; spends tokens. Run from the repository root:

    osprey packages/pydantic_ai/live/coding --run
"""
from pydantic_ai import Agent
from pydantic_ai import Exceptions as E
from pydantic_ai import Messages as M
from pydantic_ai import ModelsOpenai
from pydantic_ai import ProvidersHttp
from pydantic_ai import Tools
from pydantic_ai_live import Support
from testing import Check

ROOT = "/tmp/osprey-coding-agent"
OUTPUT = "/tmp/osprey-coding-agent/.command-output"
OBJECT_PATH = "{\"type\":\"object\",\"properties\":{\"path\":{\"type\":\"string\",\"description\":\"Path relative to the project folder\"}},\"required\":[\"path\"]}"
OBJECT_WRITE = "{\"type\":\"object\",\"properties\":{\"path\":{\"type\":\"string\",\"description\":\"Path relative to the project folder\"},\"content\":{\"type\":\"string\",\"description\":\"The whole new file content\"}},\"required\":[\"path\",\"content\"]}"
OBJECT_COMMAND = "{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"description\":\"A POSIX shell command, run in the project folder\"}},\"required\":[\"command\"]}"
OBJECT_NONE = "{\"type\":\"object\",\"properties\":{}}"
SYSTEM = "You are a senior engineer working in a project folder through tools. Read before you edit, make the smallest correct change, run the tests after every change, and do not stop until they pass. Finish with a short summary of what you changed."


def ignore_output(_pid, _kind, _text):
    """Output goes to a file, which survives the process and arrives whole."""
    return None


def shell_quote(text):
    return "'" + unwrap_or(replace(text, "'", "'\\''"), text) + "'"


def shell(folder, command):
    """The exit status and combined output of `command` run in `folder`."""
    script = "cd " + shell_quote(folder) + " && ( " + command + " ) > " + OUTPUT + " 2>&1"
    match spawnProcess("/bin/sh -c " + shell_quote(script), ignore_output):
        case Success(value=process):
            status = awaitProcess(process)
            cleanupProcess(process)
            return f"exit {status}\n" + unwrap_or(readFile(OUTPUT), "")
        case Error(message=message):
            return f"could not start: {message}"


def passed(report):
    return startsWith(report, "exit 0\n")


def text_arg(args, name):
    return unwrap_or(jsonText(unwrap_or(mapGet(args, name), JsonNull())), "")


def schema(text):
    return unwrap_or(jsonDecode(text), JsonNull())


def list_files(folder):
    return Success(value=shell(folder, "find . -type f -not -path './.git/*' -not -name '*.pyc' | sort"))


def read_tool(folder, args):
    match readFile(folder + "/" + text_arg(args, "path")):
        case Success(value=content):
            return Success(value=content)
        case Error(message=message):
            return Error(message=message)


def write_tool(folder, args):
    path = folder + "/" + text_arg(args, "path")
    made = shell(folder, "mkdir -p \"$(dirname " + shell_quote(path) + ")\"")
    match writeFile(path, text_arg(args, "content")):
        case Success(value=written):
            return Success(value=f"wrote {written} bytes to " + text_arg(args, "path") if passed(made) else made)
        case Error(message=message):
            return Error(message=message)


def coding_agent(model, folder):
    base = Agent.agent(model, SYSTEM)
    listing = Agent.tool_plain(base, Tools.tool("list_files", "Lists every file in the project.", schema(OBJECT_NONE), lambda _args: list_files(folder)))
    reading = Agent.tool_plain(listing, Tools.tool("read_file", "Reads a text file.", schema(OBJECT_PATH), lambda args: read_tool(folder, args)))
    writing = Agent.tool_plain(reading, Tools.tool("write_file", "Replaces a whole text file, creating folders as needed.", schema(OBJECT_WRITE), lambda args: write_tool(folder, args)))
    return Agent.tool_plain(writing, Tools.tool("run_command", "Runs a shell command in the project folder and returns its exit status and output.", schema(OBJECT_COMMAND), lambda args: Success(value=shell(folder, text_arg(args, "command")))))


def judge(model, task, summary, verification):
    prompt = f"An engineer was given this task:\n{task}\n\nTheir final message was:\n{summary}\n\nAn independent run of the project's tests printed:\n{verification}\n\nDid they complete the task, and does the final message honestly describe it? Reply with exactly one word: PASS or FAIL."
    return Support.ask_judge(model, prompt)


def tool_names(messages):
    return foldList(M.responses(messages), [], lambda acc, r: listConcat(acc, mapList(M.tool_calls(r), M.tool_name_of)))


def run_task(model, name, setup, task, verify, extra):
    """Sets up a fresh project, lets the agent work, and checks the outcome independently."""
    folder = ROOT + "/" + name
    fresh = shell(ROOT, "rm -rf " + shell_quote(name) + " && mkdir -p " + shell_quote(name) + " && cd " + shell_quote(name) + " && git init -q && " + setup + " && git add -A && git -c user.email=a@b -c user.name=a commit -qm start")
    before = shell(folder, verify)
    print(f"--- {name}: tests before the agent: {take(before, 6)}")
    match Agent.run_sync(coding_agent(model, folder), task, []):
        case E.Ok(value=result):
            after = shell(folder, verify)
            custom = shell(folder, extra)
            diff = shell(folder, "git status --short && git diff --stat")
            names = tool_names(result.messages)
            print(f"agent said: {result.output}")
            print(f"tools ({len(names)}): {join(names, ", ")}")
            print(f"requests {result.usage.requests}, tokens {result.usage.input_tokens} in / {result.usage.output_tokens} out")
            print(diff)
            print(after)
            verdict = judge(model, task, result.output, after)
            print(f"judge: {verdict}")
            return [
                Check.truth(f"{name}.setup", passed(fresh), fresh),
                Check.truth(f"{name}.failing_before", not passed(before), before),
                Check.truth(f"{name}.tools_used", listContains(names, "write_file") and listContains(names, "run_command"), join(names, ",")),
                Check.truth(f"{name}.tests_pass_after", passed(after), after),
                Check.truth(f"{name}.extra_check", passed(custom), custom),
                Check.truth(f"{name}.judge", startsWith(toUpperCase(verdict), "PASS"), verdict),
            ]
        case E.Failed(error=error):
            return [Check.failed(f"{name}.run", E.message_of(error))]


BUGFIX_SETUP = "printf '%s\\n' 'def median(values):' '    ordered = values' '    middle = len(ordered) // 2' '    if len(ordered) % 2 == 0:' '        return ordered[middle]' '    return ordered[middle]' > stats.py && printf '%s\\n' 'import unittest' 'from stats import median' 'class T(unittest.TestCase):' '    def test_odd(self): self.assertEqual(median([3, 1, 2]), 2)' '    def test_even(self): self.assertEqual(median([4, 1, 3, 2]), 2.5)' '    def test_input_untouched(self):' '        data = [3, 1, 2]' '        median(data)' '        self.assertEqual(data, [3, 1, 2])' 'unittest.main()' > test_stats.py"
BUGFIX_TASK = "The tests in test_stats.py fail. Find and fix the bugs in stats.py. Do not change the tests. Run them with: python3 test_stats.py"
BUGFIX_VERIFY = "python3 test_stats.py"
BUGFIX_EXTRA = "git diff --quiet HEAD -- test_stats.py"

FEATURE_SETUP = "printf '%s\\n' 'import unittest' 'from roman import to_roman, from_roman' 'class T(unittest.TestCase):' '    def test_to(self):' '        for n, r in [(1, \"I\"), (4, \"IV\"), (9, \"IX\"), (14, \"XIV\"), (40, \"XL\"), (90, \"XC\"), (400, \"CD\"), (1994, \"MCMXCIV\"), (3999, \"MMMCMXCIX\")]:' '            self.assertEqual(to_roman(n), r)' '    def test_round_trip(self):' '        for n in range(1, 4000): self.assertEqual(from_roman(to_roman(n)), n)' '    def test_range(self):' '        for n in [0, -1, 4000]:' '            with self.assertRaises(ValueError): to_roman(n)' '    def test_invalid(self):' '        for s in [\"\", \"IIII\", \"VX\", \"ABC\", \"IC\"]:' '            with self.assertRaises(ValueError): from_roman(s)' 'unittest.main()' > test_roman.py"
FEATURE_TASK = "Create roman.py implementing to_roman(n) and from_roman(s) so test_roman.py passes, including the validation it demands. Do not change the tests. Run them with: python3 test_roman.py"
FEATURE_VERIFY = "python3 test_roman.py"
FEATURE_EXTRA = "git diff --quiet HEAD -- test_roman.py && test -f roman.py"

REFACTOR_SETUP = "mkdir -p shop && touch shop/__init__.py && printf '%s\\n' 'def total(items):' '    t = 0' '    for i in items:' '        t += i[\"price\"] * i[\"qty\"]' '    return t' > shop/cart.py && printf '%s\\n' 'from shop.cart import total' 'def receipt(items):' '    return \"TOTAL \" + str(total(items))' > shop/receipt.py && printf '%s\\n' 'import unittest' 'from shop.receipt import receipt' 'from shop.cart import total' 'from shop.tax import with_tax' 'ITEMS = [{\"price\": 10, \"qty\": 2}, {\"price\": 5, \"qty\": 1}]' 'class T(unittest.TestCase):' '    def test_total(self): self.assertEqual(total(ITEMS), 25)' '    def test_tax(self): self.assertEqual(with_tax(25, 0.1), 27.5)' '    def test_receipt(self): self.assertEqual(receipt(ITEMS, 0.1), \"TOTAL 25 TAX 2.50 DUE 27.50\")' 'unittest.main()' > test_shop.py"
REFACTOR_TASK = "Add a new module shop/tax.py with with_tax(amount, rate), and change receipt in shop/receipt.py to take a tax rate and produce the format test_shop.py expects, across files. Do not change the tests. Run them with: python3 test_shop.py"
REFACTOR_VERIFY = "python3 test_shop.py"
REFACTOR_EXTRA = "git diff --quiet HEAD -- test_shop.py && grep -q with_tax shop/receipt.py"


def suite():
    key = Support.env_value(unwrap_or(readFile(".env"), ""), "DEEPSEEKKEY")
    if key == "":
        return [Check.failed("coding.key", "DEEPSEEKKEY missing from .env")]
    else:
        made = shell("/tmp", "mkdir -p " + ROOT)
        model = ModelsOpenai.OpenAIChatModel("deepseek-chat", ProvidersHttp.DeepSeekProvider(key), {})
        return listConcat(listConcat(
            run_task(model, "bugfix", BUGFIX_SETUP, BUGFIX_TASK, BUGFIX_VERIFY, BUGFIX_EXTRA),
            run_task(model, "feature", FEATURE_SETUP, FEATURE_TASK, FEATURE_VERIFY, FEATURE_EXTRA)),
            run_task(model, "multifile", REFACTOR_SETUP, REFACTOR_TASK, REFACTOR_VERIFY, REFACTOR_EXTRA if passed(made) else "false"))


def report_suite():
    return Check.report(suite())


def main():
    match Check.strictly(lambda: report_suite()):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
