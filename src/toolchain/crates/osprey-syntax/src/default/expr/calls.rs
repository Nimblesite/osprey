//! Call, argument-list, arm and block lowering.

use super::Lowerer;
use osprey_ast::{Expr, FieldAssignment, MatchArm, NamedArgument, Pattern, Stmt};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn lower_call(&self, node: Node<'_>) -> Expr {
        let mut callee = self.lower_expr_field(node, "callee");
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
        // Keep an ordinary dotted call until inference can distinguish a
        // callable record field from receiver-first sugar [BUILTIN-STRING-UFCS].
        // Written type arguments apply a named function's declared binders.
        let (arguments, named_arguments) = self.lower_arg_list(node);
        if let Some(args) = node.child_by_field_name("type_arguments") {
            callee = Expr::TypeApply {
                function: Box::new(callee),
                type_args: self
                    .named_of_kind(args, "type_list")
                    .into_iter()
                    .flat_map(|list| self.lower_type_list(list))
                    .collect(),
                position: Some(self.pos(node)),
            };
        }
        match callee {
            Expr::FieldAccess { target, field } => Expr::MethodCall {
                target,
                method: field,
                arguments,
                named_arguments,
            },
            // A saturated call of a positionally-declared constructor is a
            // construction, not a call ([TYPE-UNION-POSITIONAL]) — the one
            // call-shaped expression exempt from the named-argument rule,
            // because a positional payload has no field names to supply.
            Expr::Identifier(name) if named_arguments.is_empty() => {
                crate::positional::construct(&name, arguments.clone()).unwrap_or(Expr::Call {
                    function: Box::new(Expr::Identifier(name)),
                    arguments,
                    named_arguments,
                })
            }
            other => Expr::Call {
                function: Box::new(other),
                arguments,
                named_arguments,
            },
        }
    }

    /// Collect positional or all-named args from an `argument_list` child
    /// ([CALL-ARGUMENTS]). Named
    /// arguments live in a single direct `named_argument_list` child; a *direct*
    /// lookup is essential — descending would steal the named arguments of a
    /// nested call (`print(cc(c1: .., c2: ..))` must not hoist c1/c2 onto print).
    pub(super) fn lower_arg_list(&self, node: Node<'_>) -> (Vec<Expr>, Vec<NamedArgument>) {
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

    /// A match or `select` arm's body, carrying the arm's own source position,
    /// so diagnostics point at the arm rather than at the shared body shape.
    pub(super) fn arm_body(&self, arm: Node<'_>) -> Expr {
        crate::desugar::positioned(self.lower_expr_field(arm, "body"), Some(self.pos(arm)))
    }

    pub(super) fn lower_arms(&self, node: Node<'_>) -> Vec<MatchArm> {
        self.named_of_kind(node, "match_arm")
            .iter()
            .map(|arm| MatchArm {
                pattern: arm
                    .child_by_field_name("pattern")
                    .map_or(Pattern::Wildcard, |p| self.lower_pattern(p)),
                body: self.arm_body(*arm),
            })
            .collect()
    }

    /// The effect a `perform` or `handle` site names, INCLUDING the
    /// instantiation when it wrote one: `Signal<Count>` and `Signal<Cursor>` are
    /// different effects to a row, so they are different effects to the name a
    /// row carries. Implements [STAGE-SIGNALS-EXACT].
    pub(in crate::default) fn mentioned_effect(&self, node: Node<'_>) -> String {
        let base = self.field_text(node, "effect");
        let arguments = node
            .child_by_field_name("instantiation")
            .map_or_else(Vec::new, |instantiation| self.type_list_of(instantiation));
        osprey_ast::effect_name::instantiated(&base, &arguments)
    }

    /// `head { field: value, … }`. The grammar gives every brace head the
    /// `type_constructor` shape (it wins the dynamic-precedence tie with
    /// `update_expression`), so the head's spelling decides what the author
    /// meant by the same rule as ML ([TYPE-RECORD-UPDATE]).
    pub(super) fn lower_brace_head(&self, node: Node<'_>) -> Expr {
        osprey_ast::record_expression(
            self.field_text(node, "name"),
            self.type_arguments_of(node),
            self.lower_field_assignments(node),
        )
    }

    pub(super) fn lower_field_assignments(&self, node: Node<'_>) -> Vec<FieldAssignment> {
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

    pub(super) fn lower_block(&self, node: Node<'_>) -> Expr {
        let mut cursor = node.walk();
        let children: Vec<Node<'_>> = node.named_children(&mut cursor).collect();
        self.lower_block_items(&children)
    }

    /// Lower the items of one block. Taken as a slice rather than read from the
    /// block node so a `handle` can be given the items after it as
    /// its body. Implements [EFFECTS-HANDLE-REST].
    ///
    /// Written braces stay a [`Expr::Block`] even around a lone value: the
    /// braces are the user's construct, and a doc above `{ … }` documents the
    /// block, not what is inside it ([TESTING-DOC]). A `handle` is the
    /// exception: it governs the rest of its block, so the block IS the
    /// handled region — the same node ML's layout form lowers to
    /// ([FLAVOR-IR-EQUIV]).
    fn lower_block_items(&self, children: &[Node<'_>]) -> Expr {
        let (statements, value, position) = self.block_items(children);
        match value.as_deref() {
            Some(Expr::Handler { .. }) => crate::desugar::block(statements, value, position),
            _ => Expr::Block {
                statements,
                value,
                position,
            },
        }
    }

    /// The statements and trailing value of one block's items.
    fn block_items(
        &self,
        children: &[Node<'_>],
    ) -> (Vec<Stmt>, Option<Box<Expr>>, Option<osprey_ast::Position>) {
        let mut statements = Vec::new();
        let mut value = None;
        let mut position = None;
        for (index, child) in children.iter().enumerate() {
            if let Some(handler) = self.handler_over_rest(*child) {
                let rest = children.get(index + 1..).unwrap_or_default();
                return (
                    statements,
                    Some(Box::new(self.handling_rest(handler, rest))),
                    Some(self.pos(*child)),
                );
            }
            match child.kind() {
                "statement" => {
                    if let Some(s) = self.first_named(*child).and_then(|n| self.lower_stmt(n)) {
                        statements.push(s);
                    }
                }
                "expression" => {
                    value = Some(Box::new(self.lower_expr(*child)));
                    position = Some(self.pos(*child));
                }
                _ => {}
            }
        }
        // A block evaluates to its last expression. The grammar sometimes emits
        // that trailing expression as an `expression_statement`; recover it as
        // the block value so the type of `{ ...; r }` is the type of `r`.
        if value.is_none() && matches!(statements.last(), Some(Stmt::Expr { .. })) {
            if let Some(Stmt::Expr {
                value: e,
                position: source,
                ..
            }) = statements.pop()
            {
                value = Some(Box::new(e));
                position = source;
            }
        }
        (statements, value, position)
    }

    /// A `handle E { … }` block statement governs the remaining block items.
    /// Implements [EFFECTS-HANDLE-REST].
    fn handler_over_rest<'t>(&self, item: Node<'t>) -> Option<Node<'t>> {
        if item.kind() != "statement" {
            return None;
        }
        let statement = self.first_named(item)?;
        if statement.kind() != "expression_statement" {
            return None;
        }
        let mut node = Self::last_named(statement)?;
        while matches!(node.kind(), "expression" | "primary_expression") {
            node = self.first_named(node)?;
        }
        (node.kind() == "handler_expression").then_some(node)
    }

    /// Install the handler over the remaining block items.
    fn handling_rest(&self, handler: Node<'_>, rest: &[Node<'_>]) -> Expr {
        Expr::Handler {
            stage: self.stage(handler),
            effect: self.mentioned_effect(handler),
            arms: self.lower_handler_arms(handler),
            return_clause: self.lower_handler_return(handler),
            finally: self.lower_handler_finally(handler),
            // The rest of the block is the region, not braces the user wrote:
            // a lone trailing value is the body itself, as in ML layout.
            body: Box::new({
                let (statements, value, position) = self.block_items(rest);
                crate::desugar::block(statements, value, position)
            }),
            position: Some(self.pos(handler)),
        }
    }
}
