//! Program, namespace, statement and declaration lowering.

use super::Lowerer;
use osprey_ast::{ModuleKind, Program, Stmt, SymbolPath, TypeExpr, TypeField, TypeVariant};
use tree_sitter::Node;

impl Lowerer<'_> {
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
            // A file-scoped namespace owns the declarations after its header UP
            // TO the next file-scoped header, which opens a SIBLING namespace.
            // Canonicalise that relationship here rather than making later
            // phases reinterpret source order. [MODULES-FILE-SCOPED-NAMESPACE]
            //
            // Taking the whole remainder and stopping instead filed every later
            // declaration under the FIRST namespace: `namespace two;` was
            // lowered as an ordinary nested statement and `module B` after it
            // answered to `one::B::v`, a name the source never wrote. Nothing
            // was reported — the call site asking for `two::B::v` was blamed.
            if Self::is_file_scoped_namespace(node) {
                let end = self.next_file_scoped(&source_statements, index.saturating_add(1));
                let body = source_statements
                    .get(index.saturating_add(1)..end)
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|s| self.first_named(*s))
                    .filter_map(|s| self.lower_stmt(s))
                    .collect();
                statements.push(Stmt::Namespace {
                    name: self.lower_namespace_name(node.child_by_field_name("name")),
                    body,
                    file_scoped: true,
                    doc: self.doc_text(node),
                    // A `namespace x;` header opens no brace body, so it has no
                    // inside for a `//!` to sit in; the file's own inner doc
                    // belongs to `Program` instead.
                    inner_doc: None,
                    position: Some(self.field_pos(node, "keyword")),
                });
                index = end;
                continue;
            }
            if let Some(stmt) = self.lower_stmt(node) {
                statements.push(stmt);
            }
            index += 1;
        }
        Program {
            statements,
            doc: self.inner_doc_text(root),
        }
    }

    /// A `namespace x;` header with no `{ … }` body — it scopes the rest of the
    /// file rather than a block. [MODULES-FILE-SCOPED-NAMESPACE]
    fn is_file_scoped_namespace(node: Node<'_>) -> bool {
        node.kind() == "namespace_declaration" && node.child_by_field_name("body").is_none()
    }

    /// Index of the next file-scoped namespace header at or after `from`, or the
    /// end of the file when there is none — the boundary of what the header
    /// before it owns.
    fn next_file_scoped(&self, statements: &[Node<'_>], from: usize) -> usize {
        statements
            .iter()
            .enumerate()
            .skip(from)
            .find(|(_, wrapper)| {
                self.first_named(**wrapper)
                    .is_some_and(Self::is_file_scoped_namespace)
            })
            .map_or(statements.len(), |(at, _)| at)
    }

    /// Lower a `namespace` declaration in either spelling. The brace form owns
    /// a body, and only that body can hold the `//!` documenting the namespace
    /// from inside ([DOC-SIGIL-INNER]); the `///` above the keyword documents it
    /// from outside and the two coexist.
    fn lower_namespace(&self, node: Node<'_>) -> Stmt {
        let body = node.child_by_field_name("body");
        Stmt::Namespace {
            name: self.lower_namespace_name(node.child_by_field_name("name")),
            body: body
                .map(|b| self.lower_statement_children(b))
                .unwrap_or_default(),
            file_scoped: body.is_none(),
            doc: self.doc_text(node),
            inner_doc: body.and_then(|b| self.inner_doc_text(b)),
            position: Some(self.field_pos(node, "keyword")),
        }
    }

    pub(crate) fn lower_stmt(&self, node: Node<'_>) -> Option<Stmt> {
        Some(match node.kind() {
            "import_statement" => Stmt::Import(self.lower_import(node)),
            "namespace_declaration" => self.lower_namespace(node),
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
            "function_declaration" => {
                let (effects, effect_tail) =
                    self.lower_effect_row(node.child_by_field_name("effects"));
                Stmt::Function {
                    name: self.field_text(node, "name"),
                    type_params: self.lower_type_params(node),
                    parameters: self.lower_params(node.child_by_field_name("parameters")),
                    return_type: node
                        .child_by_field_name("return_type")
                        .map(|n| self.lower_type(n)),
                    effects,
                    effect_tail,
                    effect_row_present: node.child_by_field_name("effects").is_some(),
                    body: self.lower_expr_field(node, "body"),
                    doc: self.doc_text(node),
                    position: Some(self.field_pos(node, "name")),
                }
            }
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
                inner_doc: self.inner_doc_text(node),
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
            opaque: false,
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
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .enumerate()
            .map(|(slot, ty)| TypeField {
                name: osprey_ast::positional_field_name(slot),
                ty: self.text(ty),
                constraint: None,
                effects: None,
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
                // A field's row is always closed: the grammar admits no tail.
                effects: fd.child_by_field_name("effects").map(|row| {
                    self.lower_effect_refs(&self.descendants_of_kind(row, "effect_ref"))
                }),
            });
        }
        out
    }
}
