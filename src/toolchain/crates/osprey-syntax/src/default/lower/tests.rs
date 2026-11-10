//! Lowering tests.
#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]

use crate::test_support::find_kind;
use crate::test_support::{assert_doc_pair, assert_summary, one_stmt, program, stmt_doc, stmts};
use crate::{parse_program, parse_tree};
use osprey_ast::{Expr, Pattern, Stmt};

// ---------- [TESTING-DOC] expression-statement documentation ----------

#[test]
fn a_doc_comment_lowers_onto_the_expression_statement_it_precedes() {
    // A `test(...)` case is an expression statement, not a declaration, so
    // documenting one requires the doc to attach here ([TESTING-DOC]).
    let s = one_stmt("/// Documents the call.\nprint(\"hi\")\n");
    let doc = stmt_doc(&s).expect("doc attached to the expression statement");
    assert_eq!(doc.summary, "Documents the call.");
    assert!(matches!(s, Stmt::Expr { .. }), "still an expr stmt: {s:?}");
}

#[test]
fn a_documented_expression_statement_keeps_its_value_and_position() {
    // The doc must not displace the lowered expression, and the recorded
    // position must stay on the EXPRESSION — the Test Explorer's gutter
    // marker would otherwise land on the first `///` line.
    let s = one_stmt("/// Line one.\n/// Line two.\ntest(\"named\", 1)\n");
    match &s {
        Stmt::Expr {
            value, position, ..
        } => {
            assert!(
                matches!(value, Expr::Call { .. }),
                "the call survived: {value:?}"
            );
            let position = position.expect("position recorded");
            assert_eq!(position.line, 3, "the `test(` line, not the doc's");
            assert_eq!(position.column, 0);
        }
        other => panic!("expected an expression statement, got {other:?}"),
    }
    // Consecutive `///` lines are one paragraph, so they join into a single
    // summary line ([DOC-MODEL]).
    let doc = stmt_doc(&s).expect("doc attached");
    assert_eq!(doc.summary, "Line one. Line two.");
}

#[test]
fn an_undocumented_expression_statement_carries_no_doc() {
    let s = one_stmt("print(\"hi\")\n");
    assert!(stmt_doc(&s).is_none(), "no doc invented: {s:?}");
}

#[test]
fn each_documented_statement_owns_only_its_own_doc() {
    let all = stmts("/// First.\ntest(\"a\", 1)\ntest(\"b\", 2)\n/// Third.\ntest(\"c\", 3)\n");
    assert_eq!(all.len(), 3);
    assert_summary(&all[0], Some("First."));
    assert_summary(&all[1], None);
    assert_summary(&all[2], Some("Third."));
}

#[test]
fn a_documented_declaration_after_a_documented_statement_keeps_its_own_doc() {
    // Adding the expression-statement slot must not steal a following
    // declaration's doc comment.
    let all = stmts("/// Runs it.\nprint(\"hi\")\n/// Adds.\nfn add(a, b) = a + b\n");
    assert_doc_pair(&all, "Runs it.", "Adds.");
}

#[test]
fn documented_statements_inside_a_block_lower_with_their_docs() {
    let s = one_stmt("fn main() = {\n/// Inner case.\ntest(\"inner\", 1)\n0\n}\n");
    let Stmt::Function { body, .. } = &s else {
        panic!("expected a function, got {s:?}");
    };
    let Expr::Block { statements, .. } = body else {
        panic!("expected a block body, got {body:?}");
    };
    assert_summary(&statements[0], Some("Inner case."));
}

#[test]
fn lowers_record_type_and_array_and_function_types() {
    // record_type definition (lower_type_decl record arm + lower_field_decls)
    match one_stmt("type Point = {\n  x: int,\n  y: int\n}\n") {
        Stmt::Type { name, variants, .. } => {
            assert_eq!(name, "Point");
            assert_eq!(variants.len(), 1);
            assert_eq!(variants[0].fields.len(), 2);
            assert_eq!(variants[0].fields[0].name, "x");
        }
        s => panic!("expected record type, got {s:?}"),
    }
    // A positional payload carries generated slot names, so this Default
    // declaration lowers to the same variants as the ML twin
    // `type Tree = Leaf | Node Tree Tree` ([TYPE-UNION-POSITIONAL]).
    match one_stmt("type Tree = Leaf | Node(Tree, Tree)\n") {
        Stmt::Type { variants, .. } => {
            assert_eq!(variants.len(), 2);
            assert_eq!(variants[0].fields, Vec::<osprey_ast::TypeField>::new());
            let slots: Vec<&str> = variants[1].fields.iter().map(|f| f.name.as_str()).collect();
            assert_eq!(slots, ["0", "1"]);
            assert_eq!(variants[1].fields[0].ty, "Tree");
        }
        s => panic!("expected a union type, got {s:?}"),
    }
    // array_type `Item[int]` (lower_type array_type arm + descendants_type_in),
    // a function type, and a generic type — all in one signature.
    match one_stmt(
        "fn f(xs: Item[int], g: fn(int) -> bool, m: Map<string, int>) -> Item[int] = xs\n",
    ) {
        Stmt::Function {
            parameters,
            return_type,
            ..
        } => {
            let arr = parameters[0].ty.as_ref().unwrap();
            assert!(arr.is_array);
            assert_eq!(arr.array_element.as_ref().unwrap().name, "int");
            let func = parameters[1].ty.as_ref().unwrap();
            assert!(func.is_function);
            assert_eq!(func.return_type.as_ref().unwrap().name, "bool");
            let gen = parameters[2].ty.as_ref().unwrap();
            assert_eq!(gen.generic_params.len(), 2);
            assert!(return_type.unwrap().is_array);
        }
        s => panic!("expected function, got {s:?}"),
    }
}

/// The single match arm's pattern for `match x { <arm> => 0  _ => 1 }`.
fn first_pattern(arm: &str) -> Pattern {
    let src = format!("let r = match x {{ {arm} => 0  _ => 1 }}\n");
    match one_stmt(&src) {
        Stmt::Let {
            value: Expr::Match { mut arms, .. },
            ..
        } => arms.swap_remove(0).pattern,
        s => panic!("expected match, got {s:?}"),
    }
}

#[test]
fn lowers_constructor_type_annotated_negative_and_type_params() {
    // Sub-pattern constructor `Some(inner)` -> identifier arm -> sub_patterns.
    assert!(matches!(
        first_pattern("Some(inner)"),
        Pattern::Constructor { sub_patterns, .. } if sub_patterns.len() == 1
    ));
    // `n: int` -> TypeAnnotated.
    assert!(matches!(
        first_pattern("n: int"),
        Pattern::TypeAnnotated { ref name, .. } if name == "n"
    ));
    // `-1.5` -> negated float literal (drives negate_literal's Float arm).
    assert!(matches!(
        first_pattern("-1.5"),
        Pattern::Literal(b) if matches!(*b, Expr::Float(f) if f < 0.0)
    ));
    // Generic type params on a type declaration (type_parameters field),
    // including variance markers. Implements [TYPE-VARIANCE-DECL].
    match one_stmt("type Foo<T, out U, in V> = Bar | Baz\n") {
        Stmt::Type {
            type_params,
            variants,
            ..
        } => {
            let names: Vec<&str> = type_params.iter().map(|p| p.name.as_str()).collect();
            assert_eq!(names, vec!["T", "U", "V"]);
            let vs: Vec<osprey_ast::Variance> = type_params.iter().map(|p| p.variance).collect();
            assert_eq!(
                vs,
                vec![
                    osprey_ast::Variance::Invariant,
                    osprey_ast::Variance::Covariant,
                    osprey_ast::Variance::Contravariant
                ]
            );
            assert_eq!(variants.len(), 2);
        }
        s => panic!("expected type, got {s:?}"),
    }
    // Fn-level type params and a generic effect declaration.
    // Implements [TYPE-GENERICS-FN] and [EFFECTS-GENERIC-DECL].
    match one_stmt("fn map2<T, U>(f: (T) -> U, x: T) -> U = f(x)\n") {
        Stmt::Function { type_params, .. } => {
            assert_eq!(type_params.len(), 2);
            assert_eq!(type_params[0].name, "T");
        }
        s => panic!("expected function, got {s:?}"),
    }
    match one_stmt("effect State<T> {\n  get: fn() -> T\n}\n") {
        Stmt::Effect {
            type_params,
            operations,
            ..
        } => {
            assert_eq!(type_params.len(), 1);
            assert_eq!(type_params[0].name, "T");
            assert_eq!(operations.len(), 1);
        }
        s => panic!("expected effect, got {s:?}"),
    }
}

/// The summary a `DocComment` carries, or `None` — every inner-doc
/// assertion below asks exactly this of a different scope.
fn summary_of(doc: Option<&osprey_ast::DocComment>) -> Option<&str> {
    doc.map(|d| d.summary.as_str())
}

#[test]
fn inner_docs_attach_to_file_namespace_and_module() {
    // Implements [DOC-SIGIL-INNER]. `//!` documents the scope that ENCLOSES
    // it, so it lands on the file, the namespace or the module body rather
    // than on the declaration that happens to follow it. An outer `///` and
    // an inner `//!` describe the same scope from opposite sides and must
    // therefore coexist, never overwrite one another.
    let file = program("//! The whole file.\nlet x = 1\n");
    assert_eq!(summary_of(file.doc.as_ref()), Some("The whole file."));
    // The declaration after the inner doc keeps its own doc slot empty —
    // an inner doc must never be mistaken for the next declaration's.
    assert_summary(&file.statements[0], None);

    match one_stmt(
        "/// From outside.\n\
         namespace app {\n\
           //! From inside.\n\
           let x = 1\n\
         }\n",
    ) {
        Stmt::Namespace {
            doc,
            inner_doc,
            body,
            ..
        } => {
            assert_eq!(summary_of(doc.as_ref()), Some("From outside."));
            assert_eq!(summary_of(inner_doc.as_ref()), Some("From inside."));
            assert_summary(&body[0], None);
        }
        s => panic!("expected namespace, got {s:?}"),
    }

    match one_stmt(
        "/// From outside.\n\
         module M {\n\
           //! From inside.\n\
           export let x = 1\n\
         }\n",
    ) {
        Stmt::Module { doc, inner_doc, .. } => {
            assert_eq!(summary_of(doc.as_ref()), Some("From outside."));
            assert_eq!(summary_of(inner_doc.as_ref()), Some("From inside."));
        }
        s => panic!("expected module, got {s:?}"),
    }
}

#[test]
fn inner_doc_records_inner_scope_and_leaves_outer_alone() {
    // The `DocScope::Inner` arm exists in the model but was unreachable
    // before `//!` had a grammar rule; these pin which arm each sigil takes.
    let file = program("//! Scoped inward.\nlet x = 1\n");
    assert_eq!(
        file.doc.as_ref().map(osprey_ast::DocComment::scope),
        Some(osprey_ast::DocScope::Inner)
    );
    assert_eq!(
        stmt_doc(&one_stmt("/// Scoped outward.\nlet x = 1\n")).map(osprey_ast::DocComment::scope),
        Some(osprey_ast::DocScope::Outer)
    );
    // A file with no `//!` invents none.
    assert!(program("let x = 1\n").doc.is_none());
}

/// The `file_scoped` flag and outer-doc summary of a documented namespace
/// declaration.
fn namespace_flag_and_doc(src: &str) -> (bool, Option<String>) {
    match one_stmt(src) {
        Stmt::Namespace {
            doc, file_scoped, ..
        } => (file_scoped, summary_of(doc.as_ref()).map(str::to_owned)),
        s => panic!("expected namespace, got {s:?}"),
    }
}

#[test]
fn namespace_keeps_the_outer_doc_the_grammar_already_accepted() {
    // `namespace_declaration` accepted `optional($.doc_comment)` and threw
    // it away at lowering: the doc parsed, then vanished. Both namespace
    // spellings must keep it.
    let (semi_scoped, doc) =
        namespace_flag_and_doc("/// File-scoped namespace.\nnamespace billing;\nlet x = 1\n");
    assert!(semi_scoped);
    assert_eq!(doc.as_deref(), Some("File-scoped namespace."));
    let (braced_scoped, doc) =
        namespace_flag_and_doc("/// Braced namespace.\nnamespace billing { let x = 1 }\n");
    assert!(!braced_scoped);
    assert_eq!(doc.as_deref(), Some("Braced namespace."));
}

#[test]
fn a_stray_default_inner_doc_is_a_syntax_error() {
    // `//!` is a real token, not trivia, so one written where no scope can
    // take it is rejected rather than silently lexed as a `//` comment.
    // The same rule the ML lowerer reports at lowering, the Default grammar
    // enforces at parse time ([DOC-SIGIL-INNER]).
    let trailing = parse_program("let x = 1\n//! nothing encloses me.\n");
    assert!(
        !trailing.errors.is_empty(),
        "a trailing //! must be rejected"
    );
    assert!(
        trailing.program.doc.is_none(),
        "a stray //! must never become the file doc"
    );
    // An ordinary `//` comment in the same position stays trivia.
    assert_eq!(
        parse_program("let x = 1\n// just a comment\n").errors,
        Vec::<crate::SyntaxError>::new()
    );
}

#[test]
fn negate_literal_passes_through_non_numeric() {
    // negate_literal flips numerics and returns non-numeric literals as-is.
    assert_eq!(super::negate_literal(Expr::Integer(3)), Expr::Integer(-3));
    assert_eq!(super::negate_literal(Expr::Float(2.0)), Expr::Float(-2.0));
    assert_eq!(
        super::negate_literal(Expr::Str("x".into())),
        Expr::Str("x".into())
    );
}

#[test]
fn lowers_assignment_effects_structural_and_list_patterns() {
    // Reassignment statement (lower_stmt Assignment arm).
    match one_stmt("x = 5\n") {
        Stmt::Assignment { name, value, .. } => {
            assert_eq!(name, "x");
            assert_eq!(value, Expr::Integer(5));
        }
        s => panic!("expected assignment, got {s:?}"),
    }
    // Function effect clause `! [Log, State<int>]` — effect refs carry
    // optional type arguments. Implements [EFFECTS-GENERIC-ROWS].
    match one_stmt("fn act() ! [Log, State<int>] = 1\n") {
        Stmt::Function { effects, .. } => {
            let names: Vec<&str> = effects.iter().map(|e| e.name.as_str()).collect();
            assert_eq!(names, vec!["Log", "State"]);
            assert_eq!(effects[0].type_args, Vec::<osprey_ast::TypeExpr>::new());
            assert_eq!(effects[1].type_args.len(), 1);
            assert_eq!(effects[1].type_args[0].name, "int");
        }
        s => panic!("expected function, got {s:?}"),
    }
    // Bare structural `{ name, age }` and a fixed-length list `[a, b]`.
    assert!(matches!(
        first_pattern("{ name, age }"),
        Pattern::Structural { fields, open: false }
            if fields.iter().map(|(f, _)| f.as_str()).eq(["name", "age"])
    ));
    assert!(matches!(
        first_pattern("[a, b]"),
        Pattern::List { elements, rest: None } if elements.len() == 2
    ));
}

#[test]
fn defensive_fallthrough_arms() {
    // Drive lower_stmt / lower_type / lower_pattern on a node kind none of
    // their match arms expect (a `line_comment`), hitting their `_` fallbacks.
    let src = "// hi\nlet x = 1\n";
    let tree = parse_tree(src).unwrap();
    let lw = super::Lowerer::new(src.as_bytes());
    let comment = find_kind(tree.root_node(), "line_comment").unwrap();

    assert!(lw.lower_stmt(comment).is_none()); // `_ => return None`
    assert_eq!(lw.lower_type(comment).name, lw.text(comment)); // `_ => named(text)`
    assert!(matches!(lw.lower_pattern(comment), Pattern::Wildcard)); // `_` -> inner `_`
}
