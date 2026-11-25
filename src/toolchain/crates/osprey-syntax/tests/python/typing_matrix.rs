//! Typed Python source contracts across the three frontends.
use super::parse;

/// [FLAVOR-PYTHON-TYPING] [FLAVOR-PYTHON-EXPRESSIONS] Unsupported forms stop
/// at the written Python token with the source frontend's diagnostic.
#[test]
fn unsupported_surface_anchors() {
    for (source, rejected, at) in [
        ("x = (1, 2)\n", "(1, 2)", (1, 4)),
        ("xs = [1, 2]\nx = xs[:1]\n", ":1", (2, 7)),
        ("def f(x=1):\n    return x\n", "x=1", (1, 6)),
        ("def f(*xs):\n    return xs\n", "*xs", (1, 6)),
        ("def f(**kw):\n    return kw\n", "**kw", (1, 6)),
        (
            "class C(Other):\n    value: int\n",
            "class C(Other):\n    value: int",
            (1, 0),
        ),
        ("class C:\n    field: int = 1\n", "field: int = 1", (2, 4)),
        (
            "class C:\n    def __len__() -> int: ...\n",
            "__len__",
            (2, 8),
        ),
        ("obj.field = 1\n", "obj.field", (1, 0)),
        ("xs[0] = 2\n", "xs[0]", (1, 0)),
        (
            "try:\n    run()\nexcept ValueError:\n    pass\n",
            "try:\n    run()\nexcept ValueError:\n    pass",
            (1, 0),
        ),
        ("raise ValueError()\n", "raise ValueError()", (1, 0)),
        ("def f():\n    return await job()\n", "await job()", (2, 11)),
        ("def f():\n    yield 1\n", "yield 1", (2, 4)),
        ("def f[*Ts](x):\n    return x\n", "*Ts", (1, 6)),
        ("def f[T: int](x: T):\n    return x\n", "T: int", (1, 6)),
        ("class C[*Ts]:\n    field: int\n", "*Ts", (1, 8)),
    ] {
        one_diagnostic(
            source,
            &format!("Python `{rejected}` has no supported Osprey lowering"),
            at,
        );
    }
    for (source, message, at) in [
        (
            "x = (i for i in range(3))\n",
            "generator expressions have no Osprey lowering; write toList(map(...)) or fold",
            (1, 4),
        ),
        (
            "class C:\n    def __init__(self):\n        pass\n",
            "a declaration signature must have exactly an `...` body",
            (3, 8),
        ),
        (
            "def f(x):\n    return x\n    print(x)\n",
            "return must terminate every branch of a tail suite",
            (2, 4),
        ),
        (
            "def f(x):\n    if x:\n        return 1\n",
            "return must terminate every branch of a tail suite",
            (2, 4),
        ),
        (
            "async def f():\n    return 1\n",
            "async functions do not have an Osprey lowering; use fibers",
            (1, 0),
        ),
        (
            "from typing import Callable\ndef f(x: Callable[..., int]):\n    return x\n",
            "Callable requires an explicit parameter list",
            (2, 18),
        ),
        (
            "from typing import Final\nx: Final[int, str] = 1\n",
            "Final requires exactly one type argument",
            (2, 8),
        ),
        (
            "from typing import Final\nx: Final[int] = 1\nx = 2\n",
            "binding `x` cannot be reassigned: it is immutable or declared Final",
            (3, 0),
        ),
        (
            "from typing import Tuple\n",
            "typing form `Tuple` has no Osprey representation",
            (1, 19),
        ),
    ] {
        one_diagnostic(source, message, at);
    }
}

fn one_diagnostic(source: &str, message: &str, at: (u32, u32)) {
    let errors = parse(source).errors;
    let actual: Vec<_> = errors
        .iter()
        .map(|e| (e.message.as_str(), (e.position.line, e.position.column)))
        .collect();
    assert_eq!(actual, [(message, at)], "{source}");
}

/// PEP 484/526/585/591/695 spellings share the ordinary Osprey contracts.
#[test]
fn typed_declarations_match_default_and_ml_contracts() {
    use osprey_syntax::{parse_program_with_flavor, Flavor};
    for [default, ml, python] in [
        [
            "fn choose<T, U>(first: T, second: U) -> T = first\n",
            "choose<T, U> : (T, U) -> T\nchoose (first, second) = first\n",
            "def choose[T, U](first: T, second: U) -> T:\n    return first\n",
        ],
        [
            "type Batch<T> = { values: Map<string, List<T>> }\n",
            "type Batch T =\n    values : Map string (List T)\n",
            "class Batch[T]:\n    values: dict[str, list[T]]\n",
        ],
        [
            "fn apply(work: fn(int, int) -> int, value: int) -> int = work(value, value)\n",
            "apply : ((int, int) -> int, int) -> int\napply (work, value) = work (value, value)\n",
            "from collections.abc import Callable as Fn\ndef apply(work: Fn[[int, int], int], value: int) -> int:\n    return work(value, value)\n",
        ],
        [
            "let limit: int = 3\n",
            "limit : int\nlimit = 3\n",
            "from typing import Final as Fixed\nlimit: Fixed[int] = 3\n",
        ],
    ] {
        let canonical = |source, flavor| {
            let parsed = parse_program_with_flavor(source, flavor);
            assert!(parsed.errors.is_empty(), "{flavor}: {:?}", parsed.errors);
            osprey_ast::canonical::without_positions(&parsed.program)
        };
        let expected = canonical(default, Flavor::Default);
        assert_eq!(canonical(ml, Flavor::Ml), expected, "{ml}");
        assert_eq!(canonical(python, Flavor::Python), expected, "{python}");
    }
}
