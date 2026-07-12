//! Default-flavor lowering for logical namespaces, imports, modules, exports,
//! and explicit signatures. These surface forms converge on the shared module
//! AST before project resolution begins. [MODULES-FLAVOR-PROJECTION]

use super::lower::Lowerer;
use crate::strings::unquote;
use osprey_ast::{
    ImportDecl, ImportMember, ImportSelection, ImportTarget, ModuleItem, NamespaceName,
    SignatureAscription, SignatureItem, SignatureType, Stmt, SymbolPath, TypeExpr, Visibility,
};
use tree_sitter::Node;

impl Lowerer<'_> {
    /// Lower both modern `::` imports and the original dotted Default spelling
    /// into one canonical import edge. [MODULES-IMPORT]
    pub(crate) fn lower_import(&self, node: Node<'_>) -> ImportDecl {
        if let Some(legacy) = node.child_by_field_name("legacy_target") {
            let mut segments = self.texts_of_kind(legacy, "identifier").into_iter();
            let namespace = segments.next().unwrap_or_default();
            return ImportDecl {
                target: ImportTarget {
                    namespace: NamespaceName::Identifier(namespace),
                    path: SymbolPath::new(segments),
                },
                alias: None,
                selection: ImportSelection::Whole,
                position: Some(self.pos(node)),
            };
        }

        let target = node.child_by_field_name("target");
        let namespace_node = target.and_then(|target| target.child_by_field_name("namespace"));
        let namespace = self.lower_namespace_name(namespace_node);
        let all_identifiers: Vec<String> = target
            .map(|target| {
                self.descendants_of_kind(target, "identifier")
                    .into_iter()
                    .map(|identifier| self.text(identifier))
                    .collect()
            })
            .unwrap_or_default();
        let path = match namespace {
            NamespaceName::Identifier(_) => SymbolPath::new(all_identifiers.into_iter().skip(1)),
            NamespaceName::Quoted(_) => SymbolPath::new(all_identifiers),
        };
        let tail = node.child_by_field_name("tail");
        let alias = tail
            .and_then(|tail| tail.child_by_field_name("alias"))
            .map(|alias| self.text(alias));
        let selection = match tail {
            Some(tail) if self.text(tail).trim_end().ends_with('*') => ImportSelection::Wildcard,
            Some(tail) if self.text(tail).contains('{') => ImportSelection::Members(
                self.descendants_of_kind(tail, "import_member")
                    .into_iter()
                    .map(|member| ImportMember {
                        name: self.field_text(member, "name"),
                        alias: member
                            .child_by_field_name("alias")
                            .map(|alias| self.text(alias)),
                    })
                    .collect(),
            ),
            _ => ImportSelection::Whole,
        };
        ImportDecl {
            target: ImportTarget { namespace, path },
            alias,
            selection,
            position: Some(self.pos(node)),
        }
    }

    pub(crate) fn lower_namespace_name(&self, node: Option<Node<'_>>) -> NamespaceName {
        let Some(node) = node else {
            return NamespaceName::Identifier(String::new());
        };
        let inner = if node.kind() == "namespace_name" {
            self.first_named(node).unwrap_or(node)
        } else {
            node
        };
        if inner.kind() == "string" {
            NamespaceName::Quoted(unquote(&self.text(inner)))
        } else {
            NamespaceName::Identifier(self.text(inner))
        }
    }

    pub(crate) fn lower_symbol_path(&self, node: Node<'_>) -> SymbolPath {
        SymbolPath::new(
            self.descendants_of_kind(node, "identifier")
                .into_iter()
                .map(|identifier| self.text(identifier)),
        )
    }

    pub(crate) fn lower_statement_children(&self, node: Node<'_>) -> Vec<Stmt> {
        self.named_of_kind(node, "statement")
            .iter()
            .filter_map(|statement| self.first_named(*statement))
            .filter_map(|statement| self.lower_stmt(statement))
            .collect()
    }

    pub(crate) fn lower_signature_ascription(&self, node: Node<'_>) -> SignatureAscription {
        SignatureAscription {
            path: node
                .child_by_field_name("path")
                .map_or_else(SymbolPath::default, |path| self.lower_symbol_path(path)),
            allow_extra: node.child_by_field_name("extra").is_some(),
        }
    }

    pub(crate) fn lower_module_item(&self, node: Node<'_>) -> Option<ModuleItem> {
        let export = self.first_child_of_kind(node, "export_declaration");
        let export_doc = export.and_then(|export| self.doc_text(export));
        let (declaration_node, visibility, opaque) = match export {
            Some(export) => (
                export.child_by_field_name("declaration")?,
                Visibility::Exported,
                export.child_by_field_name("opaque").is_some(),
            ),
            None => (self.first_named(node)?, Visibility::Private, false),
        };
        let mut declaration = self.lower_stmt(declaration_node)?;
        if let Some(doc) = export_doc {
            attach_doc(&mut declaration, doc);
        }
        // A bare uppercase alias is syntactically indistinguishable from the
        // legacy one-variant union. `opaque` supplies the missing intent.
        if opaque {
            if let Stmt::Type {
                variants, alias, ..
            } = &mut declaration
            {
                let alias_name = match variants.as_slice() {
                    [variant] if variant.fields.is_empty() => Some(variant.name.clone()),
                    _ => None,
                };
                if alias.is_none() {
                    if let Some(name) = alias_name {
                        *alias = Some(TypeExpr::named(name));
                        variants.clear();
                    }
                }
            }
        }
        Some(ModuleItem {
            visibility,
            opaque,
            declaration: Box::new(declaration),
        })
    }

    pub(crate) fn lower_signature_item(&self, node: Node<'_>) -> Option<SignatureItem> {
        let item = self.first_named(node)?;
        Some(match item.kind() {
            "signature_value" => SignatureItem::Value {
                name: self.field_text(item, "name"),
                ty: item
                    .child_by_field_name("type")
                    .map_or_else(|| TypeExpr::named(""), |ty| self.lower_type(ty)),
                position: Some(self.pos(item)),
            },
            "signature_function" => {
                let parameters = item
                    .child_by_field_name("parameters")
                    .map(|parameters| {
                        self.named_of_kind(parameters, "extern_parameter")
                            .into_iter()
                            .filter_map(|parameter| parameter.child_by_field_name("type"))
                            .map(|ty| self.lower_type(ty))
                            .collect()
                    })
                    .unwrap_or_default();
                SignatureItem::Function {
                    name: self.field_text(item, "name"),
                    type_params: self.lower_type_params(item),
                    parameters,
                    return_type: item
                        .child_by_field_name("return_type")
                        .map_or_else(|| TypeExpr::named("Unit"), |ty| self.lower_type(ty)),
                    effects: self.lower_effects(item.child_by_field_name("effects")),
                    position: Some(self.pos(item)),
                }
            }
            "signature_type" => SignatureItem::Type {
                name: self.field_text(item, "name"),
                type_params: self.lower_type_params(item),
                definition: item
                    .child_by_field_name("definition")
                    .map_or(SignatureType::Abstract, |ty| {
                        SignatureType::Manifest(self.lower_type(ty))
                    }),
                opaque: item.child_by_field_name("opaque").is_some(),
                position: Some(self.pos(item)),
            },
            "effect_declaration" => SignatureItem::Effect {
                name: self.field_text(item, "name"),
                type_params: self.lower_type_params(item),
                operations: self.lower_operations(item),
                position: Some(self.pos(item)),
            },
            "signature_module" => SignatureItem::Module {
                path: item
                    .child_by_field_name("path")
                    .map_or_else(SymbolPath::default, |path| self.lower_symbol_path(path)),
                signature: item.child_by_field_name("signature").map_or(
                    SignatureAscription {
                        path: SymbolPath::default(),
                        allow_extra: false,
                    },
                    |signature| self.lower_signature_ascription(signature),
                ),
                position: Some(self.pos(item)),
            },
            _ => return None,
        })
    }
}

fn attach_doc(statement: &mut Stmt, doc: osprey_ast::DocComment) {
    match statement {
        Stmt::Let { doc: slot, .. }
        | Stmt::Function { doc: slot, .. }
        | Stmt::Extern { doc: slot, .. }
        | Stmt::Type { doc: slot, .. }
        | Stmt::Effect { doc: slot, .. }
        | Stmt::Module { doc: slot, .. }
        | Stmt::Signature { doc: slot, .. } => *slot = Some(doc),
        _ => {}
    }
}

#[cfg(test)]
#[expect(
    clippy::indexing_slicing,
    reason = "test assertions: indexing failures should identify malformed ASTs"
)]
mod tests {
    use crate::parse_program;
    use osprey_ast::{
        Expr, ImportSelection, ModuleKind, NamespaceName, SignatureItem, SignatureType, Stmt,
        Visibility,
    };

    fn parse(src: &str) -> Vec<Stmt> {
        let parsed = parse_program(src);
        assert!(
            parsed.errors.is_empty(),
            "syntax errors: {:?}",
            parsed.errors
        );
        parsed.program.statements
    }

    #[test]
    fn lowers_file_and_block_namespaces_with_opaque_labels() {
        // The file-scoped form owns every following declaration in canonical
        // AST form. [MODULES-FILE-SCOPED-NAMESPACE]
        let statements = parse("namespace billing;\nlet x = 1\nfn f() = x\n");
        match &statements[0] {
            Stmt::Namespace {
                name,
                body,
                file_scoped,
                position,
            } => {
                assert_eq!(name, &NamespaceName::Identifier("billing".into()));
                assert!(*file_scoped);
                assert_eq!(body.len(), 2);
                assert_eq!(position.map(|p| p.line), Some(1));
            }
            other => panic!("expected file namespace, got {other:?}"),
        }

        let statements = parse("namespace \"billing/api\" { let x = 1 }\n");
        match &statements[0] {
            Stmt::Namespace {
                name,
                body,
                file_scoped,
                ..
            } => {
                assert_eq!(name, &NamespaceName::Quoted("billing/api".into()));
                assert!(!file_scoped);
                assert_eq!(body.len(), 1);
            }
            other => panic!("expected block namespace, got {other:?}"),
        }
    }

    #[test]
    fn lowers_every_import_selection_and_legacy_dots() {
        let statements = parse(
            "import billing::Tax\n\
             import billing::Tax as T\n\
             import billing::Tax::{addTax, zero as noTax}\n\
             import billing::Tax::*\n\
             import \"billing/api\" as api\n\
             import std.io.file\n",
        );
        let imports: Vec<_> = statements
            .iter()
            .map(|statement| match statement {
                Stmt::Import(import) => import,
                other => panic!("expected import, got {other:?}"),
            })
            .collect();
        assert_eq!(imports[0].target.namespace.label(), "billing");
        assert_eq!(imports[0].target.path.segments, ["Tax"]);
        assert_eq!(imports[1].alias.as_deref(), Some("T"));
        match &imports[2].selection {
            ImportSelection::Members(members) => {
                assert_eq!(members.len(), 2);
                assert_eq!(members[1].name, "zero");
                assert_eq!(members[1].alias.as_deref(), Some("noTax"));
            }
            other => panic!("expected member selection, got {other:?}"),
        }
        assert!(matches!(imports[3].selection, ImportSelection::Wildcard));
        assert!(imports[4].target.namespace.is_quoted());
        assert_eq!(imports[4].alias.as_deref(), Some("api"));
        assert_eq!(imports[5].target.namespace.label(), "std");
        assert_eq!(imports[5].target.path.segments, ["io", "file"]);
    }

    #[test]
    fn lowers_state_module_paths_ascription_exports_and_opaque_alias() {
        let statements = parse(
            "state module Storage::Memory : StoreSig + extra {\n\
               mut count = 0\n\
               export opaque type Store = int\n\
               export fn empty() -> Store = count\n\
             }\n",
        );
        match &statements[0] {
            Stmt::Module {
                path,
                kind,
                signature,
                body,
                position,
                ..
            } => {
                assert_eq!(path.segments, ["Storage", "Memory"]);
                assert_eq!(*kind, ModuleKind::State);
                let ascription = signature.as_ref().expect("signature");
                assert_eq!(ascription.path.segments, ["StoreSig"]);
                assert!(ascription.allow_extra);
                assert_eq!(body.len(), 3);
                assert_eq!(body[0].visibility, Visibility::Private);
                assert_eq!(body[1].visibility, Visibility::Exported);
                assert!(body[1].opaque);
                assert!(matches!(
                    body[1].declaration.as_ref(),
                    Stmt::Type {
                        alias: Some(alias),
                        variants,
                        ..
                    } if alias.name == "int" && variants.is_empty()
                ));
                assert_eq!(position.map(|p| p.line), Some(1));
            }
            other => panic!("expected state module, got {other:?}"),
        }
    }

    #[test]
    fn lowers_typed_signature_items() {
        let statements = parse(
            "signature StoreSig {\n\
               opaque type Store\n\
               type Count = int\n\
               let zero: Count\n\
               fn empty() -> Store\n\
               effect StoreFx { load: fn() -> Store }\n\
               module Nested : NestedSig + extra\n\
             }\n",
        );
        match &statements[0] {
            Stmt::Signature { name, items, .. } => {
                assert_eq!(name, "StoreSig");
                assert_eq!(items.len(), 6);
                assert!(matches!(
                    items[0],
                    SignatureItem::Type {
                        definition: SignatureType::Abstract,
                        opaque: true,
                        ..
                    }
                ));
                assert!(matches!(
                    items[1],
                    SignatureItem::Type {
                        definition: SignatureType::Manifest(_),
                        opaque: false,
                        ..
                    }
                ));
                assert!(matches!(items[2], SignatureItem::Value { .. }));
                assert!(matches!(items[3], SignatureItem::Function { .. }));
                assert!(matches!(items[4], SignatureItem::Effect { .. }));
                assert!(matches!(
                    &items[5],
                    SignatureItem::Module { signature, .. } if signature.allow_extra
                ));
            }
            other => panic!("expected signature, got {other:?}"),
        }
    }

    #[test]
    fn lowers_qualified_paths_in_calls_types_and_interpolation() {
        let statements = parse(
            "fn run(x: billing::Money) = billing::Tax::addTax(x)\n\
             let shown = \"${billing::Tax::addTax(1)}\"\n",
        );
        match &statements[0] {
            Stmt::Function {
                parameters,
                body: Expr::Call { function, .. },
                ..
            } => {
                assert_eq!(
                    parameters[0].ty.as_ref().map(|ty| ty.name.as_str()),
                    Some("billing::Money")
                );
                assert!(matches!(
                    function.as_ref(),
                    Expr::Path(path) if path.segments == ["billing", "Tax", "addTax"]
                ));
            }
            other => panic!("expected qualified call, got {other:?}"),
        }
        let Stmt::Let {
            value: Expr::InterpolatedStr(parts),
            ..
        } = &statements[1]
        else {
            panic!("expected interpolated let")
        };
        assert!(parts.iter().any(|part| matches!(
            part,
            osprey_ast::InterpolatedPart::Expr(Expr::Call { function, .. })
                if matches!(function.as_ref(), Expr::Path(_))
        )));
    }

    #[test]
    fn module_keywords_are_reserved_as_identifiers() {
        for keyword in [
            "namespace",
            "signature",
            "export",
            "opaque",
            "state",
            "as",
            "extra",
        ] {
            let parsed = parse_program(&format!("let {keyword} = 1\n"));
            assert!(
                parsed
                    .errors
                    .iter()
                    .any(|error| error.message.contains("reserved for the module system")),
                "expected {keyword:?} to be reserved; errors: {:?}",
                parsed.errors
            );
        }
    }
}
