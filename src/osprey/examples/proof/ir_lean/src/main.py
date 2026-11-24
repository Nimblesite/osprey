# osprey: flavor=python
"""`ir_lean <module.mlir> <Namespace> [function-prefix ...]` ([IR-LEAN-TRANSLATE]).

Reads MLIR generic syntax and prints the module as Lean. With prefixes, only
functions whose symbol starts with one of them are translated; the rest keep
their position as empty functions. A file that cannot be read or parsed
prints one `IR-LEAN-FAILED` line, which is not Lean.
"""
from ir_lean import Emit
from xdsl import Parser


def rest(items, start):
    """The items from index `start` on."""
    return toList(map(range(start, len(items)), lambda i: unwrap_or(listGet(items, i), "")))


def run(path, namespace, wanted):
    """Translate the file at `path`."""
    match andThen(readFile(path), Parser.parse_module):
        case Success(value=module):
            return Emit.lean_module(module, namespace, wanted)
        case Error(message=message):
            return f"IR-LEAN-FAILED {path}: {message}"


def main():
    given = args()
    if len(given) < 2:
        print("IR-LEAN-FAILED usage: ir_lean <module.mlir> <Namespace> [function-prefix ...]")
    else:
        print(run(unwrap_or(listGet(given, 0), ""), unwrap_or(listGet(given, 1), ""), rest(given, 2)))
