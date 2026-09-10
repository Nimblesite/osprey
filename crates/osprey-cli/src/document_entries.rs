//! One complete declaration walk shared by API export and executable docs.
//! Implements [DOC-EXPORT], [DOC-ATTACH], [DOC-EFFECT-OP], [DOC-DOCTEST-HARNESS].

use osprey_ast::{DocComment, DocExample, DocScope, Program, Stmt, Visibility};

#[derive(Clone)]
pub(crate) struct DocEntry {
    pub(crate) qualified_name: String,
    pub(crate) symbol_name: String,
    pub(crate) kind: &'static str,
    pub(crate) doc: DocComment,
    pub(crate) inner_doc: Option<DocComment>,
    pub(crate) scope: Vec<usize>,
    pub(crate) public: bool,
    pub(crate) declaration: Option<Stmt>,
    pub(crate) operation_type: Option<String>,
    pub(crate) module_kind: Option<osprey_ast::ModuleKind>,
}

impl DocEntry {
    pub(crate) fn examples(&self) -> impl Iterator<Item = &DocExample> {
        self.doc
            .examples
            .iter()
            .chain(self.inner_doc.iter().flat_map(|doc| &doc.examples))
    }

    pub(crate) fn markdown(&self) -> String {
        std::iter::once(&self.doc)
            .chain(self.inner_doc.iter())
            .map(DocComment::render_markdown)
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

pub(crate) fn collect(program: &Program) -> Vec<DocEntry> {
    let mut entries = Vec::new();
    if let Some(doc) = &program.doc {
        entries.push(entry("File".into(), "File", Some(doc), &[], true));
    }
    collect_statements(&program.statements, "", &[], true, &mut entries);
    entries
}

fn collect_statements(
    statements: &[Stmt],
    prefix: &str,
    scope: &[usize],
    public: bool,
    entries: &mut Vec<DocEntry>,
) {
    for (index, statement) in statements.iter().enumerate() {
        collect_statement(statement, index, prefix, scope, public, entries);
    }
}

fn collect_statement(
    statement: &Stmt,
    index: usize,
    prefix: &str,
    scope: &[usize],
    public: bool,
    entries: &mut Vec<DocEntry>,
) {
    if container(statement, index, prefix, scope, public, entries) {
        return;
    }
    if let Some((name, kind, doc)) = declaration(statement) {
        let mut item = entry(qualified(prefix, name), kind, doc, scope, public);
        item.declaration = Some(statement.clone());
        entries.push(item);
    }
    if let Stmt::Effect {
        name, operations, ..
    } = statement
    {
        for operation in operations {
            let mut item = entry(
                qualified(&qualified(prefix, name), &operation.name),
                "Operation",
                operation.doc.as_ref(),
                scope,
                public,
            );
            item.operation_type = Some(operation.ty.clone());
            entries.push(item);
        }
    }
}

fn container(
    statement: &Stmt,
    index: usize,
    prefix: &str,
    scope: &[usize],
    public: bool,
    entries: &mut Vec<DocEntry>,
) -> bool {
    let nested = nested_scope(scope, index);
    match statement {
        Stmt::Namespace {
            name,
            doc,
            inner_doc,
            body,
            ..
        } => {
            let name = qualified(prefix, name.label());
            push_container(
                &name,
                "Namespace",
                doc.as_ref(),
                inner_doc.as_ref(),
                &nested,
                public,
                entries,
            );
            collect_statements(body, &name, &nested, public, entries);
        }
        Stmt::Module {
            path,
            kind,
            doc,
            inner_doc,
            body,
            ..
        } => {
            let name = qualified(prefix, &path.to_string());
            push_container(
                &name,
                "Module",
                doc.as_ref(),
                inner_doc.as_ref(),
                &nested,
                public,
                entries,
            );
            if let Some(entry) = entries.last_mut() {
                entry.module_kind = Some(*kind);
            }
            collect_module(body, &name, &nested, public, entries);
        }
        _ => return false,
    }
    true
}

fn collect_module(
    body: &[osprey_ast::ModuleItem],
    name: &str,
    scope: &[usize],
    public: bool,
    entries: &mut Vec<DocEntry>,
) {
    for (index, item) in body.iter().enumerate() {
        collect_statement(
            &item.declaration,
            index,
            name,
            scope,
            public && item.visibility == Visibility::Exported,
            entries,
        );
    }
}

fn push_container(
    name: &str,
    kind: &'static str,
    doc: Option<&DocComment>,
    inner_doc: Option<&DocComment>,
    scope: &[usize],
    public: bool,
    entries: &mut Vec<DocEntry>,
) {
    let mut container = entry(name.into(), kind, doc, scope, public);
    container.inner_doc = inner_doc.cloned();
    entries.push(container);
}

fn declaration(statement: &Stmt) -> Option<(&str, &'static str, Option<&DocComment>)> {
    match statement {
        Stmt::Function { name, doc, .. } => Some((name, "Function", doc.as_ref())),
        Stmt::Let { name, doc, .. } => Some((name, "Value", doc.as_ref())),
        Stmt::Extern { name, doc, .. } => Some((name, "Extern", doc.as_ref())),
        Stmt::Type { name, doc, .. } => Some((name, "Type", doc.as_ref())),
        Stmt::Effect { name, doc, .. } => Some((name, "Effect", doc.as_ref())),
        Stmt::Signature { name, doc, .. } => Some((name, "Signature", doc.as_ref())),
        Stmt::Expr { doc: Some(doc), .. } => Some(("Test", "Test", Some(doc))),
        _ => None,
    }
}

fn entry(
    qualified_name: String,
    kind: &'static str,
    doc: Option<&DocComment>,
    scope: &[usize],
    public: bool,
) -> DocEntry {
    DocEntry {
        symbol_name: qualified_name.clone(),
        qualified_name,
        kind,
        scope: scope.to_vec(),
        public,
        doc: doc
            .cloned()
            .unwrap_or_else(|| DocComment::new("", "", DocScope::Outer)),
        inner_doc: None,
        declaration: None,
        operation_type: None,
        module_kind: None,
    }
}

fn qualified(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.into()
    } else {
        format!("{prefix}::{name}")
    }
}

fn nested_scope(scope: &[usize], index: usize) -> Vec<usize> {
    scope
        .iter()
        .copied()
        .chain(std::iter::once(index))
        .collect()
}
