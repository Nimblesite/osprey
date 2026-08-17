//! Statement, type, and pattern lowering: declarations (`fn`, `let`, `type`,
//! `effect`, `extern`, `module`), type expressions, and match patterns.

use super::position_from_point;
use osprey_ast::{
    DocComment, DocScope, EffectOperation, EffectRef, Expr, ExternParameter, ModuleKind, Parameter,
    Pattern, Position, Program, Stage, Stmt, SymbolPath, TypeExpr, TypeField, TypeParam,
    TypeVariant, Variance,
};
use tree_sitter::Node;

/// The surface spelling of an ignored parameter ([PARAM-WILDCARD]).
const WILDCARD_PARAM: &str = "_";

/// Strip one doc-comment line's leading whitespace, its `///`/`//!` marker, and
/// one optional following space — leaving the prose. Implements
/// [DOC-SIGIL-DEFAULT].
fn strip_doc_line(line: &str) -> &str {
    let t = line.trim_start();
    let rest = t
        .strip_prefix("///")
        .or_else(|| t.strip_prefix("//!"))
        .unwrap_or(line);
    rest.strip_prefix(' ').unwrap_or(rest)
}

/// Holds the source bytes so node text can be sliced during lowering.
#[derive(Debug)]
pub struct Lowerer<'a> {
    src: &'a [u8],
}

impl<'a> Lowerer<'a> {
    /// Creates a lowerer over the given source bytes.
    #[must_use]
    pub(crate) fn new(src: &'a [u8]) -> Self {
        Lowerer { src }
    }

    pub(crate) fn text(&self, node: Node<'_>) -> String {
        node.utf8_text(self.src).unwrap_or("").to_string()
    }

    #[expect(
        clippy::unused_self,
        reason = "kept for Lowerer method-call ergonomics"
    )]
    pub(crate) fn pos(&self, node: Node<'_>) -> Position {
        position_from_point(node.start_position())
    }

    /// The stage of an effect declaration or handler region: [`Stage::Static`]
    /// when the `static` marker is present, dynamic otherwise — so every effect
    /// written before staging keeps its meaning. Implements [STAGE-DECL],
    /// [STAGE-HANDLE-STATIC], [STAGE-COMPAT].
    #[expect(
        clippy::unused_self,
        reason = "kept for Lowerer method-call ergonomics"
    )]
    pub(crate) fn stage(&self, node: Node<'_>) -> Stage {
        match node.child_by_field_name("stage") {
            Some(_) => Stage::Static,
            None => Stage::Dynamic,
        }
    }

    /// Position of `node`'s named `field`, or `node`'s own start when absent. A
    /// leading `///` doc comment shifts `node.start` onto the comment, so a
    /// declaration keeps a stable position by anchoring on its keyword/name.
    fn field_pos(&self, node: Node<'_>, field: &str) -> Position {
        self.pos(node.child_by_field_name(field).unwrap_or(node))
    }

    /// The leading `///`/`//!` documentation of a declaration, stripped of its
    /// markers and lowered into a structured [`DocComment`] by the shared
    /// flavor-neutral parser; `None` when the declaration carries no doc
    /// comment. Implements [DOC-SIGIL-DEFAULT], [DOC-MODEL].
    pub(crate) fn doc_text(&self, node: Node<'_>) -> Option<DocComment> {
        let doc = self.first_child_of_kind(node, "doc_comment")?;
        let text = self.text(doc);
        let scope = if text.trim_start().starts_with("//!") {
            DocScope::Inner
        } else {
            DocScope::Outer
        };
        let body: Vec<&str> = text.lines().map(strip_doc_line).collect();
        Some(crate::docparse::parse_doc(&body.join("\n"), scope))
    }

    /// First *named* child (skips anonymous tokens). Used to unwrap the wrapper
    /// nodes tree-sitter inserts (`statement`, `expression`, `primary_expression`).
    #[expect(
        clippy::unused_self,
        reason = "kept for Lowerer method-call ergonomics"
    )]
    pub(crate) fn first_named<'t>(&self, node: Node<'t>) -> Option<Node<'t>> {
        let mut cursor = node.walk();
        let found = node.named_children(&mut cursor).next();
        found
    }

    /// First named child of a given kind.
    #[expect(
        clippy::unused_self,
        reason = "kept for Lowerer method-call ergonomics"
    )]
    pub(crate) fn first_child_of_kind<'t>(&self, node: Node<'t>, kind: &str) -> Option<Node<'t>> {
        let mut cursor = node.walk();
        let found = node.named_children(&mut cursor).find(|c| c.kind() == kind);
        found
    }

    /// Lowers the root `source_file` node into a full program AST.
    #[must_use]
    pub(crate) fn lower_program(&self, root: Node<'_>) -> Program {
        let _positional = crate::positional::install(
            self.descendants_of_kind(root, "variant")
                .into_iter()
                .filter_map(|v| self.positional_ctor(v)),
        );
        let mut statements = Vec::new();
        let mut cursor = root.walk();
        let source_statements: Vec<Node<'_>> = root
            .named_children(&mut cursor)
            .filter(|child| child.kind() == "statement")
            .collect();
        let mut index = 0;
        while let Some(wrapper) = source_statements.get(index).copied() {
            let Some(node) = self.first_named(wrapper) else {
                index += 1;
                continue;
            };
            // A file-scoped namespace owns every following declaration in the
            // file. Canonicalise that relationship here rather than making
            // later phases reinterpret source order. [MODULES-FILE-SCOPED-NAMESPACE]
            if node.kind() == "namespace_declaration" && node.child_by_field_name("body").is_none()
            {
                let body = source_statements
                    .get(index.saturating_add(1)..)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|s| self.first_named(*s))
                    .filter_map(|s| self.lower_stmt(s))
                    .collect();
                statements.push(Stmt::Namespace {
                    name: self.lower_namespace_name(node.child_by_field_name("name")),
                    body,
                    file_scoped: true,
                    position: Some(self.field_pos(node, "keyword")),
                });
                break;
            }
            if let Some(stmt) = self.lower_stmt(node) {
                statements.push(stmt);
            }
            index += 1;
        }
        Program { statements }
    }

    pub(crate) fn lower_stmt(&self, node: Node<'_>) -> Option<Stmt> {
        Some(match node.kind() {
            "import_statement" => Stmt::Import(self.lower_import(node)),
            "namespace_declaration" => Stmt::Namespace {
                name: self.lower_namespace_name(node.child_by_field_name("name")),
                body: node
                    .child_by_field_name("body")
                    .map(|body| self.lower_statement_children(body))
                    .unwrap_or_default(),
                file_scoped: node.child_by_field_name("body").is_none(),
                position: Some(self.field_pos(node, "keyword")),
            },
            "let_declaration" => Stmt::Let {
                name: self.field_text(node, "name"),
                mutable: node
                    .child_by_field_name("keyword")
                    .is_some_and(|n| self.text(n) == "mut"),
                ty: node.child_by_field_name("type").map(|n| self.lower_type(n)),
                value: self.lower_expr_field(node, "value"),
                doc: self.doc_text(node),
                position: Some(self.field_pos(node, "keyword")),
            },
            "assignment" => Stmt::Assignment {
                name: self.field_text(node, "name"),
                value: self.lower_expr_field(node, "value"),
                position: Some(self.pos(node)),
            },
            "function_declaration" => Stmt::Function {
                name: self.field_text(node, "name"),
                type_params: self.lower_type_params(node),
                parameters: self.lower_params(node.child_by_field_name("parameters")),
                return_type: node
                    .child_by_field_name("return_type")
                    .map(|n| self.lower_type(n)),
                effects: self.lower_effects(node.child_by_field_name("effects")),
                body: self.lower_expr_field(node, "body"),
                doc: self.doc_text(node),
                position: Some(self.field_pos(node, "name")),
            },
            "extern_declaration" => Stmt::Extern {
                name: self.field_text(node, "name"),
                parameters: self.lower_extern_params(node.child_by_field_name("parameters")),
                return_type: node
                    .child_by_field_name("return_type")
                    .map(|n| self.lower_type(n)),
                doc: self.doc_text(node),
                position: Some(self.pos(node)),
            },
            "type_declaration" => self.lower_type_decl(node),
            "effect_declaration" => Stmt::Effect {
                stage: self.stage(node),
                name: self.field_text(node, "name"),
                type_params: self.lower_type_params(node),
                operations: self.lower_operations(node),
                doc: self.doc_text(node),
                position: Some(self.pos(node)),
            },
            "module_declaration" => Stmt::Module {
                path: node
                    .child_by_field_name("path")
                    .map_or_else(SymbolPath::default, |p| self.lower_symbol_path(p)),
                kind: if node.child_by_field_name("state").is_some() {
                    ModuleKind::State
                } else {
                    ModuleKind::Plain
                },
                signature: node
                    .child_by_field_name("signature")
                    .map(|s| self.lower_signature_ascription(s)),
                body: self
                    .named_of_kind(node, "module_item")
                    .iter()
                    .filter_map(|item| self.lower_module_item(*item))
                    .collect(),
                doc: self.doc_text(node),
                position: Some(self.field_pos(node, "keyword")),
            },
            "signature_declaration" => Stmt::Signature {
                name: self.field_text(node, "name"),
                items: self
                    .named_of_kind(node, "signature_item")
                    .iter()
                    .filter_map(|item| self.lower_signature_item(*item))
                    .collect(),
                doc: self.doc_text(node),
                position: Some(self.field_pos(node, "keyword")),
            },
            // A leading `///` block documents the expression that follows, so
            // the expression is looked up by kind rather than by position —
            // `first_named` would hand back the doc comment ([TESTING-DOC]).
            "expression_statement" => {
                let expr = self.first_child_of_kind(node, "expression")?;
                Stmt::Expr {
                    value: self.lower_expr(expr),
                    doc: self.doc_text(node),
                    position: Some(self.pos(expr)),
                }
            }
            _ => return None,
        })
    }

    fn lower_type_decl(&self, node: Node<'_>) -> Stmt {
        let def = node.child_by_field_name("definition");
        let mut alias = match def.map(|d| (d.kind(), d)) {
            Some(("type_alias", d)) => self.first_named(d).map(|t| self.lower_type(t)),
            _ => None,
        };
        let mut variants = match def.map(|d| (d.kind(), d)) {
            Some(("union_type", d)) => self.map_of_kind(d, "variant", Self::lower_variant),
            Some(("record_type", d)) => vec![TypeVariant {
                name: self.field_text(node, "name"),
                fields: self.lower_field_decls(d),
            }],
            _ => Vec::new(),
        };
        // Preserve the historical `type Color = Red` one-variant union while
        // making the adoption-friendly `type UserId = int` spelling an alias.
        // Osprey constructors conventionally begin uppercase; a lone lowercase
        // RHS therefore has an unambiguous type-alias interpretation.
        let lowercase_alias = match variants.as_slice() {
            [variant]
                if variant.fields.is_empty()
                    && variant.name.chars().next().is_some_and(char::is_lowercase) =>
            {
                Some(variant.name.clone())
            }
            _ => None,
        };
        if alias.is_none() {
            if let Some(name) = lowercase_alias {
                alias = Some(TypeExpr::named(name));
                variants.clear();
            }
        }
        Stmt::Type {
            name: self.field_text(node, "name"),
            type_params: self.lower_type_params(node),
            variants,
            alias,
            validation_func: self
                .first_child_of_kind(node, "type_validation")
                .and_then(|tv| self.first_named(tv))
                .map(|n| self.text(n)),
            doc: self.doc_text(node),
            position: Some(self.pos(node)),
        }
    }

    fn lower_variant(&self, node: Node<'_>) -> TypeVariant {
        let fields = match node.child_by_field_name("positional") {
            Some(payload) => self.lower_positional_payload(payload),
            None => node
                .child(node.child_count().saturating_sub(1))
                .filter(|_| node.child_count() > 1)
                .map(|_| self.lower_field_decls(node))
                .unwrap_or_default(),
        };
        TypeVariant {
            name: self.field_text(node, "name"),
            fields,
        }
    }

    /// A variant declared with a positional payload, as `(name, slot count)`
    /// for the shared construction table ([TYPE-UNION-POSITIONAL]).
    fn positional_ctor(&self, variant: Node<'_>) -> Option<(String, usize)> {
        let payload = variant.child_by_field_name("positional")?;
        Some((
            self.field_text(variant, "name"),
            payload.named_child_count(),
        ))
    }

    /// `Node(Tree, Tree)` — a positional payload, whose slots carry generated
    /// index names because they have no source spelling
    /// ([TYPE-UNION-POSITIONAL]).
    fn lower_positional_payload(&self, node: Node<'_>) -> Vec<TypeField> {
        (0..node.named_child_count())
            .filter_map(|i| node.named_child(i))
            .enumerate()
            .map(|(slot, ty)| TypeField {
                name: osprey_ast::positional_field_name(slot),
                ty: self.text(ty),
                constraint: None,
            })
            .collect()
    }

    fn lower_field_decls(&self, node: Node<'_>) -> Vec<TypeField> {
        let mut out = Vec::new();
        for fd in self.descendants_of_kind(node, "field_declaration") {
            out.push(TypeField {
                name: self.field_text(fd, "name"),
                // Keep the full source text (`List<Self>`, `(int) -> bool`) —
                // taking just the lowered head name would collapse a function
                // type to `fn` and a generic to its constructor.
                ty: fd
                    .child_by_field_name("type")
                    .map(|n| self.text(n))
                    .unwrap_or_default(),
                constraint: None,
            });
        }
        out
    }

    pub(crate) fn lower_operations(&self, node: Node<'_>) -> Vec<EffectOperation> {
        self.named_of_kind(node, "operation_declaration")
            .iter()
            .map(|op| EffectOperation {
                name: self.field_text(*op, "name"),
                ty: op
                    .child_by_field_name("type")
                    .map(|n| self.text(n))
                    .unwrap_or_default(),
                parameters: Vec::new(),
                return_type: String::new(),
                doc: self.doc_text(*op),
                // Anchor on the NAME, not the node: a leading `///` is now a
                // child of `operation_declaration`, so `pos(op)` would land on
                // the comment and defeat position-based hover resolution.
                position: Some(self.field_pos(*op, "name")),
            })
            .collect()
    }

    pub(crate) fn lower_params(&self, list: Option<Node<'_>>) -> Vec<Parameter> {
        let Some(list) = list else { return Vec::new() };
        self.named_of_kind(list, "parameter")
            .iter()
            .enumerate()
            .map(|(slot, p)| Parameter {
                // `_` takes the generated ignored-parameter name for its slot,
                // the same name the ML lowerer emits, so `|acc, _| => …` and
                // `\(acc, _) => …` stay IR-equivalent twins ([PARAM-WILDCARD]).
                name: match self.field_text(*p, "name") {
                    name if name == WILDCARD_PARAM => osprey_ast::wildcard_param_name(slot),
                    name => name,
                },
                ty: p.child_by_field_name("type").map(|n| self.lower_type(n)),
            })
            .collect()
    }

    fn lower_extern_params(&self, list: Option<Node<'_>>) -> Vec<ExternParameter> {
        let Some(list) = list else { return Vec::new() };
        self.named_of_kind(list, "extern_parameter")
            .iter()
            .map(|p| ExternParameter {
                name: self.field_text(*p, "name"),
                ty: p
                    .child_by_field_name("type")
                    .map_or_else(|| TypeExpr::named(""), |n| self.lower_type(n)),
            })
            .collect()
    }

    /// Lower a declaration's `type_parameters` field into variance-carrying
    /// [`TypeParam`]s. Implements [TYPE-VARIANCE-DECL].
    pub(crate) fn lower_type_params(&self, node: Node<'_>) -> Vec<TypeParam> {
        let Some(list) = node.child_by_field_name("type_parameters") else {
            return Vec::new();
        };
        self.named_of_kind(list, "type_parameter")
            .iter()
            .map(|tp| TypeParam {
                name: self.field_text(*tp, "name"),
                variance: match tp.child_by_field_name("variance") {
                    Some(v) if self.text(v) == "out" => Variance::Covariant,
                    Some(_) => Variance::Contravariant,
                    None => Variance::Invariant,
                },
            })
            .collect()
    }

    /// Lower an effect row into effect references with optional type
    /// arguments (`!State<int>`). Implements [EFFECTS-GENERIC-ROWS].
    pub(crate) fn lower_effects(&self, effects: Option<Node<'_>>) -> Vec<EffectRef> {
        let Some(effects) = effects else {
            return Vec::new();
        };
        self.descendants_of_kind(effects, "effect_ref")
            .iter()
            .map(|r| EffectRef {
                name: self.field_text(*r, "name"),
                type_args: self
                    .first_child_of_kind(*r, "type_arguments")
                    .and_then(|ta| self.first_child_of_kind(ta, "type_list"))
                    .map(|l| self.lower_type_list(l))
                    .unwrap_or_default(),
                position: Some(self.pos(*r)),
            })
            .collect()
    }

    /// Lower a `_type` node (function/generic/array/identifier).
    pub(crate) fn lower_type(&self, node: Node<'_>) -> TypeExpr {
        match node.kind() {
            "type_identifier" => TypeExpr::named(
                self.first_named(node)
                    .map(|n| self.text(n))
                    .unwrap_or_default(),
            ),
            "generic_type" => {
                let mut t = TypeExpr::named(self.field_text(node, "name"));
                if let Some(list) = self.first_child_of_kind(node, "type_list") {
                    t.generic_params = self.lower_type_list(list);
                }
                t
            }
            "array_type" => {
                let mut t = TypeExpr::named(self.field_text(node, "name"));
                t.is_array = true;
                t.array_element = self.descendants_type_in(node).map(Box::new);
                t
            }
            "function_type" => {
                let types = self
                    .first_child_of_kind(node, "type_list")
                    .map(|l| self.lower_type_list(l))
                    .unwrap_or_default();
                // last bare type child is the return type
                let ret = self.last_type_child(node);
                TypeExpr {
                    name: "fn".into(),
                    generic_params: Vec::new(),
                    is_array: false,
                    array_element: None,
                    is_function: true,
                    parameter_types: types,
                    return_type: ret.map(Box::new),
                    position: Some(self.pos(node)),
                }
            }
            _ => TypeExpr::named(self.text(node)),
        }
    }

    pub(crate) fn lower_type_list(&self, list: Node<'_>) -> Vec<TypeExpr> {
        let mut out = Vec::new();
        let mut cursor = list.walk();
        for child in list.named_children(&mut cursor) {
            if Self::is_type_kind(child.kind()) {
                out.push(self.lower_type(child));
            }
        }
        out
    }

    fn descendants_type_in(&self, node: Node<'_>) -> Option<TypeExpr> {
        let mut cursor = node.walk();
        let found = node
            .named_children(&mut cursor)
            .find(|c| Self::is_type_kind(c.kind()));
        found.map(|c| self.lower_type(c))
    }

    pub(crate) fn last_type_child(&self, node: Node<'_>) -> Option<TypeExpr> {
        let mut cursor = node.walk();
        let found = node
            .named_children(&mut cursor)
            .filter(|c| Self::is_type_kind(c.kind()))
            .last();
        found.map(|c| self.lower_type(c))
    }

    fn is_type_kind(kind: &str) -> bool {
        matches!(
            kind,
            "type_identifier" | "generic_type" | "array_type" | "function_type"
        )
    }

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
                    && super::is_i64_min_magnitude_text(&self.text(inner));
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
                    .map(|fp| self.field_pattern_names(fp))
                    .unwrap_or_default()
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
            .map(|n| self.text(n))
            .unwrap_or_default()
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
    fn map_of_kind<T>(
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

/// Negate a numeric literal for a `-N` pattern; non-numeric literals pass through.
fn negate_literal(e: Expr) -> Expr {
    match e {
        Expr::Integer(n) => Expr::Integer(-n),
        Expr::Float(f) => Expr::Float(-f),
        other => other,
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: an out-of-bounds index is a test failure, not a production panic"
)]
mod tests {
    use crate::parse_tree;
    use crate::test_support::{one_stmt, stmts};
    use osprey_ast::{Expr, Pattern, Stmt};
    use tree_sitter::Node;

    /// Find the first descendant node of a given kind anywhere in the tree.
    fn find_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
        if node.kind() == kind {
            return Some(node);
        }
        let mut cursor = node.walk();
        let children: Vec<Node<'t>> = node.children(&mut cursor).collect();
        children.into_iter().find_map(|c| find_kind(c, kind))
    }

    // ---------- [TESTING-DOC] expression-statement documentation ----------

    /// The doc comment lowered onto a statement, or `None`.
    fn stmt_doc(stmt: &Stmt) -> Option<&osprey_ast::DocComment> {
        match stmt {
            Stmt::Expr { doc, .. } | Stmt::Let { doc, .. } | Stmt::Function { doc, .. } => {
                doc.as_ref()
            }
            _ => None,
        }
    }

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
        assert_eq!(
            stmt_doc(&all[0]).map(|d| d.summary.as_str()),
            Some("First.")
        );
        assert_eq!(stmt_doc(&all[1]).map(|d| d.summary.as_str()), None);
        assert_eq!(
            stmt_doc(&all[2]).map(|d| d.summary.as_str()),
            Some("Third.")
        );
    }

    #[test]
    fn a_documented_declaration_after_a_documented_statement_keeps_its_own_doc() {
        // Adding the expression-statement slot must not steal a following
        // declaration's doc comment.
        let all = stmts("/// Runs it.\nprint(\"hi\")\n/// Adds.\nfn add(a, b) = a + b\n");
        assert_eq!(all.len(), 2);
        assert_eq!(
            stmt_doc(&all[0]).map(|d| d.summary.as_str()),
            Some("Runs it.")
        );
        assert!(matches!(all[1], Stmt::Function { .. }));
        assert_eq!(stmt_doc(&all[1]).map(|d| d.summary.as_str()), Some("Adds."));
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
        assert_eq!(
            stmt_doc(&statements[0]).map(|d| d.summary.as_str()),
            Some("Inner case.")
        );
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
                assert!(variants[0].fields.is_empty());
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
                let vs: Vec<osprey_ast::Variance> =
                    type_params.iter().map(|p| p.variance).collect();
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
                assert!(effects[0].type_args.is_empty());
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
}
