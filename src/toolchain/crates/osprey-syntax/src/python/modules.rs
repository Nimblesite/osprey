//! Logical imports and file contributions retain canonical project boundaries.
//! Implements [FLAVOR-PYTHON-INTEROP], [FLAVOR-PYTHON-SELECT].
use super::context::{children, position, Lowerer, Result};
use osprey_ast::{
    stmt_meta, DocScope, Expr, ImportDecl, ImportMember, ImportSelection, ImportTarget,
    NamespaceName, Program, Stmt, SymbolPath,
};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn file_program(
        &mut self,
        node: Node<'_>,
        statements: Vec<Stmt>,
    ) -> Result<Program> {
        let header = Header::read(self, node)?;
        let doc = self.docstring(node, DocScope::Inner)?;
        let has_module = header.module.is_some();
        let statements = self.module_contribution(&header, statements, doc.clone())?;
        let statements = header.namespace_contribution(statements);
        Ok(Program {
            statements,
            doc: (!has_module).then_some(doc).flatten(),
        })
    }

    fn module_contribution(
        &self,
        header: &Header,
        statements: Vec<Stmt>,
        doc: Option<osprey_ast::DocComment>,
    ) -> Result<Vec<Stmt>> {
        self.check_export_context(header)?;
        if header.module.is_some() {
            self.check_export_names(&statements)?;
        }
        Ok(header.module_contribution(statements, |name| self.exports.contains_key(name), doc))
    }

    fn check_export_context(&self, header: &Header) -> Result<()> {
        if header.module.is_none() {
            if let Some((name, position)) = self.exports.first_key_value() {
                return Err(crate::SyntaxError {
                    message: format!("export `{name}` requires a module directive"),
                    position: *position,
                });
            }
        }
        Ok(())
    }

    fn check_export_names(&self, statements: &[Stmt]) -> Result<()> {
        let names = statements
            .iter()
            .filter_map(stmt_meta::declaration_name)
            .collect::<std::collections::BTreeSet<_>>();
        for (name, position) in &self.exports {
            if !names.contains(name.as_str()) {
                return Err(crate::SyntaxError {
                    message: format!("export `{name}` is not a declaration in this file"),
                    position: *position,
                });
            }
        }
        Ok(())
    }

    /// Record `name` as exported at `at`, refusing a second declaration.
    fn record_export(&mut self, name: &str, at: Node<'_>) -> Result<()> {
        if self.exports.insert(name.to_owned(), position(at)).is_some() {
            return Err(Self::error(at, format!("duplicate export `{name}`")));
        }
        Ok(())
    }

    pub(super) fn mark_export(&mut self, node: Node<'_>) -> Result<()> {
        if !self.file_scope() {
            return self.unsupported(node);
        }
        let name = self.text(node).to_owned();
        self.record_export(&name, node)
    }

    pub(super) fn export_list(&mut self, node: Node<'_>) -> Result<()> {
        if !self.file_scope() || node.child_by_field_name("type").is_some() {
            return self.unsupported(node);
        }
        let value = self.kind(Self::field(node, "right")?, "list")?;
        for item in children(value) {
            let Expr::Str(name) = self.expr(item)? else {
                return self.unsupported(item);
            };
            if !identifier(&name) {
                return self.unsupported(item);
            }
            self.record_export(&name, item)?;
        }
        Ok(())
    }

    pub(super) fn imported(&mut self, node: Node<'_>) -> Result<Option<Stmt>> {
        let import = if node.kind() == "import_from_statement" {
            let module = Self::field(node, "module_name")?;
            if matches!(self.text(module), "typing" | "collections.abc") {
                self.typing_import(node)?;
                return Ok(None);
            }
            self.member_import(node, module)?
        } else {
            self.whole_import(node)?
        };
        if !self.file_scope() {
            return self.unsupported(node);
        }
        Ok(Some(Stmt::Import(import)))
    }

    fn member_import(&mut self, node: Node<'_>, module: Node<'_>) -> Result<ImportDecl> {
        let target = self.import_target(module)?;
        let members = children(node)
            .into_iter()
            .filter(|item| *item != module)
            .map(|item| self.import_member(item))
            .collect::<Result<_>>()?;
        Ok(ImportDecl {
            target,
            alias: None,
            selection: ImportSelection::Members(members),
            position: Some(position(node)),
        })
    }

    fn whole_import(&mut self, node: Node<'_>) -> Result<ImportDecl> {
        let item = self.single(node)?;
        let name = item.child_by_field_name("name").unwrap_or(item);
        let target = self.import_target(name)?;
        let alias = item.child_by_field_name("alias");
        if alias.is_none() && !target.path.is_empty() {
            return Err(Self::error(
                item,
                "a dotted module import requires an explicit `as` alias",
            ));
        }
        let binding = alias.unwrap_or(name);
        self.import_binding(binding)?;
        Ok(ImportDecl {
            target,
            alias: alias.map(|node| self.text(node).into()),
            selection: ImportSelection::Whole,
            position: Some(position(node)),
        })
    }

    fn import_member(&mut self, node: Node<'_>) -> Result<ImportMember> {
        let name = node.child_by_field_name("name").unwrap_or(node);
        let parts = self.logical_path(name)?;
        if parts.len() != 1 {
            return self.unsupported(name);
        }
        let alias = node.child_by_field_name("alias");
        self.import_binding(alias.unwrap_or(name))?;
        Ok(ImportMember {
            name: self.text(name).into(),
            alias: alias.map(|node| self.text(node).into()),
        })
    }

    fn import_target(&self, node: Node<'_>) -> Result<ImportTarget> {
        let parts = self.logical_path(node)?;
        let Some((namespace, path)) = parts.split_first() else {
            return self.unsupported(node);
        };
        Ok(ImportTarget {
            namespace: NamespaceName::Identifier(namespace.clone()),
            path: SymbolPath::new(path.iter().cloned()),
        })
    }

    pub(super) fn logical_path(&self, node: Node<'_>) -> Result<Vec<String>> {
        let text = self.text(node);
        if !text.split('.').all(identifier) {
            return self.unsupported(node);
        }
        Ok(text.split('.').map(str::to_owned).collect())
    }
}

use crate::file_header::identifier;

/// The file's directives, read from its leading `# osprey:` comments.
struct Header(crate::file_header::Directives);

impl std::ops::Deref for Header {
    type Target = crate::file_header::Directives;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Header {
    fn read(lowerer: &Lowerer<'_>, node: Node<'_>) -> Result<Self> {
        let first_code = children(node).first().map_or(usize::MAX, Node::start_byte);
        let mut header = Self(crate::file_header::Directives::default());
        header.comments(lowerer, node, first_code)?;
        header.check(position(node))?;
        Ok(header)
    }

    fn comments(&mut self, lowerer: &Lowerer<'_>, node: Node<'_>, first_code: usize) -> Result<()> {
        if node.kind() == "comment" {
            let text = lowerer.text(node).trim_start_matches('#').trim_start();
            if let Some(directive) = text.strip_prefix("osprey:") {
                if node.start_byte() >= first_code {
                    return Err(Lowerer::error(
                        node,
                        "Osprey directives must precede source declarations",
                    ));
                }
                self.0.directive(directive.trim(), position(node))?;
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.comments(lowerer, child, first_code)?;
        }
        Ok(())
    }
}
