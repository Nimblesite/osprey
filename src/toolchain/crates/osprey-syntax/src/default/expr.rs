//! Expression lowering: every [`Expr`] form — literals, operators, call shapes
//! and named arguments, match/handler arms, and string interpolation.

use super::lower::Lowerer;
use crate::strings::{lower_interpolation, unquote};
use osprey_ast::SelectOperation;
use osprey_ast::{Expr, MapEntry, MatchArm, Pattern, Stmt};
use tree_sitter::Node;

mod calls;
#[cfg(test)]
mod tests;

impl Lowerer<'_> {
    pub(crate) fn lower_expr_field(&self, node: Node<'_>, field: &str) -> Expr {
        match node.child_by_field_name(field) {
            Some(n) => self.lower_expr(n),
            None => Expr::Bool(false), // unreachable for well-formed trees
        }
    }

    /// Lower an expression node, transparently unwrapping the `expression` and
    /// `primary_expression` wrapper nodes tree-sitter inserts.
    pub(crate) fn lower_expr(&self, mut node: Node<'_>) -> Expr {
        // Parenthesised expressions can contain hundreds of alternating
        // `expression`/`primary_expression` CST wrappers. They carry no AST
        // meaning, so peel them iteratively instead of spending one Rust stack
        // frame per wrapper before reaching the actual expression node.
        while matches!(node.kind(), "expression" | "primary_expression") {
            let Some(inner) = self.first_named(node) else {
                return Expr::Bool(false);
            };
            node = inner;
        }
        match node.kind() {
            "binary_expression" => Expr::Binary {
                position: node
                    .child_by_field_name("operator")
                    .map(|op| super::position_from_point(op.start_position())),
                op: self.field_text(node, "operator"),
                left: Box::new(self.lower_expr_field(node, "left")),
                right: Box::new(self.lower_expr_field(node, "right")),
            },
            "unary_expression" => self.lower_unary(node),
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
            "select_expression" => self.lower_select(node),
            // `handler E { arm… }` — the handler with no region attached, as a
            // value. Implements [EFFECTS-HANDLER-VALUE].
            "handler_value_expression" => osprey_ast::handler_value(
                self.stage(node),
                self.mentioned_effect(node),
                self.lower_handler_arms(node),
                self.lower_handler_clauses(node),
                Some(self.pos(node)),
            ),
            "kernel_expression" => self.lower_kernel(node),
            "perform_expression" => {
                let (arguments, named_arguments) = self.lower_arg_list(node);
                Expr::Perform {
                    effect: self.mentioned_effect(node),
                    operation: self.field_text(node, "operation"),
                    arguments,
                    named_arguments,
                    position: Some(self.pos(node)),
                }
            }
            // Implements [EFFECTS-MASK] and [EFFECTS-INSTANCE].
            "mask_expression" => self.lower_mask(node),
            "instance_expression" => Expr::Instance {
                effect: self.mentioned_effect(node),
                position: Some(self.pos(node)),
            },
            "resume_expression" => {
                Expr::Resume(self.first_named(node).map(|n| Box::new(self.lower_expr(n))))
            }
            "spawn_expression" => Expr::Spawn(Box::new(self.lower_inner_expr(node))),
            "yield_expression" => {
                Expr::Yield(self.first_named(node).map(|n| Box::new(self.lower_expr(n))))
            }
            "await_call" => Expr::Await(Box::new(self.lower_inner_expr(node))),
            "recv_call" => Expr::Recv(Box::new(self.lower_inner_expr(node))),
            "send_call" => self.lower_send(node),
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
            "type_constructor" => self.lower_brace_head(node),
            "update_expression" => Expr::Update {
                record: self.field_text(node, "record"),
                fields: self.lower_field_assignments(node),
            },
            "object_literal" => Expr::Object(self.lower_field_assignments(node)),
            "block" => self.lower_block(node),
            "literal" => self.lower_literal(node),
            "qualified_path" => Expr::Path(self.lower_symbol_path(node)),
            "identifier" => Expr::Identifier(self.text(node)),
            "ternary_expression" => self.lower_ternary(node),
            _ => Expr::Bool(false),
        }
    }

    fn lower_send(&self, node: Node<'_>) -> Expr {
        let mut cursor = node.walk();
        let mut expressions = node
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "expression")
            .map(|expression| self.lower_expr(expression));
        Expr::Send {
            channel: Box::new(expressions.next().unwrap_or(Expr::Bool(false))),
            value: Box::new(expressions.next().unwrap_or(Expr::Bool(false))),
        }
    }

    /// `select { value = recv(c) => … send(c, v) => … timeout(ms) => … _ => … }`.
    fn lower_select(&self, node: Node<'_>) -> Expr {
        let arms = self.named_of_kind(node, "select_arm");
        Expr::Select {
            operations: arms
                .iter()
                .map(|arm| self.lower_select_operation(*arm))
                .collect(),
            arms: arms
                .iter()
                .map(|arm| MatchArm {
                    pattern: arm
                        .child_by_field_name("binding")
                        .map(|binding| self.text(binding))
                        .filter(|name| name != "_")
                        .map_or(Pattern::Wildcard, Pattern::Binding),
                    body: self.arm_body(*arm),
                })
                .collect(),
        }
    }

    fn lower_select_operation(&self, arm: Node<'_>) -> SelectOperation {
        let Some(operation) = arm.child_by_field_name("operation") else {
            return SelectOperation::Default;
        };
        match (operation.kind(), self.lower_send(operation)) {
            ("send_call", Expr::Send { channel, value }) => {
                SelectOperation::Send { channel, value }
            }
            ("recv_call", _) => SelectOperation::Recv(Box::new(self.lower_inner_expr(operation))),
            _ => SelectOperation::Timeout(Box::new(self.lower_inner_expr(operation))),
        }
    }

    /// `if cond { a } else { b }` desugars to `match cond { true => a  false => b }`,
    /// reusing the boolean-match path ([GRAMMAR-IF-ELSE]). `else if` is a nested
    /// `if_expression` in the `alternative` field, so it recurses naturally into
    /// the `false` arm.
    fn lower_if(&self, node: Node<'_>) -> Expr {
        crate::desugar::bool_match(
            self.lower_expr_field(node, "condition"),
            self.lower_expr_field(node, "consequence"),
            self.lower_expr_field(node, "alternative"),
        )
    }

    /// `cond ? then : else` desugars to a boolean match. The Elvis spelling
    /// `result ?: fallback` instead becomes an explicit Success/Error match.
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
                position: None,
                statements,
                value: Some(Box::new(then_expr)),
            };
        }
        let else_expr = self.lower_expr_field(node, "else");
        match node.child_by_field_name("then") {
            Some(n) => crate::desugar::bool_match(condition, self.lower_expr(n), else_expr),
            None => crate::desugar::result_default(condition, else_expr),
        }
    }

    fn lower_inner_expr(&self, node: Node<'_>) -> Expr {
        match self.first_named(node) {
            Some(n) => self.lower_expr(n),
            None => Expr::Bool(false),
        }
    }

    fn lower_literal(&self, node: Node<'_>) -> Expr {
        let Some(inner) = self.first_named(node) else {
            return Expr::Bool(false);
        };
        self.lower_literal_node(inner)
    }

    fn lower_unary(&self, node: Node<'_>) -> Expr {
        let op = self.field_text(node, "operator");
        let operand = node.child_by_field_name("operand");
        // The magnitude of i64::MIN is one larger than i64::MAX. Accept that
        // one spelling only under unary minus and lower it directly; every
        // larger magnitude is reported by the frontend's numeric validator.
        if op == "-" && operand.is_some_and(|n| super::is_i64_min_magnitude_text(&self.text(n))) {
            return Expr::Integer(i64::MIN);
        }
        let lowered = self.lower_expr_field(node, "operand");
        // A negated literal folds to the literal itself so `-1` stays an `int`
        // rather than becoming a fallible `Result` ([ARITH-NEG-LITERAL]).
        if op == "-" {
            return Expr::negated(lowered);
        }
        Expr::Unary {
            op,
            operand: Box::new(lowered),
        }
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
                let raw = self.text(inner);
                if raw.contains("${") {
                    Expr::InterpolatedStr(lower_interpolation(
                        &raw,
                        Some(self.pos(inner)),
                        crate::Flavor::Default,
                        parse_fragment,
                    ))
                } else {
                    Expr::Str(unquote(&raw))
                }
            }
            "interpolated_string" => Expr::InterpolatedStr(lower_interpolation(
                &self.text(inner),
                Some(self.pos(inner)),
                crate::Flavor::Default,
                parse_fragment,
            )),
            "list_literal" => Expr::List(
                self.exprs_of_kind(inner, "expression"),
                Some(self.pos(inner)),
            ),
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
    let binding = crate::strings::fragment_binding(crate::Flavor::Default);
    let parsed = super::parse(&format!("{binding}{frag}\n"));
    if !parsed.errors.is_empty() {
        return Expr::Identifier(frag.trim().to_owned());
    }
    match parsed.program.statements.into_iter().next() {
        Some(Stmt::Let { value, .. }) => value,
        _ => Expr::Identifier(frag.trim().to_string()),
    }
}

/// Fold a piped value into its right-hand callee: `x |> f(a, …)` becomes
/// `f(x, a, …)` (the piped value is prepended as the first positional
/// argument). A bare callee `x |> f` becomes `f(x)`. Producing a plain
/// [`Expr::Call`] keeps pipes invisible to every later stage.
/// Implements [BUILTIN-ITER-PIPE].
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
