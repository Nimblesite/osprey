# osprey: flavor=python
from xdsl import Ir, Parser

def count_ops(op):
    return foldList(op.regions, 1, lambda n, region: foldList(region.blocks, n, lambda m, block: foldList(block.ops, m, lambda k, inner: wrapAdd(k, count_ops(inner)))))

def show(result):
    match result:
        case Success(value=value):
            return f"{value.name} with {count_ops(value)} operations"
        case Error(message=message):
            return f"error {message}"

print(show(Parser.parse_module(unwrap_or(readFile(unwrap_or(listGet(args(), 0), "")), ""))))
