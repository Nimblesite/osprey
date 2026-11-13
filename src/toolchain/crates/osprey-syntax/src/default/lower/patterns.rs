//! Match pattern lowering.

use super::{negate_literal, Lowerer};
use osprey_ast::{Expr, Pattern};
use tree_sitter::Node;

impl Lowerer<'_> {
    // ---- Patterns ----
    pub(crate) fn lower_pattern(&self, node: Node<'_>) -> Pattern {
        match node.kind() {
            "pattern" => {
                if let Some(inner) = self.first_named(node) {
                    return self.lower_pattern_inner(node, inner);
                }
                // bare `_` wildcard has no named child
                Pattern::Wildcard
            }
            _ => self.lower_pattern_inner(node, node),
        }
    }

    fn lower_pattern_inner(&self, pat: Node<'_>, inner: Node<'_>) -> Pattern {
        match inner.kind() {
            // A `-N` / `+N` pattern carries the sign in the `operator` field
            // (grammar: `seq(operator: choice('-','+'), integer|float)`); fold it
            // into the literal so `-5` matches `-5`, not `5`. Scalar literals now
            // appear unwrapped (no `literal` node) so `[…]` stays a list_pattern.
            "integer" | "float" | "boolean" | "string" | "interpolated_string" => {
                let negated = pat
                    .child_by_field_name("operator")
                    .is_some_and(|op| self.text(op) == "-");
                let minimum = negated
                    && inner.kind() == "integer"
                    && super::super::is_i64_min_magnitude_text(&self.text(inner));
                let lit = self.lower_literal_node(inner);
                let signed = if minimum {
                    Expr::Integer(i64::MIN)
                } else if negated {
                    negate_literal(lit)
                } else {
                    lit
                };
                Pattern::Literal(Box::new(signed))
            }
            "list_pattern" => self.lower_list_pattern(inner),
            "structural_pattern" => Pattern::Structural {
                fields: self
                    .first_child_of_kind(inner, "field_pattern")
                    .map_or_default(|fp| self.field_pattern_names(fp))
                    .into_iter()
                    .map(|f| (f.clone(), f))
                    .collect(),
                open: Self::has_row_rest(inner),
            },
            "tuple_pattern" => osprey_ast::tuple_pattern(self.tuple_binders(inner)),
            "identifier" | "qualified_path" => {
                // Could be: constructor `Ctor { fields }`, type-annotated, sub-patterns,
                // or a bare binding. Inspect siblings of the name field.
                let name = self.text(inner);
                if let Some(fp) = self.first_child_of_kind(pat, "field_pattern") {
                    return Pattern::Constructor {
                        name,
                        fields: self.field_pattern_names(fp),
                        sub_patterns: vec![],
                    };
                }
                if let Some(ty) = pat.child_by_field_name("type") {
                    return Pattern::TypeAnnotated {
                        name,
                        ty: self.lower_type(ty),
                    };
                }
                let subs = self.map_of_kind(pat, "pattern", Self::lower_pattern);
                // `Success(found)` binds the payload by ROLE, as ML's `Success found`
                // does, so it shares ML's lowering ([PATTERN-RESULT-BINDERS]).
                if let [Pattern::Binding(binder)] = subs.as_slice() {
                    if crate::desugar::is_result_ctor(&name) {
                        return crate::desugar::ctor_pattern(name, vec![binder.clone()]);
                    }
                }
                if !subs.is_empty() {
                    return Pattern::Constructor {
                        name,
                        fields: vec![],
                        sub_patterns: subs,
                    };
                }
                Pattern::Binding(name)
            }
            _ => Pattern::Wildcard,
        }
    }

    fn field_pattern_names(&self, fp: Node<'_>) -> Vec<String> {
        self.texts_of_kind(fp, "identifier")
    }

    /// Whether a structural pattern node carries the trailing `..` row-opener
    /// ([PATTERN-STRUCTURAL]). `..` is an anonymous token, invisible to the
    /// named-children traversal the other helpers use.
    fn has_row_rest(pat: Node<'_>) -> bool {
        (0..pat.child_count()).any(|i| pat.child(i).is_some_and(|c| c.kind() == ".."))
    }

    /// Each tuple slot's binder in written order: an identifier's text, or the
    /// empty binder for a `_` slot ([PATTERN-TUPLE]).
    fn tuple_binders(&self, tp: Node<'_>) -> Vec<String> {
        (0..tp.child_count())
            .filter_map(|i| tp.child(i))
            .filter_map(|c| match c.kind() {
                "identifier" => Some(self.text(c)),
                "_" => Some(String::new()),
                _ => None,
            })
            .collect()
    }

    /// Build a [`Pattern::List`] from a `list_pattern` node: the `element` fields
    /// (each a `pattern`) become the fixed-prefix patterns in source order, and
    /// the `rest` field (an identifier) becomes the optional tail binder.
    fn lower_list_pattern(&self, node: Node<'_>) -> Pattern {
        let elements = self.map_of_kind(node, "pattern", Self::lower_pattern);
        let rest = node.child_by_field_name("rest").map(|r| self.text(r));
        Pattern::List { elements, rest }
    }

    // ---- small node helpers ----
    pub(crate) fn field_text(&self, node: Node<'_>, field: &str) -> String {
        node.child_by_field_name(field)
            .map_or_default(|n| self.text(n))
    }

    #[expect(
        clippy::unused_self,
        reason = "kept for Lowerer method-call ergonomics"
    )]
    pub(crate) fn named_of_kind<'t>(&self, node: Node<'t>, kind: &str) -> Vec<Node<'t>> {
        let mut out = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == kind {
                out.push(child);
            }
        }
        out
    }

    /// Map every named child of `node` of the given `kind` through `f` — the
    /// shared "collect the children of a kind, lower each" step behind the
    /// per-kind accessors below and the variant/pattern lowering sites.
    pub(super) fn map_of_kind<T>(
        &self,
        node: Node<'_>,
        kind: &str,
        f: impl Fn(&Self, Node<'_>) -> T,
    ) -> Vec<T> {
        self.named_of_kind(node, kind)
            .iter()
            .map(|n| f(self, *n))
            .collect()
    }

    /// The source text of every named child of `node` of the given `kind`.
    pub(crate) fn texts_of_kind(&self, node: Node<'_>, kind: &str) -> Vec<String> {
        self.map_of_kind(node, kind, Self::text)
    }

    /// The lowered expression of every named child of `node` of the given `kind`.
    pub(crate) fn exprs_of_kind(&self, node: Node<'_>, kind: &str) -> Vec<Expr> {
        self.map_of_kind(node, kind, Self::lower_expr)
    }

    /// Recursive search for all descendants of a kind (for nested wrappers).
    #[expect(
        clippy::self_only_used_in_recursion,
        reason = "kept for Lowerer method-call ergonomics"
    )]
    pub(crate) fn descendants_of_kind<'t>(&self, node: Node<'t>, kind: &str) -> Vec<Node<'t>> {
        let mut out = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == kind {
                out.push(child);
            } else {
                out.extend(self.descendants_of_kind(child, kind));
            }
        }
        out
    }
}
