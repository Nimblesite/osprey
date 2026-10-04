use super::*;

#[test]
fn structured_sections_render_in_hover() {
    let src = "/// Divides two numbers.\n\
                   ///\n\
                   /// # Parameters\n\
                   /// - a: the numerator\n\
                   ///\n\
                   /// # Returns\n\
                   /// the quotient\n\
                   fn div(a, b) = intDiv(a, b)\n";
    let d = doc_for(src, "div").expect("div doc");
    assert!(d.contains("Divides two numbers."), "{d}");
    assert!(d.contains("**Parameters**") && d.contains("`a`"), "{d}");
    assert!(
        d.contains("**Returns**") && d.contains("the quotient"),
        "{d}"
    );
}

#[test]
fn ml_flavor_doc_comments_reach_hover() {
    // The ML (** … *) doc form lowers to the same DocComment and renders
    // identically ([DOC-SIGIL-ML]).
    let src = "(** Doubles the input. *)\n\
                   double x = x * 2\n\
                   (** A performance tier. *)\n\
                   type Tier =\n    Epic\n    Solid\n";
    let parsed = osprey_syntax::parse_program_with_flavor(src, osprey_syntax::Flavor::Ml);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let syms = collect_all_symbols(&parsed.program);
    assert_doc(&syms, "double", "Doubles the input.");
    assert_doc(&syms, "Tier", "A performance tier.");
}

#[test]
fn hover_renders_builtin_signature_and_rejects_unknowns() {
    // [BUILTIN-PRINT] Hover uses the constrained public signature, not the
    // internal `any` scheme used to implement receiver dispatch.
    let md = builtin_hover("print");
    // The rich hover carries the call signature and the description, not just
    // a bare `name : type` line.
    assert!(
            md.as_deref().is_some_and(|m| m.contains(
                "print(value: int | float | bool | string | Unit | any | Result<printable, printable>) -> Unit"
            ) && m.contains("supported scalar or Result")),
            "{md:?}"
        );
    assert!(builtin_hover("notARealBuiltin").is_none());
}

#[test]
fn json_strings_escape_quotes_and_control_chars() {
    assert_eq!(json_str("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    assert_eq!(json_str("\u{1}"), "\"\\u0001\"");
    // Carriage return and tab get their own short escapes.
    assert_eq!(json_str("a\rb\tc"), "\"a\\rb\\tc\"");
}

#[test]
fn render_type_covers_named_array_generic_and_function_forms() {
    use osprey_ast::TypeExpr;
    // A bare named type renders as its name.
    assert_eq!(render_type(&TypeExpr::named("int")), "int");

    // An array type renders as `[element]`, and the empty array as `[]`.
    let mut array = TypeExpr::named("");
    array.is_array = true;
    array.array_element = Some(Box::new(TypeExpr::named("string")));
    assert_eq!(render_type(&array), "[string]");
    let mut bare_array = TypeExpr::named("");
    bare_array.is_array = true;
    assert_eq!(render_type(&bare_array), "[]");

    // A generic type renders as `Name<args>`.
    let mut generic = TypeExpr::named("Result");
    generic.generic_params = vec![TypeExpr::named("int"), TypeExpr::named("string")];
    assert_eq!(render_type(&generic), "Result<int, string>");

    // A function type renders parameter and return types; a missing return
    // type defaults to `Unit`.
    let mut func = TypeExpr::named("");
    func.is_function = true;
    func.parameter_types = vec![TypeExpr::named("int")];
    func.return_type = Some(Box::new(TypeExpr::named("bool")));
    assert_eq!(render_type(&func), "fn(int) -> bool");
    let mut func_unit = TypeExpr::named("");
    func_unit.is_function = true;
    assert_eq!(render_type(&func_unit), "fn() -> Unit");
}

#[test]
fn collect_symbols_qualifies_module_members_and_skips_non_declarations() {
    // [MODULES-ABI] A flat wire outline keeps collision-safe source names:
    // the module itself plus qualified members, never flattened leaves.
    let parsed = osprey_syntax::parse_program(
        "module Inner {\n  fn helper() -> int = 1\n  let seed = 2\n}\n",
    );
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let syms = collect_symbols(&parsed.program);
    let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Inner", "Inner::helper", "Inner::seed"]);
    // The `let` carries no annotation, so its rendered type is empty and its
    // kind is `Variable`.
    let seed = syms
        .iter()
        .find(|s| s.name == "Inner::seed")
        .expect("seed symbol");
    assert_eq!(seed.kind, SymbolKind::Variable);
    assert_eq!(seed.ty, "");
}

#[test]
fn namespace_module_and_signature_symbols_never_flatten_collisions() {
    // [MODULES-NAMESPACE] Two modules may export the same leaf name. The
    // outline preserves both ownership paths and the container kinds.
    let parsed = osprey_syntax::parse_program(
        "namespace sales { module Tax { export fn rate() = 10 } }\n\
             namespace payroll { module Tax { export fn rate() = 20 } }\n\
             signature TaxSig { fn rate() -> int }\n",
    );
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let symbols = collect_symbols(&parsed.program);
    let named = |name: &str| symbols.iter().find(|symbol| symbol.name == name);
    assert_eq!(named("sales").map(|s| s.kind), Some(SymbolKind::Namespace));
    assert_eq!(
        named("sales::Tax").map(|s| s.kind),
        Some(SymbolKind::Module)
    );
    assert!(named("sales::Tax::rate").is_some());
    assert!(named("payroll::Tax::rate").is_some());
    assert_eq!(named("TaxSig").map(|s| s.kind), Some(SymbolKind::Signature));
}

/// One of every container `Expr` variant, each holding a block whose single
/// `let` is named for the slot it sits in — the fixture for the deep-walker
/// test below. Implements [LSP-HOVER-VARIABLES]
#[expect(
    clippy::too_many_lines,
    reason = "exhaustive fixture: one arm per AST container variant is the point"
)]
fn every_container_with_a_nested_let() -> Vec<Expr> {
    use osprey_ast::{
        Expr, FieldAssignment, HandlerArm, MapEntry, MatchArm, NamedArgument, Pattern,
    };
    let blk = |name: &str| Expr::Block {
        statements: vec![Stmt::Let {
            name: name.into(),
            mutable: false,
            ty: None,
            value: Expr::Integer(0),
            doc: None,
            position: Some(Position { line: 1, column: 0 }),
        }],
        value: None,
    };
    let b = |name: &str| Box::new(blk(name));
    let narg = |name: &str| NamedArgument {
        name: "n".into(),
        value: blk(name),
    };
    let field = |name: &str| FieldAssignment {
        name: "f".into(),
        value: blk(name),
    };
    let arm = |name: &str| MatchArm {
        pattern: Pattern::Wildcard,
        body: blk(name),
    };
    vec![
        Expr::List(vec![blk("list")], None),
        Expr::Map(vec![MapEntry {
            key: blk("mapk"),
            value: blk("mapv"),
        }]),
        Expr::Object(vec![field("obj")]),
        Expr::Binary {
            position: None,
            op: "+".into(),
            left: b("binl"),
            right: b("binr"),
        },
        Expr::Pipe {
            left: b("pipel"),
            right: b("piper"),
        },
        Expr::Unary {
            op: "-".into(),
            operand: b("unary"),
        },
        Expr::InterpolatedStr(vec![InterpolatedPart::Expr(blk("interp"))]),
        Expr::Call {
            function: b("callfn"),
            arguments: vec![blk("callarg")],
            named_arguments: vec![narg("callnamed")],
        },
        Expr::MethodCall {
            target: b("mtarget"),
            method: "m".into(),
            arguments: vec![blk("marg")],
            named_arguments: vec![narg("mnamed")],
        },
        Expr::FieldAccess {
            target: b("fatarget"),
            field: "f".into(),
        },
        Expr::Index {
            target: b("idxt"),
            index: b("idxi"),
        },
        Expr::Lambda {
            parameters: Vec::new(),
            return_type: None,
            body: b("lambda"),
            position: None,
        },
        Expr::Match {
            value: b("matchval"),
            arms: vec![arm("matcharm")],
        },
        Expr::TypeConstructor {
            name: "T".into(),
            type_args: Vec::new(),
            fields: vec![field("tc")],
        },
        Expr::Update {
            record: "r".into(),
            fields: vec![field("update")],
        },
        Expr::Spawn(b("spawn")),
        Expr::Await(b("await")),
        Expr::Recv(b("recv")),
        Expr::Yield(Some(b("yield"))),
        Expr::Send {
            channel: b("sendc"),
            value: b("sendv"),
        },
        Expr::Select {
            arms: vec![arm("select")],
        },
        Expr::Perform {
            effect: "E".into(),
            operation: "op".into(),
            arguments: vec![blk("perform")],
            named_arguments: vec![narg("performnamed")],
            position: None,
        },
        Expr::Handler {
            stage: osprey_ast::Stage::Dynamic,
            effect: "E".into(),
            arms: vec![HandlerArm {
                operation: "op".into(),
                params: Vec::new(),
                body: blk("handlerarm"),
                position: None,
            }],
            body: b("handlerbody"),
            return_clause: None,
            position: None,
        },
    ]
}

#[test]
fn collect_all_symbols_descends_into_every_expression_form() {
    // A `let` is buried inside each container expression variant; the deep
    // collector must surface every one — this exercises all walker arms.
    // Implements [LSP-HOVER-VARIABLES]
    let program = Program {
        statements: every_container_with_a_nested_let()
            .into_iter()
            .map(|value| Stmt::Expr {
                value,
                doc: None,
                position: None,
            })
            .collect(),
        doc: None,
    };
    let found: Vec<String> = collect_all_symbols(&program)
        .into_iter()
        .map(|s| s.name)
        .collect();
    for expected in [
        "list",
        "mapk",
        "mapv",
        "obj",
        "binl",
        "binr",
        "pipel",
        "piper",
        "unary",
        "interp",
        "callfn",
        "callarg",
        "callnamed",
        "mtarget",
        "marg",
        "mnamed",
        "fatarget",
        "idxt",
        "idxi",
        "lambda",
        "matchval",
        "matcharm",
        "tc",
        "update",
        "spawn",
        "await",
        "recv",
        "yield",
        "sendc",
        "sendv",
        "select",
        "perform",
        "performnamed",
        "handlerarm",
        "handlerbody",
    ] {
        assert!(found.iter().any(|n| n == expected), "missing `{expected}`");
    }
}

#[test]
fn symbol_kind_as_str_round_trips_each_variant() {
    assert_eq!(SymbolKind::Namespace.as_str(), "namespace");
    assert_eq!(SymbolKind::Module.as_str(), "module");
    assert_eq!(SymbolKind::Signature.as_str(), "signature");
    assert_eq!(SymbolKind::Function.as_str(), "function");
    assert_eq!(SymbolKind::Variable.as_str(), "variable");
    assert_eq!(SymbolKind::Type.as_str(), "type");
}
