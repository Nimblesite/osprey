//! Expression lowering: every [`Expr`] form — literals, operators, call shapes
//! and named arguments, match/handler arms, and string interpolation.

use super::lower::Lowerer;
use crate::strings::{lower_interpolation, unquote};
use osprey_ast::{
    Expr, FieldAssignment, HandlerArm, MapEntry, MatchArm, NamedArgument, Pattern, Stmt,
};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(crate) fn lower_expr_field(&self, node: Node<'_>, field: &str) -> Expr {
        match node.child_by_field_name(field) {
            Some(n) => self.lower_expr(n),
            None => Expr::Bool(false), // unreachable for well-formed trees
        }
    }

    /// Lower an expression node, transparently unwrapping the `expression` and
    /// `primary_expression` wrapper nodes tree-sitter inserts.
    pub(crate) fn lower_expr(&self, node: Node<'_>) -> Expr {
        match node.kind() {
            "expression" | "primary_expression" => match self.first_named(node) {
                Some(inner) => self.lower_expr(inner),
                None => Expr::Bool(false),
            },
            "binary_expression" => Expr::Binary {
                op: self.field_text(node, "operator"),
                left: Box::new(self.lower_expr_field(node, "left")),
                right: Box::new(self.lower_expr_field(node, "right")),
            },
            "unary_expression" => Expr::Unary {
                op: self.field_text(node, "operator"),
                operand: Box::new(self.lower_expr_field(node, "operand")),
            },
            // `x |> f` desugars to `f(x)` and `x |> f(a, …)` to `f(x, a, …)` —
            // the piped value becomes the callee's first positional argument, so
            // both the type checker and codegen see an ordinary call.
            "pipe_expression" => pipe_into(
                self.lower_expr_field(node, "left"),
                self.lower_expr_field(node, "right"),
            ),
            "call_expression" => self.lower_call(node),
            "match_expression" => Expr::Match {
                value: Box::new(self.lower_expr_field(node, "value")),
                arms: self.lower_arms(node),
            },
            // Populist Default-flavor `if cond { a } else { b }` desugars to the
            // same boolean `match` the ternary uses; `else if` nests into the
            // `false` arm. No new AST node ([FLAVOR-BOUNDARY]).
            "if_expression" => self.lower_if(node),
            "select_expression" => Expr::Select {
                arms: self.lower_arms(node),
            },
            "handler_expression" => Expr::Handler {
                effect: self.field_text(node, "effect"),
                arms: self.lower_handler_arms(node),
                body: Box::new(self.lower_expr_field(node, "body")),
                position: Some(self.pos(node)),
            },
            "perform_expression" => {
                let (arguments, named_arguments) = self.lower_arg_list(node);
                Expr::Perform {
                    effect: self.field_text(node, "effect"),
                    operation: self.field_text(node, "operation"),
                    arguments,
                    named_arguments,
                    position: Some(self.pos(node)),
                }
            }
            "resume_expression" => {
                Expr::Resume(self.first_named(node).map(|n| Box::new(self.lower_expr(n))))
            }
            "spawn_expression" => Expr::Spawn(Box::new(self.lower_inner_expr(node))),
            "yield_expression" => {
                Expr::Yield(self.first_named(node).map(|n| Box::new(self.lower_expr(n))))
            }
            "await_call" => Expr::Await(Box::new(self.lower_inner_expr(node))),
            "recv_call" => Expr::Recv(Box::new(self.lower_inner_expr(node))),
            "send_call" => {
                let mut cursor = node.walk();
                let mut exprs = node
                    .named_children(&mut cursor)
                    .filter(|c| c.kind() == "expression");
                let channel = exprs
                    .next()
                    .map_or(Expr::Bool(false), |n| self.lower_expr(n));
                let value = exprs
                    .next()
                    .map_or(Expr::Bool(false), |n| self.lower_expr(n));
                Expr::Send {
                    channel: Box::new(channel),
                    value: Box::new(value),
                }
            }
            "lambda_expression" => Expr::Lambda {
                parameters: self.lower_params(node.child_by_field_name("parameters").or_else(
                    || {
                        self.named_of_kind(node, "parameter_list")
                            .into_iter()
                            .next()
                    },
                )),
                // The only direct type-kind child of a lambda node is its
                // `-> ret` annotation (parameter types sit inside the
                // parameter_list, the body is an expression kind).
                return_type: self.last_type_child(node),
                body: Box::new(self.lower_expr_field(node, "body")),
                position: Some(self.pos(node)),
            },
            // Explicit construction-site type arguments (`Box<int> { ... }`)
            // are captured for the checker. Implements [TYPE-GENERICS-DECL].
            "type_constructor" => Expr::TypeConstructor {
                name: self.field_text(node, "name"),
                type_args: self
                    .first_child_of_kind(node, "type_arguments")
                    .and_then(|ta| self.first_child_of_kind(ta, "type_list"))
                    .map(|l| self.lower_type_list(l))
                    .unwrap_or_default(),
                fields: self.lower_field_assignments(node),
            },
            "update_expression" => Expr::Update {
                record: self.field_text(node, "record"),
                fields: self.lower_field_assignments(node),
            },
            "object_literal" => Expr::Object(self.lower_field_assignments(node)),
            "block" => self.lower_block(node),
            "literal" => self.lower_literal(node),
            "identifier" => Expr::Identifier(self.text(node)),
            "ternary_expression" => self.lower_ternary(node),
            _ => Expr::Bool(false),
        }
    }

    /// `if cond { a } else { b }` desugars to `match cond { true => a  false => b }`,
    /// reusing the boolean-match path ([GRAMMAR-IF-ELSE]). `else if` is a nested
    /// `if_expression` in the `alternative` field, so it recurses naturally into
    /// the `false` arm.
    fn lower_if(&self, node: Node<'_>) -> Expr {
        Expr::Match {
            value: Box::new(self.lower_expr_field(node, "condition")),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Bool(true))),
                    body: self.lower_expr_field(node, "consequence"),
                },
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Bool(false))),
                    body: self.lower_expr_field(node, "alternative"),
                },
            ],
        }
    }

    /// `cond ? then : else` desugars to `match cond { true => then  false => else }`
    /// (and the Elvis form `cond ?: else` reuses the condition as the `then`),
    /// so the existing boolean-match lowering carries the runtime semantics.
    fn lower_ternary(&self, node: Node<'_>) -> Expr {
        let condition = self.lower_expr_field(node, "condition");
        // Structural form `cond { f1, f2 } ? then : else`: bind each field from
        // `cond` and evaluate `then` — a record/object always carries its declared
        // fields, so the structural check succeeds. The then/else are the two
        // positional `expression` children after `condition`.
        if let Some(fp) = self.first_child_of_kind(node, "field_pattern") {
            let exprs = self.named_of_kind(node, "expression");
            let then_expr = exprs
                .get(1)
                .map_or(Expr::Bool(false), |n| self.lower_expr(*n));
            let statements = self
                .texts_of_kind(fp, "identifier")
                .into_iter()
                .map(|f| Stmt::Let {
                    name: f.clone(),
                    mutable: false,
                    ty: None,
                    value: Expr::FieldAccess {
                        target: Box::new(condition.clone()),
                        field: f,
                    },
                    doc: None,
                    position: None,
                })
                .collect();
            return Expr::Block {
                statements,
                value: Some(Box::new(then_expr)),
            };
        }
        let else_expr = self.lower_expr_field(node, "else");
        let then_expr = match node.child_by_field_name("then") {
            Some(n) => self.lower_expr(n),
            None => condition.clone(), // Elvis `?:`
        };
        Expr::Match {
            value: Box::new(condition),
            arms: vec![
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Bool(true))),
                    body: then_expr,
                },
                MatchArm {
                    pattern: Pattern::Literal(Box::new(Expr::Bool(false))),
                    body: else_expr,
                },
            ],
        }
    }

    fn lower_inner_expr(&self, node: Node<'_>) -> Expr {
        match self.first_named(node) {
            Some(n) => self.lower_expr(n),
            None => Expr::Bool(false),
        }
    }

    fn lower_call(&self, node: Node<'_>) -> Expr {
        let callee = self.lower_expr_field(node, "callee");
        if let Some(member) = node.child_by_field_name("member") {
            return Expr::FieldAccess {
                target: Box::new(callee),
                field: self.text(member),
            };
        }
        if let Some(index) = node.child_by_field_name("index") {
            return Expr::Index {
                target: Box::new(callee),
                index: Box::new(self.lower_expr(index)),
            };
        }
        // function/method call. UFCS: `x.f(a, …)` is sugar for `f(x, a, …)`, so a
        // field-access callee lowers to an ordinary call with the receiver as the
        // first positional argument — keeping method calls invisible downstream.
        let (mut arguments, named_arguments) = self.lower_arg_list(node);
        match callee {
            Expr::FieldAccess { target, field } => {
                arguments.insert(0, *target);
                Expr::Call {
                    function: Box::new(Expr::Identifier(field)),
                    arguments,
                    named_arguments,
                }
            }
            other => Expr::Call {
                function: Box::new(other),
                arguments,
                named_arguments,
            },
        }
    }

    /// Collect positional + named args from an `argument_list` child. Named
    /// arguments live in a single direct `named_argument_list` child; a *direct*
    /// lookup is essential — descending would steal the named arguments of a
    /// nested call (`print(cc(c1: .., c2: ..))` must not hoist c1/c2 onto print).
    fn lower_arg_list(&self, node: Node<'_>) -> (Vec<Expr>, Vec<NamedArgument>) {
        let Some(list) = self.named_of_kind(node, "argument_list").into_iter().next() else {
            return (Vec::new(), Vec::new());
        };
        if let Some(nal) = self.first_child_of_kind(list, "named_argument_list") {
            let nodes = self.named_of_kind(nal, "named_argument");
            let named = self.lower_name_value(&nodes, |name, value| NamedArgument { name, value });
            return (Vec::new(), named);
        }
        let mut cursor = list.walk();
        let positional = list
            .named_children(&mut cursor)
            .filter(|c| c.kind() == "expression")
            .map(|c| self.lower_expr(c))
            .collect();
        (positional, Vec::new())
    }

    fn lower_arms(&self, node: Node<'_>) -> Vec<MatchArm> {
        self.named_of_kind(node, "match_arm")
            .iter()
            .chain(self.named_of_kind(node, "select_arm").iter())
            .map(|arm| MatchArm {
                pattern: arm
                    .child_by_field_name("pattern")
                    .map_or(Pattern::Wildcard, |p| self.lower_pattern(p)),
                body: self.lower_expr_field(*arm, "body"),
            })
            .collect()
    }

    fn lower_handler_arms(&self, node: Node<'_>) -> Vec<HandlerArm> {
        self.named_of_kind(node, "handler_arm")
            .iter()
            .map(|arm| HandlerArm {
                operation: self.field_text(*arm, "operation"),
                params: self
                    .first_child_of_kind(*arm, "handler_params")
                    .map(|hp| self.texts_of_kind(hp, "identifier"))
                    .unwrap_or_default(),
                body: self.lower_expr_field(*arm, "body"),
            })
            .collect()
    }

    fn lower_field_assignments(&self, node: Node<'_>) -> Vec<FieldAssignment> {
        let nodes = self.descendants_of_kind(node, "field_assignment");
        self.lower_name_value(&nodes, |name, value| FieldAssignment { name, value })
    }

    /// Map each node carrying a `name` field and a `value` expression into an AST
    /// node built by `ctor` — the shared shape of named arguments and record
    /// field assignments.
    fn lower_name_value<T>(&self, nodes: &[Node<'_>], ctor: impl Fn(String, Expr) -> T) -> Vec<T> {
        nodes
            .iter()
            .map(|n| {
                ctor(
                    self.field_text(*n, "name"),
                    self.lower_expr_field(*n, "value"),
                )
            })
            .collect()
    }

    fn lower_block(&self, node: Node<'_>) -> Expr {
        let mut statements = Vec::new();
        let mut value = None;
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "statement" => {
                    if let Some(s) = self.first_named(child).and_then(|n| self.lower_stmt(n)) {
                        statements.push(s);
                    }
                }
                "expression" => value = Some(Box::new(self.lower_expr(child))),
                _ => {}
            }
        }
        // A block evaluates to its last expression. The grammar sometimes emits
        // that trailing expression as an `expression_statement`; recover it as
        // the block value so the type of `{ ...; r }` is the type of `r`.
        if value.is_none() && matches!(statements.last(), Some(Stmt::Expr { .. })) {
            if let Some(Stmt::Expr { value: e, .. }) = statements.pop() {
                value = Some(Box::new(e));
            }
        }
        Expr::Block { statements, value }
    }

    pub(crate) fn lower_literal(&self, node: Node<'_>) -> Expr {
        let Some(inner) = self.first_named(node) else {
            return Expr::Bool(false);
        };
        self.lower_literal_node(inner)
    }

    /// Lower an already-unwrapped literal node (`integer`/`string`/… directly,
    /// not the `literal` wrapper) — shared by `lower_literal` and scalar pattern
    /// lowering, where the grammar exposes the scalar node without a wrapper.
    pub(crate) fn lower_literal_node(&self, inner: Node<'_>) -> Expr {
        match inner.kind() {
            "integer" => Expr::Integer(self.text(inner).parse().unwrap_or(0)),
            "float" => Expr::Float(self.text(inner).parse().unwrap_or(0.0)),
            "boolean" => Expr::Bool(self.text(inner) == "true"),
            "string" => {
                // Equal-length `string`/`interpolated_string` token matches let the
                // plain `string` rule win, so a `"...${e}..."` can arrive tagged as
                // `string`. Detect the `${` marker here and interpolate either way.
                let raw = self.text(inner);
                if raw.contains("${") {
                    Expr::InterpolatedStr(lower_interpolation(&raw, parse_fragment))
                } else {
                    Expr::Str(unquote(&raw))
                }
            }
            "interpolated_string" => {
                Expr::InterpolatedStr(lower_interpolation(&self.text(inner), parse_fragment))
            }
            "list_literal" => Expr::List(self.exprs_of_kind(inner, "expression")),
            "map_literal" => Expr::Map(
                self.named_of_kind(inner, "map_entry")
                    .iter()
                    .map(|me| MapEntry {
                        key: self.lower_expr_field(*me, "key"),
                        value: self.lower_expr_field(*me, "value"),
                    })
                    .collect(),
            ),
            _ => Expr::Bool(false),
        }
    }

    // Interpolation splitting and escape resolution are flavor-neutral and live
    // in `crate::strings`; this frontend supplies its own fragment parser below.
}

/// Parse an interpolation fragment (`${ ... }` contents) into a single [`Expr`].
fn parse_fragment(frag: &str) -> Expr {
    let parsed = crate::parse_program(&format!("let __frag__ = {frag}\n"));
    match parsed.program.statements.into_iter().next() {
        Some(Stmt::Let { value, .. }) => value,
        _ => Expr::Identifier(frag.trim().to_string()),
    }
}

/// Fold a piped value into its right-hand callee: `x |> f(a, …)` becomes
/// `f(x, a, …)` (the piped value is prepended as the first positional
/// argument). A bare callee `x |> f` becomes `f(x)`. Producing a plain
/// [`Expr::Call`] keeps pipes invisible to every later stage.
fn pipe_into(left: Expr, right: Expr) -> Expr {
    match right {
        Expr::Call {
            function,
            mut arguments,
            named_arguments,
        } => {
            arguments.insert(0, left);
            Expr::Call {
                function,
                arguments,
                named_arguments,
            }
        }
        callee => Expr::Call {
            function: Box::new(callee),
            arguments: vec![left],
            named_arguments: Vec::new(),
        },
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
mod tests {
    use crate::parse_program;
    use crate::parse_tree;
    use osprey_ast::{Expr, InterpolatedPart, Stmt};
    use tree_sitter::Node;

    fn let_value(src: &str) -> Expr {
        let parsed = parse_program(src);
        assert!(parsed.errors.is_empty(), "errors: {:?}", parsed.errors);
        match parsed.program.statements.into_iter().next() {
            Some(Stmt::Let { value, .. }) => value,
            other => panic!("expected let, got {other:?}"),
        }
    }

    fn find_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        let kids: Vec<Node<'t>> = node.children(&mut cursor).collect();
        kids.into_iter().find_map(|c| find_kind(c, kind))
    }

    #[test]
    fn unquote_resolves_all_escapes() {
        // Every recognised escape (\n \r \t \e \0 \" \\), plus an unknown escape
        // (\q) that is kept verbatim.
        match let_value("let s = \"\\n\\r\\t\\e\\0\\\"\\\\\\q\"\n") {
            Expr::Str(s) => assert_eq!(s, "\n\r\t\u{1b}\0\"\\\\q"),
            other => panic!("expected string, got {other:?}"),
        }
    }

    #[test]
    fn lowers_unary_and_block_recovery() {
        // unary_expression arm.
        assert!(matches!(let_value("let r = -x\n"), Expr::Unary { .. }));
        // A function-body block whose trailing value is emitted as an
        // expression_statement: lower_block recovers it as the block value.
        let parsed = parse_program("fn f() = {\n  print(1)\n  42\n}\n");
        assert!(parsed.errors.is_empty(), "errors: {:?}", parsed.errors);
        match parsed.program.statements.into_iter().next() {
            Some(Stmt::Function {
                body: Expr::Block { statements, value },
                ..
            }) => {
                assert_eq!(statements.len(), 1);
                assert!(matches!(value.as_deref(), Some(Expr::Integer(42))));
            }
            other => panic!("expected function with block body, got {other:?}"),
        }
    }

    #[test]
    fn interpolation_keeps_trailing_text_and_inner_expr() {
        // `${1 + 2}` followed by trailing text exercises the tail-text push.
        match let_value("let s = \"v ${1 + 2} end\"\n") {
            Expr::InterpolatedStr(parts) => {
                assert!(matches!(parts[0], InterpolatedPart::Text(ref t) if t == "v "));
                assert!(matches!(
                    parts[1],
                    InterpolatedPart::Expr(Expr::Binary { .. })
                ));
                assert!(matches!(parts[2], InterpolatedPart::Text(ref t) if t == " end"));
            }
            other => panic!("expected interpolated, got {other:?}"),
        }
        // An interpolation that ends exactly at `}` leaves no trailing text, so
        // the final empty-text branch is taken (no extra Text part is pushed).
        match let_value("let s = \"${1}\"\n") {
            Expr::InterpolatedStr(parts) => {
                assert_eq!(parts.len(), 1);
                assert!(matches!(parts[0], InterpolatedPart::Expr(_)));
            }
            other => panic!("expected interpolated, got {other:?}"),
        }
    }

    #[test]
    fn spawn_and_yield_lower() {
        assert!(matches!(let_value("let f = spawn g()\n"), Expr::Spawn(_)));
        assert!(matches!(let_value("let y = yield\n"), Expr::Yield(None)));
    }

    #[test]
    fn lowers_common_expression_forms() {
        // Pipe desugars `x |> f(2)` to a call with x prepended.
        match let_value("let r = x |> f(2)\n") {
            Expr::Call { arguments, .. } => assert_eq!(arguments.len(), 2),
            other => panic!("expected call, got {other:?}"),
        }
        // UFCS field-access call `o.m(1)` -> Call(m, [o, 1]).
        match let_value("let r = o.m(1)\n") {
            Expr::Call {
                function,
                arguments,
                ..
            } => {
                assert!(matches!(*function, Expr::Identifier(ref n) if n == "m"));
                assert_eq!(arguments.len(), 2);
            }
            other => panic!("expected call, got {other:?}"),
        }
        // Plain field access and indexing.
        assert!(matches!(
            let_value("let r = o.field\n"),
            Expr::FieldAccess { .. }
        ));
        assert!(matches!(let_value("let r = xs[0]\n"), Expr::Index { .. }));
        // Named-argument call.
        match let_value("let r = mk(a: 1, b: 2)\n") {
            Expr::Call {
                named_arguments, ..
            } => assert_eq!(named_arguments.len(), 2),
            other => panic!("expected call, got {other:?}"),
        }
        // Ternary and Elvis desugar to a boolean match.
        assert!(matches!(
            let_value("let r = c ? 1 : 2\n"),
            Expr::Match { .. }
        ));
        assert!(matches!(let_value("let r = c ?: 2\n"), Expr::Match { .. }));
        // Object literal and lambda.
        assert!(matches!(
            let_value("let r = { a: 1, b: 2 }\n"),
            Expr::Object(_)
        ));
        assert!(matches!(
            let_value("let r = |x| => x\n"),
            Expr::Lambda { .. }
        ));
    }

    #[test]
    fn lowers_match_select_handler_perform() {
        // match with a structural-field pattern body.
        assert!(matches!(
            let_value("let r = match x { Ok { value } => value  _ => 0 }\n"),
            Expr::Match { .. }
        ));
        // select expression (reuses arm lowering).
        assert!(matches!(
            let_value("let r = select { a => 1  b => 2 }\n"),
            Expr::Select { .. }
        ));
        // handler with params + perform inside its body.
        match let_value("let r = handle Log\n  info m => m\nin perform Log.info(x: 1)\n") {
            Expr::Handler { effect, arms, .. } => {
                assert_eq!(effect, "Log");
                assert_eq!(arms[0].operation, "info");
                assert_eq!(arms[0].params, vec!["m"]);
            }
            other => panic!("expected handler, got {other:?}"),
        }
    }

    #[test]
    fn parse_fragment_falls_back_to_identifier() {
        // A fragment that does not parse to a `let` value returns the trimmed text.
        assert_eq!(
            super::parse_fragment(" fn "),
            Expr::Identifier("fn".to_string())
        );
    }

    #[test]
    fn lowers_calls_literals_block_and_constructor() {
        // send/recv/await call forms.
        assert!(matches!(
            let_value("let r = send(ch, 1)\n"),
            Expr::Send { .. }
        ));
        assert!(matches!(let_value("let r = recv(ch)\n"), Expr::Recv(_)));
        assert!(matches!(let_value("let r = await(f)\n"), Expr::Await(_)));
        // type constructor carrying field assignments.
        match let_value("let r = Point { x: 1, y: 2 }\n") {
            Expr::TypeConstructor { name, fields, .. } => {
                assert_eq!(name, "Point");
                assert_eq!(fields.len(), 2);
            }
            other => panic!("expected constructor, got {other:?}"),
        }
        // list + map literals.
        assert!(matches!(let_value("let r = [1, 2, 3]\n"), Expr::List(v) if v.len() == 3));
        assert!(
            matches!(let_value("let r = { \"a\": 1, \"b\": 2 }\n"), Expr::Map(m) if m.len() == 2)
        );
        // block with a statement plus a trailing value expression.
        match let_value("let r = {\n  let a = 1\n  a\n}\n") {
            Expr::Block { statements, value } => {
                assert_eq!(statements.len(), 1);
                assert!(value.is_some());
            }
            other => panic!("expected block, got {other:?}"),
        }
        // structural ternary `cond { f } ? then : else` -> block binding fields.
        assert!(matches!(
            let_value("let r = rec { a } ? a : 0\n"),
            Expr::Block { .. }
        ));
    }

    #[test]
    fn defensive_expr_fallbacks() {
        // Direct calls on a `line_comment` (or a missing field) hit the
        // best-effort `Expr::Bool(false)` arms that well-formed trees never reach.
        let src = "// c\nlet x = 1\n";
        let tree = parse_tree(src).unwrap();
        let lw = super::Lowerer::new(src.as_bytes());
        let comment = find_kind(tree.root_node(), "line_comment").unwrap();
        let let_node = find_kind(tree.root_node(), "let_declaration").unwrap();

        assert_eq!(lw.lower_expr(comment), Expr::Bool(false)); // `_` arm
        assert_eq!(lw.lower_literal(comment), Expr::Bool(false)); // no named child
        assert_eq!(lw.lower_literal_node(comment), Expr::Bool(false)); // `_` arm
        assert_eq!(lw.lower_inner_expr(comment), Expr::Bool(false)); // no named child
        assert_eq!(lw.lower_expr_field(let_node, "nope"), Expr::Bool(false)); // missing field
    }
}
