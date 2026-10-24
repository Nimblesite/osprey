# osprey: flavor=python
from jinja2 import Render as R
from jinja2 import Value as V
from testing import Check

def run(name, template, context):
    match R.render(R.chat_environment(), template, context):
        case Success(value=value):
            return Check.equal(name, value, "EXPECTED")
        case Error(message=message):
            return Check.equal(name, "ERROR:" + message, "EXPECTED")


def all_cases():
    return [run("hello", "Hello {{ name }}!", {"name": V.text("world")})]


def main():
    match Check.strictly(lambda: Check.report(all_cases())):
        case Success(value=value):
            return value
        case Error(message=message):
            return print("suite stopped: " + message)
