//! Logical imports and file contributions retain canonical project boundaries.
//! Implements [FLAVOR-PYTHON-INTEROP], [FLAVOR-PYTHON-SELECT].
use super::context::{children, position, Lowerer, Result};
use osprey_ast::{
    DocScope, Expr, ImportDecl, ImportMember, ImportSelection, ImportTarget, ModuleItem,
    ModuleKind, NamespaceName, Position, Program, Stmt, SymbolPath, Visibility,
};
use std::collections::BTreeMap;
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
        let Some((path, position)) = &header.module else {
            return Ok(statements);
        };
        let (imports, declarations): (Vec<_>, Vec<_>) = statements
            .into_iter()
            .partition(|statement| matches!(statement, Stmt::Import(_)));
        let body = self.module_items(declarations)?;
        let mut result = imports;
        result.push(module_statement(path, *position, body, doc));
        Ok(result)
    }

    fn module_items(&self, statements: Vec<Stmt>) -> Result<Vec<ModuleItem>> {
        self.check_export_names(&statements)?;
        Ok(statements
            .into_iter()
            .map(|statement| self.module_item(statement))
            .collect())
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
            .filter_map(declared_name)
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

    fn module_item(&self, statement: Stmt) -> ModuleItem {
        ModuleItem {
            visibility: if declared_name(&statement)
                .is_some_and(|name| self.exports.contains_key(name))
            {
                Visibility::Exported
            } else {
                Visibility::Private
            },
            opaque: false,
            declaration: Box::new(statement),
        }
    }

    pub(super) fn mark_export(&mut self, node: Node<'_>) -> Result<()> {
        if !self.file_scope() {
            return self.unsupported(node);
        }
        let name = self.text(node).to_owned();
        if self.exports.insert(name.clone(), position(node)).is_some() {
            return Err(Self::error(node, format!("duplicate export `{name}`")));
        }
        Ok(())
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
            if self.exports.insert(name.clone(), position(item)).is_some() {
                return Err(Self::error(item, format!("duplicate export `{name}`")));
            }
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

fn declared_name(statement: &Stmt) -> Option<&str> {
    match statement {
        Stmt::Let { name, .. }
        | Stmt::Function { name, .. }
        | Stmt::Extern { name, .. }
        | Stmt::Type { name, .. }
        | Stmt::Effect { name, .. } => Some(name),
        _ => None,
    }
}

fn identifier(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| ch.is_alphabetic() || ch == '_')
        && text.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
}

#[derive(Default)]
struct Header {
    namespace: Option<(String, Position)>,
    module: Option<(String, Position)>,
    seen: BTreeMap<String, Position>,
}

impl Header {
    fn namespace_contribution(&self, statements: Vec<Stmt>) -> Vec<Stmt> {
        match &self.namespace {
            Some((name, position)) => vec![Stmt::Namespace {
                name: NamespaceName::Identifier(name.clone()),
                body: statements,
                file_scoped: true,
                doc: None,
                inner_doc: None,
                position: Some(*position),
            }],
            None => statements,
        }
    }
    fn read(lowerer: &Lowerer<'_>, node: Node<'_>) -> Result<Self> {
        let first_code = children(node).first().map_or(usize::MAX, Node::start_byte);
        let mut header = Self::default();
        header.comments(lowerer, node, first_code)?;
        if header.module.is_some() && header.namespace.is_none() {
            return Err(Lowerer::error(
                node,
                "a module directive requires a namespace directive",
            ));
        }
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
                self.directive(node, directive.trim())?;
            }
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            self.comments(lowerer, child, first_code)?;
        }
        Ok(())
    }

    fn directive(&mut self, node: Node<'_>, directive: &str) -> Result<()> {
        let Some((key, value)) = directive.split_once('=') else {
            return Err(Lowerer::error(node, "expected `osprey: key=value`"));
        };
        let (key, value) = (key.trim(), value.trim());
        if self.seen.insert(key.into(), position(node)).is_some() {
            return Err(Lowerer::error(node, format!("duplicate {key} directive")));
        }
        self.apply_directive(node, key, value)
    }

    fn apply_directive(&mut self, node: Node<'_>, key: &str, value: &str) -> Result<()> {
        if key == "flavor" {
            return flavor_directive(node, value);
        }
        let slot = match key {
            "namespace" if identifier(value) => &mut self.namespace,
            "module" if value.split('.').all(identifier) => &mut self.module,
            _ => {
                return Err(Lowerer::error(
                    node,
                    "unknown or malformed Osprey directive",
                ))
            }
        };
        *slot = Some((value.into(), position(node)));
        Ok(())
    }
}

fn flavor_directive(node: Node<'_>, value: &str) -> Result<()> {
    value
        .parse::<crate::Flavor>()
        .map(|_| ())
        .map_err(|message| Lowerer::error(node, message))
}

fn module_statement(
    path: &str,
    position: Position,
    body: Vec<ModuleItem>,
    inner_doc: Option<osprey_ast::DocComment>,
) -> Stmt {
    Stmt::Module {
        path: SymbolPath::new(path.split('.')),
        kind: ModuleKind::Plain,
        signature: None,
        body,
        doc: None,
        inner_doc,
        position: Some(position),
    }
}
