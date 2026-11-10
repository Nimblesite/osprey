//! Python indentation, patterns, comprehensions and f-strings lower to the shared AST.
//! Implements [FLAVOR-PYTHON-CST] and [FLAVOR-PYTHON-FUNCTIONS].
use osprey_ast::{Expr, Stmt};

use super::{equal, parse, parsed, rejects, rejects_containing};

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
