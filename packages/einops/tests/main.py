# osprey: flavor=python
from einops import Recipe
from testing import Check

def recipe_case(i):
    match Recipe.prepare(unwrap_or(listGet(["a b -> b a"], i), ""), unwrap_or(listGet(["rearrange"], i), ""), unwrap_or(listGet([[]], i), []), unwrap_or(listGet([2], i), 0)):
        case Success(value=value):
            return Check.equal("a", "x", "x")
        case Error(message=message):
            return Check.failed("a", "x")

def main():
    match Check.strictly(lambda: Check.report(toList(map(range(1), recipe_case)))):
        case Success(value=value):
            return value
        case Error(message=message):
            return print(f"suite stopped: {message}")
