# osprey: flavor=python
"""Hunt driver: einops pattern parsing and recipe resolution on one symbolic
pattern line from stdin. `Parsing.parse` and `Recipe.prepare` and
`Reconstruct.reconstruct`
run on the raw bytes, so KLEE explores every identifier, anonymous length,
composition, ellipsis and error path the parser has.
"""
from testing import Check
from einops.Parsing import parse
from einops.Recipe import prepare
from einops.Reconstruct import reconstruct



def run():
    pattern = input()
    match parse(pattern, False, False):
        case Error(message=message):
            print("parse-error")
        case Success(value=value):
            print("parsed")
    match prepare(pattern, "rearrange", [], 4):
        case Error(message=message):
            print("prepare-error")
        case Success(value=value):
            match reconstruct(value, [2, 3, 4, 5], []):
                case Error(message=message):
                    print("reconstruct-error")
                case Success(value=value):
                    print("cooked")


def main():
    return Check.lenient(run)
