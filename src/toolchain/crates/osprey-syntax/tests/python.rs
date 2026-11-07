//! Python concrete syntax lowers directly to the shared Osprey AST.
//! Implements [FLAVOR-PYTHON-BOUNDARY], [FLAVOR-PYTHON-SELECT],
//! [FLAVOR-PYTHON-CST], and [FLAVOR-PYTHON-FUNCTIONS].
use osprey_ast::{Expr, Stmt};
use osprey_syntax::{parse_program_with_flavor, resolve_flavor, Flavor, Parsed};

#[expect(
    clippy::expect_used,
    reason = "the fixture cannot run until Python selection exists"
)]
fn python() -> Flavor {
    "python"
        .parse()
        .expect("the Python flavor must be selectable")
}

fn parse(source: &str) -> Parsed {
    parse_program_with_flavor(source, python())
}

/// Parses `source`, asserting it has no diagnostics.
fn parsed(source: &str) -> Parsed {
    let parsed = parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    parsed
}

/// Asserts `actual` has the same canonical AST as `expected`.
fn same_ast(actual: &osprey_ast::Program, expected: &osprey_ast::Program) {
    assert_eq!(
        osprey_ast::canonical::without_positions(actual),
        osprey_ast::canonical::without_positions(expected)
    );
}

fn equal(default: &str, source: &str) {
    let expected = parse_program_with_flavor(default, Flavor::Default);
    assert!(expected.errors.is_empty(), "{:?}", expected.errors);
    same_ast(&parsed(source).program, &expected.program);
}

/// [`equal`], with every Default `let h = handler E { … }` read as the
/// handler function a Python handler class declares.
fn equal_with_handler_functions(default: &str, source: &str) {
    let expected = parse_program_with_flavor(default, Flavor::Default);
    assert!(expected.errors.is_empty(), "{:?}", expected.errors);
    let expected = osprey_ast::Program {
        statements: expected
            .program
            .statements
            .into_iter()
            .map(handler_lets_as_functions)
            .collect(),
        doc: None,
    };
    same_ast(&parsed(source).program, &expected);
}

fn handler_lets_as_functions(statement: Stmt) -> Stmt {
    match statement {
        Stmt::Namespace {
            name,
            body,
            file_scoped,
            doc,
            inner_doc,
            position,
        } => Stmt::Namespace {
            name,
            body: body.into_iter().map(handler_lets_as_functions).collect(),
            file_scoped,
            doc,
            inner_doc,
            position,
        },
        Stmt::Module {
            path,
            kind,
            signature,
            body,
            doc,
            inner_doc,
            position,
        } => Stmt::Module {
            path,
            kind,
            signature,
            body: body
                .into_iter()
                .map(|item| osprey_ast::ModuleItem {
                    declaration: Box::new(handler_lets_as_functions(*item.declaration)),
                    ..item
                })
                .collect(),
            doc,
            inner_doc,
            position,
        },
        Stmt::Let {
            name,
            value:
                Expr::Lambda {
                    parameters,
                    body,
                    position,
                    ..
                },
            doc,
            ..
        } => Stmt::Function {
            name,
            type_params: Vec::new(),
            parameters,
            return_type: None,
            effects: Vec::new(),
            effect_tail: None,
            effect_row_present: false,
            body: *body,
            doc,
            position,
        },
        other => other,
    }
}

/// Asserts every `source` is rejected with a diagnostic.
fn rejects(sources: &[&str]) {
    for source in sources {
        assert!(
            !parse(source).errors.is_empty(),
            "silently accepted {source}"
        );
    }
}

/// Asserts every `source` is rejected with a diagnostic at an original position.
fn rejects_with_positions(sources: &[&str]) {
    for source in sources {
        let parsed = parse(source);
        assert!(!parsed.errors.is_empty(), "silently accepted {source}");
        assert!(parsed.errors.iter().all(|error| error.position.line > 0));
    }
}

/// Asserts every `source` is rejected with a diagnostic containing `needle`.
fn rejects_containing(needle: &str, sources: &[&str]) {
    for source in sources {
        let parsed = parse(source);
        assert!(
            parsed
                .errors
                .iter()
                .any(|error| error.message.contains(needle)),
            "missing `{needle}` diagnostic for {source}: {:?}",
            parsed.errors
        );
    }
}

#[test]
fn python_routing_preserves_explicit_selection_and_marker_conflicts() {
    assert_eq!(python().to_string(), "python");
    assert_eq!(resolve_flavor(None, "model.py", ""), Ok(python()));
    let header = "#!/usr/bin/env osprey\n# coding: utf-8\n# osprey: flavor=python\nx = 1\n";
    assert_eq!(resolve_flavor(None, "model.txt", header), Ok(python()));
    assert!(resolve_flavor(None, "model.osp", header).is_err());
    assert_eq!(
        resolve_flavor(Some(python()), "model.osp", header),
        Ok(python())
    );
    // [FLAVOR-SELECT]: only leading comments and blank lines may precede a
    // marker, so a marker-shaped line after code is ordinary text, not a
    // selection that contradicts the extension.
    assert_eq!(
        resolve_flavor(None, "notes.osp", "let x = 1\n// osprey: flavor=ml\n"),
        Ok(Flavor::Default)
    );
    assert_eq!(
        resolve_flavor(None, "model.txt", "x = 1\n# osprey: flavor=python\n"),
        Ok(Flavor::Default)
    );
    assert_eq!(
        resolve_flavor(None, "a.txt", "/// Doc.\n\n// osprey: flavor=ml\nx = 1\n"),
        Ok(Flavor::Ml)
    );
    // Python markers use `#`; Default and ML keep `//`.
    assert_eq!(
        resolve_flavor(None, "a.txt", "# osprey: flavor=ml\n"),
        Ok(Flavor::Default)
    );
    assert_eq!(
        resolve_flavor(None, "a.txt", "// osprey: flavor=python\n"),
        Ok(Flavor::Default)
    );
}

#[test]
fn python_functions_bindings_and_types_equal_default_ast() {
    // [FLAVOR-PYTHON-FUNCTIONS]: calls, lambdas, cells, fibers and loops.
    // [FLAVOR-PYTHON-TYPING] PEP 484 and PEP 585 vocabulary is compile-time syntax.
    for (default, source) in [
        (
            "fn scale(value, factor) = value * factor\nlet twice = fn(value) => scale(value: value, factor: 2)\nlet result = twice(21)\n",
            "def scale(value, factor):\n    return value * factor\ntwice = lambda value: scale(value=value, factor=2)\nresult = twice(21)\n",
        ),
        (
            "fn answer(x) = { let offset = -2\n x + offset }\nlet smallest = -9223372036854775808\n",
            "def answer(x):\n    offset = -2\n    return x + offset\nsmallest = -9223372036854775808\n",
        ),
        (
            "fn next(value: int) -> int = value\nmut counter: int = 1\ncounter = 2\n",
            "def next(value: int) -> int:\n    return value\ncounter: int = 1\ncounter = 2\n",
        ),
        // [FLAVOR-PYTHON-TYPING] PEP 695 type parameters.
        (
            "fn checked<T>(work) = work()\n",
            "def checked[T](work):\n    return work()\n",
        ),
        (
            "fn run(work, channel, value) = { let task = spawn work()\n send(channel, value)\n yield\n let received = recv(channel)\n await(task) }\nlet newer = point { x: 2 }\n",
            "def run(work, channel, value):\n    task = spawn(work)\n    send(channel, value)\n    yield_fiber()\n    received = recv(channel)\n    return await_fiber(task)\nnewer = update(point, x=2)\n",
        ),
        // [FLAVOR-PYTHON-EXPRESSIONS]: `unwrap_or(result, fallback)` is Default
        // `result ?: fallback`, so the fallback stays an arm evaluated only on
        // Error and a `gpuGet` read keeps its device gather form.
        (
            "fn at(b, i) = gpuGet(b, i) ?: 0.0\nfn rank(ranks, key) = mapGet(ranks, key) ?: expensive()\n",
            "def at(b, i):\n    return unwrap_or(gpuGet(b, i), 0.0)\ndef rank(ranks, key):\n    return unwrap_or(mapGet(ranks, key), expensive())\n",
        ),
        (
            "fn total(items) = { mut result = 0\n forEach(items, fn(n) => { result = result + n })\n result }\n",
            "def total(items):\n    result = 0\n    for n in items:\n        result = result + n\n    return result\n",
        ),
        (
            "fn work() = console(fn() => { let message = \"ok\"\n print(message) })\n",
            "def work():\n    with console:\n        message = \"ok\"\n        return print(message)\n",
        ),
        // [FLAVOR-PYTHON-FUNCTIONS] The interop agent reuses this exact boundary.
        (
            "extern fn bridge(operation: string, argument: int) -> string\n",
            "@extern\ndef bridge(operation: str, argument: int) -> str: ...\n",
        ),
        (
            "fn apply(f: fn(int) -> int, value: int) -> int = f(value)\n",
            "from typing import Callable\ndef apply(f: Callable[[int], int], value: int) -> int:\n    return f(value)\n",
        ),
        (
            "fn size(xs: List<int>) -> int = length(xs)\n",
            "from typing import List as Items\ndef size(xs: Items[int]) -> int:\n    return len(xs)\n",
        ),
        // PEP 484 `None` and PEP 695 aliases, no null object.
        (
            "fn noop() -> Unit = {}\ntype Label = string\n",
            "def noop() -> None:\n    return None\ntype Label = str\n",
        ),
    ] {
        equal(default, source);
    }
}

#[test]
fn python_expressions_and_builtins_equal_default_ast() {
    // [FLAVOR-PYTHON-EXPRESSIONS]: operators, literals, matches, comprehensions.
    for (default, source) in [
        (
            "fn pick(a, b, yes) = if yes && !false { a } else { b }\n",
            "def pick(a, b, yes):\n    return a if yes and not False else b\n",
        ),
        (
            "fn absolute(x) = if x < 0 { -x } else { x }\n",
            "def absolute(x):\n    if x < 0:\n        return -x\n    else:\n        return x\n",
        ),
        (
            "let xs = [1, 2]\nlet n = length(xs)\nlet values = range(0, n)\nlet q = intDiv(-7, 3)\nlet mapping = {\"x\": 2}\n",
            "xs = [1, 2]\nn = len(xs)\nvalues = range(n)\nq = -7 // 3\nmapping = {\"x\": 2}\n",
        ),
        (
            "fn read(text) = match parseInt(text) { Success { value } => value\n Error { message } => 0 }\nfn first(xs) = match xs { [head, ...tail] => head\n [] => 0 }\n",
            "def read(text):\n    match parseInt(text):\n        case Success(value=value):\n            return value\n        case Error(message=message):\n            return 0\ndef first(xs):\n    match xs:\n        case [head, *tail]:\n            return head\n        case []:\n            return 0\n",
        ),
        (
            "let xs = toList(map(items, fn(x) => f(x)))\nlet ys = toList(map(filter(items, fn(x) => p(x)), fn(x) => f(x)))\nlet zs = toList(map(range(0, length(xs)), fn(i) => i))\n",
            "xs = [f(x) for x in items]\nys = [f(x) for x in items if p(x)]\nzs = [i for i in range(len(xs))]\n",
        ),
        // [BUILTIN-JSON-TYPED], [FLAVOR-PYTHON-EXPRESSIONS]: the built-in `Json`
        // union has positional payloads, so `JsonObject([...])` and `JsonInt(3)` are
        // saturated constructor applications kept as calls for assembly to fold,
        // and `case JsonNull:` is the nullary variant, not a capture. Both flavors
        // lower the same calls and patterns to the same canonical AST.
        (
            "let doc = JsonObject([JsonMember(\"tags\", JsonArray([JsonString(\"fast\"), JsonString(\"typed\")]))])\nfn kind(json) = match json { JsonObject(members) => length(members)\n JsonArray(items) => length(items)\n JsonInt(n) => n\n JsonNull => 0\n _ => 0 }\n",
            "doc = JsonObject([JsonMember(\"tags\", JsonArray([JsonString(\"fast\"), JsonString(\"typed\")]))])\ndef kind(json):\n    match json:\n        case JsonObject(members):\n            return len(members)\n        case JsonArray(items):\n            return len(items)\n        case JsonInt(n):\n            return n\n        case JsonNull:\n            return 0\n        case _:\n            return 0\n",
        ),
    ] {
        equal(default, source);
    }
}

#[test]
fn python_effect_metadata_and_requirement_rows_equal_default_ast() {
    for (default, source) in [
        (
            "effect Ask<T> { control once replayable ask: fn(T) -> T }\nstatic effect Tensor<B> { size: fn(B) -> int }\n",
            "@effect\nclass Ask[T]:\n    @control\n    @once\n    @replayable\n    def ask(value: T) -> T: ...\n@static_effect\nclass Tensor[B]:\n    def size(buffer: B) -> int: ...\n",
        ),
        (
            "fn quiet(x) ![] = x\nfn logging(x) ![Log<int> | e] = x\n",
            "@requires()\ndef quiet(x):\n    return x\n@requires(Log[int], tail=\"e\")\ndef logging(x):\n    return x\n",
        ),
    ] {
        equal(default, source);
    }
}

#[test]
fn python_handler_classes_and_their_docs_equal_default_ast() {
    // [FLAVOR-PYTHON-HANDLER]: a handler class is a named handler function,
    // the module idiom `fn h(work) = { handle E { … }\n work() }`, so a module
    // may export it where a module `let` must be a compile-time constant. Its
    // generated parameter is unspellable, so the twin is the Default handler
    // value's closure declared as that function rather than bound by `let`.
    for (default, source) in [
        (
            "effect Ask { control ask: fn() -> int }\nlet Answer = handler Ask { ask => resume(7) }\nfn work() = perform Ask.ask()\nlet result = Answer(work)\n",
            "@effect\nclass Ask:\n    @control\n    def ask() -> int: ...\n@handler(Ask)\nclass Answer:\n    def ask():\n        return resume(7)\ndef work():\n    return perform(Ask.ask())\nresult = Answer(work)\n",
        ),
        (
            "namespace app;\nmodule Api {\n/// A capability.\nexport effect Log {\n/// Writes a message.\nwrite: fn(string) -> Unit }\n/// A console.\nlet Console = handler Log { write message => print(message) }\n/// A foreign call.\nexport extern fn foreign(value: int) -> int\n}\n",
            "# osprey: namespace=app\n# osprey: module=Api\n@export\n@effect\nclass Log:\n    \"\"\"A capability.\"\"\"\n    def write(message: str) -> None:\n        \"\"\"Writes a message.\"\"\"\n        ...\n@handler(Log)\nclass Console:\n    \"\"\"A console.\"\"\"\n    def write(message):\n        return print(message)\n@extern\n@export\ndef foreign(value: int) -> int:\n    \"\"\"A foreign call.\"\"\"\n    ...\n",
        ),
    ] {
        equal_with_handler_functions(default, source);
    }
}

#[test]
fn python_typing_aliases_remain_lexically_scoped() {
    equal(
        "fn inner() = {}\nfn outer(value: Items<int>) = value\n",
        "def inner():\n    from typing import List as Items\n    pass\ndef outer(value: Items[int]):\n    return value\n",
    );
    rejects(&["from collections.abc import Final\n"]);
}

#[test]
fn python_multiline_string_tabs_are_content() {
    equal(
        "let text = \"line\\n\\tcontent\\n\"\n",
        "text = \"\"\"line\n\tcontent\n\"\"\"\n",
    );
    rejects(&["def f():\n\treturn 1\n"]);
}

#[test]
fn python_classes_records_and_variants_equal_default_ast() {
    // [FLAVOR-PYTHON-CLASS]: records, stored fields and sum-type variants.
    for (default, source) in [
        (
            "type Box<T> = { value: T }\nlet box = Box<int> { value: 42 }\n",
            "class Box[T]:\n    value: T\nbox = Box[int](value=42)\n",
        ),
        (
            "type Price = { value: int, discount: (int) -> int }\n",
            "class Price:\n    value: int\n    def discount(amount: int) -> int: ...\n",
        ),
        (
            "type Shape = Circle { r: float } | Square { side: float }\n",
            "@union\nclass Shape:\n    class Circle:\n        r: float\n    class Square:\n        side: float\n",
        ),
        (
            "type Shape = Circle { r: float } | Square { side: float }\nlet c = Circle { r: 1.0 }\nlet s = Square { side: 2.0 }\nfn area(shape) = match shape { Circle { r } => 3.0 * r * r\n Square { side } => side * side }\n",
            "@union\nclass Shape:\n    class Circle:\n        r: float\n    class Square:\n        side: float\nc = Circle(r=1.0)\ns = Square(side=2.0)\ndef area(shape):\n    match shape:\n        case Circle(r=r):\n            return 3.0 * r * r\n        case Square(side=side):\n            return side * side\n",
        ),
        (
            "import shapes as Shapes\nfn area(shape) = match shape { Shapes::Circle { r } => r }\n",
            "import shapes as Shapes\ndef area(shape):\n    match shape:\n        case Shapes.Circle(r=r):\n            return r\n",
        ),
    ] {
        equal(default, source);
    }
}

#[test]
fn python_modules_imports_and_docs_equal_default_ast() {
    for (default, source) in [
        (
            "namespace gpt2;\nimport core::Tensor::{build as make}\nimport core::Model as Model\nmodule PythonModel { export let WIDTH = 768\nexport fn forward(x) = x\nfn privateHelper(x) = x }\n",
            "# osprey: namespace=gpt2\n# osprey: module=PythonModel\nfrom core.Tensor import build as make\nimport core.Model as Model\n__all__ = [\"WIDTH\"]\nWIDTH = 768\n@export\ndef forward(x):\n    return x\ndef privateHelper(x):\n    return x\n",
        ),
        (
            "import core::Model as Model\nfn load() = Model::load()\nfn read(Model) = Model.value\n",
            "import core.Model as Model\ndef load():\n    return Model.load()\ndef read(Model):\n    return Model.value\n",
        ),
        (
            "import data::Values as V\nfn keep(value: V::Box<int>) -> V::Box<int> ![V::Read<int>] = value\n",
            "import data.Values as V\n@requires(V.Read[int])\ndef keep(value: V.Box[int]) -> V.Box[int]:\n    return value\n",
        ),
        (
            "//! File documentation.\n/// A value.\ntype Value = { number: int }\n/// Returns its argument.\nfn identity(x) = x\n",
            "\"\"\"File documentation.\"\"\"\nclass Value:\n    \"\"\"A value.\"\"\"\n    number: int\ndef identity(x):\n    \"\"\"Returns its argument.\"\"\"\n    return x\n",
        ),
        // [FLAVOR-PYTHON-CST]: the leading triple-quoted string documents the
        // module the file contributes, not the file alone.
        (
            "namespace gpt2;\nmodule Backends {\n//! Loads tensors.\nexport let WIDTH = 768\n}\n",
            "# osprey: namespace=gpt2\n# osprey: module=Backends\n\"\"\"Loads tensors.\"\"\"\n__all__ = [\"WIDTH\"]\nWIDTH = 768\n",
        ),
        // PEP 257 attribute docstrings ([FLAVOR-PYTHON-CST]).
        (
            "namespace gpt2;\nmodule Api {\n/// Model width.\nexport let WIDTH = 768\n/// A label.\nexport type Label = string\n}\n",
            "# osprey: namespace=gpt2\n# osprey: module=Api\n__all__ = [\"WIDTH\", \"Label\"]\nWIDTH = 768\n\"\"\"Model width.\"\"\"\ntype Label = str\n\"\"\"A label.\"\"\"\n",
        ),
    ] {
        equal(default, source);
    }
}

#[test]
fn python_partial_tail_returns_are_rejected_even_for_unit() {
    rejects(&[
        "def f(x):\n    if x:\n        return None\n",
        "def f(x):\n    if x:\n        return\n    else:\n        pass\n",
        "def f(x):\n    match x:\n        case True:\n            return\n        case False:\n            pass\n",
    ]);
}

#[test]
fn python_intrinsics_reject_invalid_arity_and_context_managers() {
    rejects(&[
        "perform()\n",
        "resume(1, 2)\n",
        "spawn()\n",
        "await_fiber()\n",
        "yield_fiber(1, 2)\n",
        "send(1)\n",
        "recv()\n",
        "update()\n",
        "update(a, b, x=2)\n",
        "unwrap_or(result)\n",
        "unwrap_or(result, 0, 1)\n",
        "unwrap_or(result, fallback=0)\n",
        "with handler as value:\n    pass\n",
        "with first, second:\n    pass\n",
    ]);
}

#[test]
fn python_file_metadata_rejects_late_duplicate_or_invalid_declarations() {
    rejects(&[
        "x = 1\n# osprey: namespace=late\n",
        "# osprey: namespace=one\n# osprey: namespace=two\n",
        "# osprey: flavor=python\n# osprey: flavor=python\n",
        "# osprey: module=Alone\n",
        "# osprey: namespace=bad-name\n",
        "# osprey: namespace=one\n# osprey: module=Two\n__all__ = [\"missing\"]\n",
        "# osprey: namespace=one\n# osprey: module=Two\n__all__ = [\"x\", \"x\"]\nx = 1\n",
        "@export\ndef alone():\n    return 1\n",
        "from .relative import thing\n",
        "from logical import *\n",
    ]);
}

#[test]
fn python_rejects_unsupported_semantics_at_original_source_positions() {
    rejects_with_positions(&[
        "x = ...\n",
        "x = (1, 2)\n",
        "while True:\n    pass\n",
        "def f(x=1):\n    return x\n",
        "def f(*xs):\n    return xs\n",
        "from typing import Final\nx: Final = 1\nx = 2\n",
        "x = [1]\nx[0] = 2\n",
        "def f(x):\n    return x\n    print(x)\n",
        "@unknown\ndef f():\n    pass\n",
        "x = 1 < 2 < 3\n",
        "x = 9223372036854775808\n",
        "x = 1e999\n",
        "async def f():\n    return 1\n",
        "s = {x for x in xs}\n",
        "d = {k: v for k, v in pairs}\n",
        "g = (x for x in xs)\n",
        "def f(xs):\n    for a, b in xs:\n        pass\n",
        "def f(xs):\n    for x in xs:\n        pass\n    else:\n        pass\n",
        "def f(xs):\n    async for x in xs:\n        pass\n",
        "def f(xs):\n    while True:\n        pass\n",
    ]);
}

#[test]
fn python_reports_malformed_indentation_and_incomplete_cst_nodes() {
    rejects(&[
        "def f(:\n",
        "def f():\n\treturn 1\n",
        "def f():\n    x = 1\n  return x\n",
        "match a:\n    case 1:\n        x\n  case 2:\n        y\n",
    ]);
}

/// Whether `source` gets an indentation diagnostic, as opposed to any other.
fn refuses_indentation(source: &str) -> bool {
    let parsed = parse(source);
    parsed
        .errors
        .iter()
        .any(|error| error.message.contains("indentation"))
}

/// [FLAVOR-PYTHON-CST] Every case agrees with `compile` in `CPython`: the first list
/// raises `IndentationError` there, and the second compiles.
#[test]
fn python_indentation_matches_cpython_for_every_statement_and_clause() {
    for source in [
        "if False:\n    print(\"hidden\")\n  print(\"leaked\")\n",
        "if a:\n    if b:\n        x\n      y\n",
        "x = 1\n  y = 2\n",
        "  x = 1\n",
        "if a:\n    x\n  else:\n    y\n",
        "if a:\n    x\n  elif b:\n    y\n",
        "for i in a:\n    x\n  else:\n    y\n",
        "def f():\n    return 1\n  def g():\n    return 2\n",
        "if a: x = 1\n  y = 2\n",
        "class A:\n    x = 1\n  y = 2\n",
        "if a:\n    x\n    y\n  z\nw\n",
        "@dec\n  def f():\n    pass\n",
    ] {
        assert!(refuses_indentation(source), "accepted {source:?}");
    }
    for source in [
        "if a:\n    x\n  # odd comment\ny\n",
        "if a:\n  x\nelse:\n      y\n",
        "if a:\n    if b:\n        x\n    y\nz\n",
        "x = [\n      1,\n  2,\n]\ny = 1\n",
        "def f(\n        a,\n  b):\n    return a\n",
        "if a: x = 1\ny = 2\n",
        "x = 1; y = 2\n",
        "if a:\n    x = 1; y = 2\n    z = 3\n",
        "x = 1 + \\\n      2\ny = 3\n",
        "s = \"\"\"\n  odd\n      lines\n\"\"\"\nt = 1\n",
        "if a:\n    x\n   \n\ny\n",
        "if a:\n    s = \"\"\"\n  odd\n\"\"\"\n    t = 1\n",
        "# first\n   # second\nx = 1\n",
        "@dec\ndef f():\n    pass\n",
    ] {
        assert!(!refuses_indentation(source), "refused {source:?}");
    }
}

#[test]
fn python_classes_and_externs_reject_python_object_or_dynamic_decorator_behavior() {
    rejects(&[
        "class A(Base):\n    value: int\n",
        "class A:\n    value: int = 1\n",
        "class A:\n    def method(self):\n        return 1\n",
        "class A:\n    def method(x): ...\n",
        "@extern\ndef f(x): ...\n",
        "@extern\ndef f(x: int) -> int:\n    return x\n",
        "@python(\"math\", \"exp\")\ndef exp(x: float) -> float: ...\n",
    ]);
}

#[test]
fn python_variant_pattern_renames_bind_the_written_name() {
    // Labels select slots; only the written binders enter scope.
    assert_eq!(
        match_patterns("@union\nclass Shape:\n    class Circle:\n        r: float\n    class Square:\n        side: float\ndef area(shape):\n    match shape:\n        case Circle(r=r):\n            return 3.0 * r * r\n        case Square(side=x):\n            return x * x\n"),
        vec![
            constructor("Circle", "r", vec![]),
            constructor(
                "Square",
                "side",
                vec![osprey_ast::Pattern::Binding("x".into())]
            ),
        ]
    );
}

#[test]
fn python_qualified_pascal_case_calls_with_positional_arguments_stay_calls() {
    // A port's constructor functions carry the original library's class names.
    // Imported declaration identity is resolved after parsing.
    equal(
        "import logits_process as LogitsProcess\nlet w = LogitsProcess::TopKLogitsWarper(3, fv, 1)\n",
        "import logits_process as LogitsProcess\nw = LogitsProcess.TopKLogitsWarper(3, fv, 1)\n",
    );
    // No arguments cannot distinguish an imported factory from a constructor.
    let parsed = parsed("import shapes as Shapes\ne = Shapes.Empty()\n");
    let Some(Stmt::Let { value, .. }) = parsed
        .program
        .statements
        .iter()
        .find(|stmt| matches!(stmt, Stmt::Let { name, .. } if name == "e"))
    else {
        panic!("expected the `e` binding");
    };
    assert!(
        matches!(value, Expr::Call { function, arguments, named_arguments }
            if matches!(function.as_ref(), Expr::Path(path) if path.to_string() == "Shapes::Empty")
                && arguments.is_empty() && named_arguments.is_empty()),
        "expected an unresolved qualified call, got {value:?}"
    );
}

#[test]
fn python_rejects_bindings_that_shadow_builtin_renames() {
    // `len` lowers to `length` and `//` lowers to `intDiv`; a binding that
    // shadows either target would silently redirect the lowering, so it is
    // rejected with a diagnostic naming both names.
    rejects_containing(
        "shadows the builtin",
        &[
            "length = 5\nn = len(xs)\n",
            "def length(xs):\n    return 1\n",
            "intDiv = 5\nq = a // b\n",
        ],
    );
}

#[test]
fn python_rejects_rebinding_every_reserved_intrinsic() {
    // [FLAVOR-PYTHON-EXPRESSIONS]: call sites dispatch these names to the
    // frontend intrinsic first, so a user binding or `def` would never run.
    for name in [
        "perform",
        "resume",
        "update",
        "unwrap_or",
        "spawn",
        "await_fiber",
        "yield_fiber",
        "send",
        "recv",
    ] {
        for source in [
            format!("{name} = 5\n"),
            format!("def {name}(x):\n    return x\n"),
        ] {
            let parsed = parse(&source);
            assert!(
                parsed.errors.iter().any(
                    |error| error.message == format!("`{name}` is a reserved Osprey intrinsic")
                ),
                "silently accepted {source}: {:?}",
                parsed.errors
            );
        }
    }
}

#[test]
fn python_update_accepts_an_expression_record_argument() {
    // [FLAVOR-PYTHON-EXPRESSIONS]: `update(expr, field=value)` binds `expr`
    // once to a generated name, then updates that binding, because the
    // canonical `Expr::Update` names a record variable, never an expression.
    let parsed = parsed("def f():\n    return update(blank(), kind=\"x\")\n");
    let Some(record) = find_update_record(&parsed.program) else {
        panic!("expected an `Expr::Update`");
    };
    assert_eq!(record, "$upd0", "the generated record binding");
}

#[test]
fn python_rejects_nested_function_definitions() {
    rejects_containing(
        "nested `def`",
        &[
            "def outer():\n    def inner():\n        return 1\n    return inner()\n",
            "def outer():\n    f = lambda: 1\n    def inner():\n        return 2\n    return f()\n",
        ],
    );
}

#[test]
fn python_result_pattern_renames_bind_the_written_name() {
    assert_eq!(
        match_patterns("def read(text):\n    match parseInt(text):\n        case Success(value=v):\n            return v\n        case Error(message=message):\n            return 0\n"),
        vec![
            constructor(
                "Success",
                "value",
                vec![osprey_ast::Pattern::Binding("v".into())]
            ),
            constructor("Error", "message", vec![]),
        ]
    );
}

#[test]
fn python_empty_variant_patterns_destructure_nothing() {
    let patterns = match_patterns(
        "@union\nclass Shape:\n    class Nothing:\n        pass\n    class Point:\n        x: int\ndef kind(shape):\n    match shape:\n        case Nothing():\n            return 0\n        case Point(x=x):\n            return x\n",
    );
    let Some(first_arm) = patterns.first() else {
        panic!("expected at least one arm")
    };
    assert!(matches!(
        first_arm,
        osprey_ast::Pattern::Constructor { name, fields, sub_patterns }
            if name == "Nothing" && fields.is_empty() && sub_patterns.is_empty()
    ));
}

#[test]
fn python_nested_comprehensions_lower_to_nested_folds() {
    let parsed = parsed("ys = [x + y for x in xs for y in items if x > 0]\n");
    let Some(Stmt::Let { value, .. }) = parsed.program.statements.first() else {
        panic!("expected a binding")
    };
    let Expr::Call {
        function,
        arguments,
        ..
    } = value
    else {
        panic!("expected the outer fold call")
    };
    assert_eq!(function.as_ref(), &Expr::Identifier("fold".into()));
    let [_, seed, callback] = arguments.as_slice() else {
        panic!("fold takes three arguments")
    };
    assert!(matches!(seed, Expr::List(..)), "outer seed is a list");
    let Expr::Lambda {
        parameters, body, ..
    } = callback
    else {
        panic!("expected an accumulator callback")
    };
    assert_eq!(parameters.len(), 2);
    let Expr::Call { function, .. } = body.as_ref() else {
        panic!("expected the inner fold")
    };
    assert_eq!(function.as_ref(), &Expr::Identifier("fold".into()));
}

#[test]
fn python_fstrings_retain_expression_structure_and_source_positions() {
    let parsed = parsed("def label(value):\n    return f\"λ={value + 1}\"\n");
    equal(
        "let text = \"value=${1 + 2}\"\n",
        "text = f\"value={1 + 2}\"\n",
    );
    let dump = format!("{:#?}", parsed.program);
    assert!(dump.contains("line: 2"), "{dump}");
    assert!(dump.contains("column: 17"), "{dump}");
}

#[test]
fn typed_python_reassignment_and_final_use_existing_binding_kinds() {
    equal(
        "mut count = 1\ncount = 2\nlet limit: int = 3\n",
        "from typing import Final\ncount = 1\ncount = 2\nlimit: Final[int] = 3\n",
    );
    let invalid = parse("from typing import Final\nlimit: Final[int] = 3\nlimit = 4\n");
    assert!(
        invalid
            .errors
            .iter()
            .any(|error| error.message.contains("limit") && error.message.contains("Final")),
        "{:?}",
        invalid.errors
    );
}

/// The patterns of the first match expression in `source`.
fn match_patterns(source: &str) -> Vec<osprey_ast::Pattern> {
    python_match_arms(source)
        .into_iter()
        .map(|arm| arm.pattern)
        .collect()
}

/// A `Name(field=sub)` constructor pattern, for pattern assertions.
fn constructor(
    name: &str,
    field: &str,
    sub_patterns: Vec<osprey_ast::Pattern>,
) -> osprey_ast::Pattern {
    osprey_ast::Pattern::Constructor {
        name: name.into(),
        fields: vec![field.into()],
        sub_patterns,
    }
}

fn python_match_arms(source: &str) -> Vec<osprey_ast::MatchArm> {
    let parsed = parsed(source);
    let arms = find_match_arms(&parsed.program);
    assert!(arms.is_some(), "expected a match expression");
    arms.unwrap_or_default()
}

fn find_match_arms(program: &osprey_ast::Program) -> Option<Vec<osprey_ast::MatchArm>> {
    find_expr(program, &|expr| match expr {
        Expr::Match { arms, .. } => Some(arms.clone()),
        _ => None,
    })
}

fn find_update_record(program: &osprey_ast::Program) -> Option<String> {
    find_expr(program, &|expr| match expr {
        Expr::Update { record, .. } => Some(record.clone()),
        _ => None,
    })
}

/// The expression body of a `let` or `fn` statement.
fn stmt_body(stmt: &Stmt) -> Option<&Expr> {
    match stmt {
        Stmt::Function { body, .. } | Stmt::Let { value: body, .. } => Some(body),
        _ => None,
    }
}

/// Depth-first search of every statement body in `program`.
fn find_expr<T>(program: &osprey_ast::Program, find: &dyn Fn(&Expr) -> Option<T>) -> Option<T> {
    program
        .statements
        .iter()
        .filter_map(stmt_body)
        .find_map(|body| search_expr(body, find))
}

fn search_expr<T>(expr: &Expr, find: &dyn Fn(&Expr) -> Option<T>) -> Option<T> {
    find(expr).or_else(|| match expr {
        Expr::Block {
            statements, value, ..
        } => statements
            .iter()
            .filter_map(stmt_body)
            .find_map(|body| search_expr(body, find))
            .or_else(|| value.as_ref().and_then(|v| search_expr(v, find))),
        _ => None,
    })
}
