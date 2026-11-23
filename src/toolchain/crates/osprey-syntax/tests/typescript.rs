//! TypeScript concrete syntax lowers directly to the shared Osprey AST.
//! Implements [FLAVOR-TS-BOUNDARY], [FLAVOR-TS-SELECT], [FLAVOR-TS-CST],
//! [FLAVOR-TS-FUNCTIONS], [FLAVOR-TS-TYPES], [FLAVOR-TS-UNIONS],
//! [FLAVOR-TS-EXPRESSIONS], [FLAVOR-TS-INTEROP], [FLAVOR-TS-ENTRY] and
//! [FLAVOR-TS-RECEIVER].
use osprey_syntax::{parse_program_with_flavor, resolve_flavor, Flavor, Parsed};

fn parse(source: &str) -> Parsed {
    parse_program_with_flavor(source, Flavor::TypeScript)
}

/// The lines around the first place two canonical renderings differ.
fn first_difference(actual: &str, expected: &str) -> String {
    let (actual, expected): (Vec<_>, Vec<_>) =
        (actual.lines().collect(), expected.lines().collect());
    let at = actual
        .iter()
        .zip(&expected)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| actual.len().min(expected.len()));
    let window = |lines: &[&str]| {
        let end = lines.len().min(at + 6);
        lines
            .get(at.saturating_sub(6)..end)
            .unwrap_or_default()
            .join("\n")
    };
    format!(
        "--- typescript\n{}\n--- default\n{}",
        window(&actual),
        window(&expected)
    )
}

/// Asserts `source` lowers to the canonical AST of the Default program.
fn equal(default: &str, source: &str) {
    let expected = parse_program_with_flavor(default, Flavor::Default);
    assert!(
        expected.errors.is_empty(),
        "{default}\n{:?}",
        expected.errors
    );
    let actual = parse(source);
    assert!(actual.errors.is_empty(), "{source}\n{:?}", actual.errors);
    let actual = osprey_ast::canonical::without_positions(&actual.program);
    let expected = osprey_ast::canonical::without_positions(&expected.program);
    assert!(
        actual == expected,
        "{source}\n{}",
        first_difference(&actual, &expected)
    );
}

/// Asserts `source` is rejected by a diagnostic containing `needle`, at the
/// one-based `line` and zero-based `column` of the construct it names.
fn rejected_at(source: &str, needle: &str, line: u32, column: u32) {
    let parsed = parse(source);
    let found = parsed
        .errors
        .iter()
        .find(|error| error.message.contains(needle))
        .map(|error| (error.position.line, error.position.column));
    assert_eq!(
        found,
        Some((line, column)),
        "{source}\nexpected `{needle}`, got {:?}",
        parsed.errors
    );
}

#[test]
fn typescript_selection_follows_flag_marker_and_extension() {
    assert_eq!(Flavor::TypeScript.to_string(), "typescript");
    assert_eq!("typescript".parse(), Ok(Flavor::TypeScript));
    assert_eq!(resolve_flavor(None, "api.ts", ""), Ok(Flavor::TypeScript));
    let marked = "// osprey: flavor=typescript\nconst x = 1;\n";
    assert_eq!(
        resolve_flavor(None, "api.txt", marked),
        Ok(Flavor::TypeScript)
    );
    assert_eq!(
        resolve_flavor(None, "api.ts", marked),
        Ok(Flavor::TypeScript)
    );
    // A marker and an extension that disagree are an error unless overridden.
    assert!(resolve_flavor(None, "api.osp", marked).is_err());
    assert!(resolve_flavor(None, "api.ts", "// osprey: flavor=ml\n").is_err());
    assert_eq!(
        resolve_flavor(Some(Flavor::TypeScript), "api.osp", marked),
        Ok(Flavor::TypeScript)
    );
    // Declaration files and the other TypeScript extensions are not sources.
    for other in ["lib.d.ts", "view.tsx", "node.mts", "node.cts"] {
        assert_eq!(
            resolve_flavor(None, other, ""),
            Ok(Flavor::Default),
            "{other}"
        );
    }
    // A marker-shaped line after code selects nothing.
    assert_eq!(
        resolve_flavor(
            None,
            "api.txt",
            "const x = 1;\n// osprey: flavor=typescript\n"
        ),
        Ok(Flavor::Default)
    );
    let unknown = "fsharp".parse::<Flavor>();
    assert!(unknown.is_err_and(|message| message.contains("typescript")));
}

#[test]
fn typescript_functions_bindings_and_types_equal_default_ast() {
    for (default, source) in [
        (
            "fn scale(value, factor) = value * factor\nlet twice = fn(value) => scale(value, 2.0)\nlet result = twice(21.0)\n",
            "function scale(value, factor) {\n  return value * factor;\n}\nconst twice = (value) => scale(value, 2.0);\nconst result = twice(21.0);\n",
        ),
        (
            "fn answer(x) = { let offset = -2\n x + offset }\nlet hex = 255\nlet big = 1000000\n",
            "function answer(x) {\n  const offset = -2;\n  return x + offset;\n}\nconst hex = 0xff;\nconst big = 1_000_000;\n",
        ),
        (
            "fn next(value: int) -> int = value\nfn noop() -> Unit = {}\nfn ratio(a: float, b: float) -> float = a / b\n",
            "function next(value: int): int {\n  return value;\n}\nfunction noop(): void {}\nfunction ratio(a: number, b: number): number {\n  return a / b;\n}\n",
        ),
        (
            "fn checked<T>(work) = work()\nlet picked = first<int>(xs)\n",
            "function checked<T>(work) {\n  return work();\n}\nconst picked = first<int>(xs);\n",
        ),
        (
            "fn apply(f: fn(int) -> int, value: int) -> int = f(value)\nfn size(xs: List<int>, ys: List<string>, zs: List<bool>) -> int = listLength(xs)\n",
            "function apply(f: (value: int) => int, value: int): int {\n  return f(value);\n}\nfunction size(xs: int[], ys: readonly string[], zs: List<boolean>): int {\n  return xs.length;\n}\n",
        ),
        (
            "fn total(items) = { forEach(items, fn(item) => { let shown = show(item) })\n let done = true }\n",
            "function total(items) {\n  for (const item of items) {\n    const shown = show(item);\n  }\n  const done = true;\n}\n",
        ),
        (
            "type Label = string\ntype Lookup = Map<string, int>\nfn keep(value: any) -> any = value\n",
            "type Label = string;\ntype Lookup = Map<string, int>;\nfunction keep(value: unknown): any {\n  return value;\n}\n",
        ),
    ] {
        equal(default, source);
    }
}

#[test]
fn typescript_expressions_equal_default_ast() {
    for (default, source) in [
        (
            "fn pick(a, b, yes) = if yes && !false { a } else { b }\nfn same(a, b) = a == b || a != b\n",
            "function pick(a, b, yes) {\n  return yes && !false ? a : b;\n}\nfunction same(a, b) {\n  return a === b || a !== b;\n}\n",
        ),
        (
            "fn absolute(x) = if x < 0 { -x } else { x }\n",
            "function absolute(x) {\n  if (x < 0) {\n    return -x;\n  } else {\n    return x;\n  }\n}\n",
        ),
        // A guard clause takes the statements after it as its `else`.
        (
            "fn grade(score) = if score >= 90 { \"a\" } else { if score >= 50 { \"b\" } else { \"c\" } }\n",
            "function grade(score) {\n  if (score >= 90) {\n    return \"a\";\n  }\n  if (score >= 50) {\n    return \"b\";\n  }\n  return \"c\";\n}\n",
        ),
        (
            "let xs = [1, 2]\nlet first = xs[0]\nlet rest = a % b - c * d + e\nlet ordered = a <= b && c > d\n",
            "const xs = [1, 2];\nconst first = xs[0];\nconst rest = a % b - c * d + e;\nconst ordered = a <= b && c > d;\n",
        ),
        (
            "let greeting = \"total ${n} of ${toString(m)}\"\nlet plain = \"no holes\"\n",
            "const greeting = `total ${n} of ${toString(m)}`;\nconst plain = `no holes`;\n",
        ),
        (
            "fn read(text) = { let parsed = parseInt(text)\n match parsed { Success { value } => value\n Error { message } => length(message) } }\nlet good = Success { value: 1 }\nlet bad = Error { message: \"no\" }\n",
            "function read(text) {\n  const parsed = parseInt(text);\n  switch (parsed.kind) {\n    case \"success\":\n      return parsed.value;\n    case \"error\":\n      return length(parsed.message);\n  }\n}\nconst good = Success(1);\nconst bad = Error(\"no\");\n",
        ),
        // [TYPE-RECORD-CONTEXT]: with no type in sight the literal is the
        // anonymous record Default writes; the shared checker gives it the
        // record type of the declared context it flows into.
        (
            "let a = { x: 1, y: \"two\" }\n",
            "const a = { x: 1, y: \"two\" };\n",
        ),
        // A reassigned `let` or parameter is a rebinding, as a receiver
        // statement is; a compound assignment reads the old value.
        (
            "fn count(n) = { let total = 0\n let total = total + n\n let total = total * 2\n let n = n - 1\n total + n }\n",
            "function count(n) {\n  let total = 0;\n  total = total + n;\n  total *= 2;\n  n -= 1;\n  return total + n;\n}\n",
        ),
        (
            "fn reset(x) = { let x = 1\n x }\n",
            "function reset(x) {\n  x = 1;\n  return x;\n}\n",
        ),
        // Array methods are the list operations; `reduce` takes its initial
        // value last, as JavaScript does, and `.length` counts list elements.
        (
            "let doubled = mapList(xs, fn(x) => x * 2)\nlet evens = filterList(xs, fn(x) => x % 2 == 0)\nlet total = foldList(xs, 0, fn(sum, x) => sum + x)\nlet size = listLength(xs)\nlet chained = foldList(filterList(mapList(xs, fn(x) => x + 1), fn(x) => x > 2), 0, fn(a, b) => a + b)\nlet nested = listLength(listGet(rows, 0))\n",
            "const doubled = xs.map((x) => x * 2);\nconst evens = xs.filter((x) => x % 2 === 0);\nconst total = xs.reduce((sum, x) => sum + x, 0);\nconst size = xs.length;\nconst chained = xs.map((x) => x + 1).filter((x) => x > 2).reduce((a, b) => a + b, 0);\nconst nested = listGet(rows, 0).length;\n",
        ),
    ] {
        equal(default, source);
    }
}

#[test]
fn typescript_records_unions_and_switches_equal_default_ast() {
    for (default, source) in [
        (
            "type Point = { x: int, y: int }\nlet origin: Point = Point { x: 0, y: 0 }\nlet moved = move(Point { x: 1, y: 2 })\nlet shifted = origin { x: 5 }\n",
            "interface Point {\n  readonly x: int;\n  readonly y: int;\n}\nconst origin: Point = { x: 0, y: 0 };\nconst moved = move({ x: 1, y: 2 } satisfies Point);\nconst shifted = { ...origin, x: 5 };\n",
        ),
        (
            "type Box<T> = { value: T }\nlet box: Box<int> = Box<int> { value: 42 }\nfn wrap(n) -> Box<int> = Box<int> { value: n }\n",
            "interface Box<T> {\n  readonly value: T;\n}\nconst box: Box<int> = { value: 42 };\nfunction wrap(n): Box<int> {\n  return { value: n };\n}\n",
        ),
        (
            "type Price = { value: int, discount: (int) -> int, tax: (int) -> int ![] }\n",
            "interface Price {\n  readonly value: int;\n  discount(amount: int): int;\n  readonly tax: (amount: int) => Requires<int, []>;\n}\n",
        ),
        (
            "type Shape = Circle { r: float } | Square { side: float } | Empty\nlet c = Circle { r: 1.0 }\nlet none = Empty\nfn area(shape) = match shape { Circle { r } => 3.0 * r * r\n Square { side } => side * side\n Empty => 0.0 }\n",
            "type Shape =\n  | { readonly kind: \"circle\"; readonly r: number }\n  | { readonly kind: \"square\"; readonly side: number }\n  | { readonly kind: \"empty\" };\nconst c = { kind: \"circle\", r: 1.0 };\nconst none = { kind: \"empty\" };\nfunction area(shape) {\n  switch (shape.kind) {\n    case \"circle\":\n      return 3.0 * shape.r * shape.r;\n    case \"square\": {\n      return shape.side * shape.side;\n    }\n    case \"empty\":\n      return 0.0;\n  }\n}\n",
        ),
    ] {
        equal(default, source);
    }
}

/// A narrowed field never captures a same-named parameter, local or function:
/// the case binds the field under a fresh name. [FLAVOR-TS-UNIONS]
#[test]
fn typescript_narrowed_fields_never_capture_a_same_named_binding() {
    equal(
        "fn total(value, parsed) = match parsed { Success(parsedValue) => value + parsedValue\n Error => value }\n",
        "function total(value, parsed) {\n  switch (parsed.kind) {\n    case \"success\":\n      return value + parsed.value;\n    case \"error\":\n      return value;\n  }\n}\n",
    );
    let shapes = "type Shape = { readonly kind: \"square\"; readonly side: int } | { readonly kind: \"circle\"; readonly r: int };\n";
    for (source, binders) in [
        (
            "function grow(shape, side) {\n  switch (shape.kind) {\n    case \"square\":\n      return side + shape.side;\n    case \"circle\":\n      return shape.r;\n  }\n}\n",
            ["shapeSide", "r"],
        ),
        (
            "function grow(shape) {\n  switch (shape.kind) {\n    case \"square\": {\n      const side = 2;\n      return side * shape.side;\n    }\n    case \"circle\":\n      return r(shape.r);\n  }\n}\n",
            ["shapeSide", "shapeR"],
        ),
        (
            "function grow(shape, side, shapeSide) {\n  switch (shape.kind) {\n    case \"square\":\n      return side + shapeSide + shape.side;\n    case \"circle\":\n      return shape.r;\n  }\n}\n",
            ["shapeSide_", "r"],
        ),
    ] {
        let parsed = parse(&format!("{shapes}{source}"));
        assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
        let lowered = format!("{:?}", parsed.program);
        let [side, r] = binders;
        for expected in [
            if side == "side" { String::new() } else { format!("fields: [\"side\"], sub_patterns: [Binding(\"{side}\")]") },
            if r == "r" { "fields: [\"r\"], sub_patterns: []".to_owned() } else { format!("fields: [\"r\"], sub_patterns: [Binding(\"{r}\")]") },
            format!("Identifier(\"{side}\")"),
        ] {
            assert!(lowered.contains(&expected), "{source}\nmissing {expected}\n{lowered}");
        }
    }
}

#[test]
fn typescript_modules_imports_and_docs_equal_default_ast() {
    for (default, source) in [
        (
            "namespace shop;\nimport shop::Prices::{total, tax as vat}\nmodule Orders { export let LIMIT = 10\nexport fn charge(x) = total(x)\nfn helper(x) = vat(x) }\n",
            "// osprey: namespace=shop\n// osprey: module=Orders\nimport { total, tax as vat } from \"./prices\";\nexport const LIMIT = 10;\nexport function charge(x) {\n  return total(x);\n}\nfunction helper(x) {\n  return vat(x);\n}\n",
        ),
        // A package default import brings the package's functions into scope:
        // its entry function is called by the alias, its other functions are
        // the methods of its values.
        (
            "import express::Express::*\nfn build() = Router()\nfn create() = express()\n",
            "import server from \"express\";\nfunction build() {\n  return server.Router();\n}\nfunction create() {\n  return server();\n}\n",
        ),
        (
            "//! File documentation.\nimport data::Data::{Box}\n/// A value.\ntype Value = { number: int }\n/// Returns its argument.\n///\n/// Second paragraph.\nfn identity(x) = x\n",
            "/** File documentation. */\nimport { Box } from \"data\";\n/** A value. */\ninterface Value {\n  readonly number: int;\n}\n/**\n * Returns its argument.\n *\n * Second paragraph.\n */\nfunction identity(x) {\n  return x;\n}\n",
        ),
        (
            "namespace web;\nmodule Api {\n//! Serves requests.\n/// Removes a route.\nexport fn delete(app) = app\n}\n",
            "// osprey: namespace=web\n// osprey: module=Api\n/** Serves requests. */\n\n/** Removes a route. */\nfunction remove(app) {\n  return app;\n}\nexport { remove as delete };\n",
        ),
    ] {
        equal(default, source);
    }
}

#[test]
fn typescript_top_level_statements_and_receivers_equal_default_ast() {
    for (default, source) in [
        // [FLAVOR-TS-ENTRY]: top-level statements are `main`, declarations stay.
        (
            "let port = 3000\nfn label() = \"ready\"\nfn main() = { print(label())\n let shown = show(port) }\n",
            "const port = 3000;\nconsole.log(label());\nfunction label() {\n  return \"ready\";\n}\nconst shown = show(port);\n",
        ),
        // [FLAVOR-TS-RECEIVER]: a statement-level method chain rebinds its
        // local; the last one binds nothing because nothing reads it.
        (
            "fn main() = { let app = create()\n let app = app.get(\"/\", home)\n print(\"routes\")\n let app = app.use(log).post(\"/\", make)\n let _ = app.listen(3000) }\n",
            "const app = create();\napp.get(\"/\", home);\nconsole.log(\"routes\");\napp.use(log).post(\"/\", make);\napp.listen(3000);\n",
        ),
        // A function returns the one parameter it updates in place.
        (
            "let show = fn(req, res) => { let res = res.status(201)\n let res = res.send(req.body)\n res }\n",
            "const show = (req, res) => {\n  res.status(201);\n  res.send(req.body);\n};\n",
        ),
        (
            "fn guard(req, res) = match req.ok { true => { let res = res.send(\"yes\")\n res }\n false => { let res = res.status(404)\n let res = res.send(\"no\")\n res } }\n",
            "function guard(req, res) {\n  if (req.ok) {\n    res.send(\"yes\");\n    return;\n  }\n  res.status(404);\n  res.send(\"no\");\n}\n",
        ),
    ] {
        equal(default, source);
    }
}

#[test]
fn typescript_rejects_unsupported_semantics_at_original_positions() {
    for (source, needle, line, column) in [
        ("const x = 1;\nvar y = 2;\n", "`var`", 2, 0),
        ("let x;\n", "binding without a value", 1, 4),
        ("const { a } = point;\n", "destructuring", 1, 6),
        ("const a = b == c;\n", "`==`/`!=`", 1, 10),
        ("const a = b ?? c;\n", "`??`", 1, 10),
        ("const total = xs.reduce((a, b) => a + b);\n", "`reduce` without an initial value", 1, 14),
        ("const a = 1;\na = 2;\n", "`a` is a `const`", 2, 0),
        ("function f(c) {\n  let a = 1;\n  if (c) {\n    a = 2;\n  }\n  return a;\n}\n", "inside a branch", 4, 4),
        ("let a = 1;\nconst read = () => a;\na = 2;\n", "after a function captured", 3, 0),
        ("function f(p) {\n  p.x = 1;\n  return p;\n}\n", "assignment to a field or element", 2, 2),
        ("let a = 1;\nconst f = () => {\n  a = 2;\n};\n", "`a` is not a local of this function", 3, 2),
        ("let a = 1;\nconst b = (a = 2);\n", "assignment expression", 2, 11),
        ("const a = b?.c;\n", "`?.`", 1, 10),
        ("const a = null;\n", "`null`", 1, 10),
        ("const a = undefined;\n", "`undefined`", 1, 10),
        ("const a = new Date();\n", "`new`", 1, 10),
        ("const a = typeof b;\n", "unary operator", 1, 10),
        ("const a = b as int;\n", "type assertion", 1, 10),
        ("const a = b!;\n", "non-null assertion", 1, 10),
        ("const a = function () {};\n", "`function` expression", 1, 10),
        ("const a = async () => 1;\n", "`async`", 1, 10),
        ("const a = (b = 1) => b;\n", "default or optional parameter", 1, 11),
        ("const a = (...b) => b;\n", "rest parameter", 1, 11),
        ("const a = [...b];\n", "spread or hole", 1, 11),
        ("const a = f(...b);\n", "spread argument", 1, 12),
        ("const a = { ...b(), x: 1 };\n", "spread must come first", 1, 15),
        ("const a = { get x() { return 1; } } satisfies P;\n", "method, accessor", 1, 12),
        ("const a = 9223372036854775808;\n", "signed 64-bit range", 1, 10),
        ("function f(x: A | B) {}\n", "union type outside", 1, 14),
        ("function f(x: A & B) {}\n", "intersection type", 1, 14),
        ("function f(x: { a: int }) {}\n", "anonymous object type", 1, 14),
        ("function f<T extends A>(x: T) {}\n", "constrained type parameter", 1, 11),
        ("interface P { x?: int }\n", "optional or computed", 1, 14),
        ("interface P extends Q { x: int }\n", "interface `extends`", 1, 20),
        ("interface P { [key: string]: int }\n", "index, call or construct", 1, 14),
        ("type S = { a: int } | { kind: \"b\" };\n", "needs a `kind` property", 1, 9),
        ("function f() {\n  return 1;\n  g();\n}\n", "follows a `return`", 3, 2),
        ("function f(x) {\n  if (x) {\n    return 1;\n  } else {\n    g();\n  }\n  h();\n  return 2;\n}\n", "early return", 3, 4),
        ("function f() {\n  function g() {}\n}\n", "nested `function`", 2, 2),
        ("function f(x) {\n  while (x) {}\n}\n", "`while` loop", 2, 2),
        ("function f(x) {\n  throw x;\n}\n", "`throw`/`try`", 2, 2),
        ("function f(x) {\n  switch (x) {\n    case 1:\n      return 1;\n  }\n}\n", "must test `value.kind`", 2, 10),
        ("function f(x) {\n  switch (x.kind) {\n    default:\n      return 1;\n  }\n}\n", "`default` is rejected", 3, 4),
        ("function f(x) {\n  switch (x.kind) {\n    case \"a\":\n      g();\n    case \"b\":\n      return 1;\n  }\n}\n", "fallthrough is rejected", 3, 4),
        // Adjacent labels share the next body ([PATTERN-OR-SYNTAX]); a label
        // with no body after it falls through.
        ("function f(x) {\n  switch (x.kind) {\n    case \"a\":\n      return 1;\n    case \"b\":\n  }\n}\n", "must end with a body", 5, 4),
        ("function f(a, b) {\n  a.set(1);\n  b.set(2);\n}\n", "only one parameter", 1, 17),
        ("function f(a, c) {\n  if (c) {\n    a.set(1);\n  }\n  return a;\n}\n", "inside a branch", 3, 4),
        ("function f() {\n  const a = make();\n  const read = () => a.size;\n  a.grow(1);\n  return read;\n}\n", "captured it", 4, 2),
        ("import \"./setup\";\n", "side-effect import", 1, 0),
        ("import * as all from \"express\";\n", "`import *`", 1, 7),
        ("// osprey: namespace=a\nimport x from \"./local\";\n", "default import of a project file", 2, 7),
        ("import { x } from \"./local\";\n", "needs a `// osprey: namespace=`", 1, 18),
        ("export default 1;\n", "`export default`", 1, 0),
        ("function f() {}\nexport { g as h };\n", "is not a function declared", 2, 9),
        ("class A {}\n", "`class`", 1, 0),
        ("enum E { A }\n", "`enum`", 1, 0),
        ("const x = 1;\n// osprey: module=Late\n", "must precede source declarations", 2, 0),
        ("// osprey: module=Orphan\nconst x = 1;\n", "requires a namespace directive", 1, 0),
        ("// osprey: namespace=a\n// osprey: namespace=b\n", "duplicate namespace directive", 2, 0),
        ("const = 1;\n", "invalid TypeScript syntax", 1, 6),
    ] {
        rejected_at(source, needle, line, column);
    }
}

/// [FLAVOR-TS-LIB]: the calls the ambient library defines syntactically.
#[test]
fn typescript_intrinsics_lower_to_the_documented_nodes() {
    let parsed =
        parse("const lookup = new Map<string, int>();\nconsole.log(`size ${[1, 2].length}`);\n");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let rendered = format!("{:?}", parsed.program);
    assert!(rendered.contains("value: Map([])"), "{rendered}");
    assert!(rendered.contains("Identifier(\"print\")"), "{rendered}");
    assert!(
        rendered.contains("Identifier(\"listLength\")"),
        "{rendered}"
    );
    assert!(!rendered.contains("console"), "{rendered}");
    // A local named like an intrinsic is an ordinary value.
    let shadowed = parse("function f(console, Success) {\n  const logged = console.log(1);\n  return Success(2);\n}\n");
    assert!(shadowed.errors.is_empty(), "{:?}", shadowed.errors);
    let rendered = format!("{:?}", shadowed.program);
    assert!(rendered.contains("method: \"log\""), "{rendered}");
    assert!(!rendered.contains("TypeConstructor"), "{rendered}");
    for (source, needle) in [
        ("console.log(1, 2);\n", "exactly one argument"),
        ("const a = Success();\n", "exactly one argument"),
        ("const a = new Map(entries);\n", "only `new Map()`"),
    ] {
        rejected_at(
            source,
            needle,
            1,
            if source.starts_with("console") { 0 } else { 10 },
        );
    }
}
