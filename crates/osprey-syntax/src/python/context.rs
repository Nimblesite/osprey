//! Source identities and lexical scope owned exclusively by the Python frontend.
use crate::{BindingKind, BindingRange, SyntaxError};
use osprey_ast::{Expr, Position, Stmt, TypeExpr};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::Node;

pub(super) type Result<T> = std::result::Result<T, SyntaxError>;

pub(super) struct Lowerer<'s> {
    source: &'s str,
    scopes: Vec<Scope>,
    generated: usize,
    pub(super) errors: Vec<SyntaxError>,
    pub(super) bindings: Vec<BindingRange>,
    pub(super) annotations: Vec<crate::AnnotationEdit>,
    pub(super) exports: BTreeMap<String, Position>,
}

struct Scope {
    bindings: BTreeMap<String, bool>,
    mutable: BTreeSet<String>,
    typing_names: BTreeMap<String, String>,
    constructors: BTreeSet<String>,
    imports: BTreeSet<String>,
    function_boundary: bool,
}

impl Scope {
    fn new(function_boundary: bool) -> Self {
        Self {
            bindings: BTreeMap::new(),
            mutable: BTreeSet::new(),
            typing_names: BTreeMap::new(),
            constructors: BTreeSet::new(),
            imports: BTreeSet::new(),
            function_boundary,
        }
    }
}

pub(super) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|child| child.kind() != "comment")
        .collect()
}

pub(super) fn position(node: Node<'_>) -> Position {
    crate::default::position_from_point(node.start_position())
}

pub(super) fn unit() -> Expr {
    Expr::Block {
        statements: Vec::new(),
        value: None,
        position: None,
    }
}

pub(super) fn apply(function: Expr, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function: Box::new(function),
        arguments,
        named_arguments: Vec::new(),
    }
}

impl<'s> Lowerer<'s> {
    pub(super) fn new(source: &'s str) -> Self {
        Self {
            source,
            scopes: vec![Scope::new(true)],
            generated: 0,
            errors: Vec::new(),
            bindings: Vec::new(),
            annotations: Vec::new(),
            exports: BTreeMap::new(),
        }
    }

    pub(super) fn text(&self, node: Node<'_>) -> &'s str {
        self.source.get(node.byte_range()).unwrap_or_default()
    }

    pub(super) fn error(node: Node<'_>, message: impl Into<String>) -> SyntaxError {
        SyntaxError {
            message: message.into(),
            position: position(node),
        }
    }

    pub(super) fn field<'t>(node: Node<'t>, name: &str) -> Result<Node<'t>> {
        node.child_by_field_name(name)
            .ok_or_else(|| Self::error(node, format!("missing Python {name}")))
    }

    pub(super) fn single<'t>(&self, node: Node<'t>) -> Result<Node<'t>> {
        let items = children(node);
        match items.as_slice() {
            [item] => Ok(*item),
            _ => self.unsupported(node),
        }
    }

    pub(super) fn unsupported<T>(&self, node: Node<'_>) -> Result<T> {
        Err(Self::error(
            node,
            format!(
                "Python `{}` has no supported Osprey lowering",
                self.text(node)
            ),
        ))
    }

    pub(super) fn scoped<T>(
        &mut self,
        boundary: bool,
        action: impl FnOnce(&mut Self) -> Result<T>,
    ) -> Result<T> {
        self.scopes.push(Scope::new(boundary));
        let result = action(self);
        let _ = self.scopes.pop();
        result
    }

    /// A fresh compiler-generated binding name, unspellable in source.
    pub(super) fn fresh(&mut self, role: &str) -> String {
        let name = osprey_ast::generated_name(role, self.generated);
        self.generated += 1;
        name
    }

    pub(super) fn lookup(&self, name: &str) -> Option<bool> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.bindings.get(name).copied())
    }

    pub(super) fn file_scope(&self) -> bool {
        self.scopes.len() == 1
    }

    pub(super) fn import_binding(&mut self, node: Node<'_>) -> Result<()> {
        self.bind(node, false)?;
        let name = self.text(node).to_owned();
        if let Some(scope) = self.scopes.last_mut() {
            let _ = scope.imports.insert(name);
        }
        Ok(())
    }

    pub(super) fn imported_name(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .find(|scope| scope.bindings.contains_key(name))
            .is_some_and(|scope| scope.imports.contains(name))
    }

    pub(super) fn typing_name(&self, name: &str) -> Option<&str> {
        let scope = self
            .scopes
            .iter()
            .rev()
            .find(|scope| scope.bindings.contains_key(name))?;
        scope.typing_names.get(name).map(String::as_str)
    }

    pub(super) fn import_typing_name(&mut self, name: String, canonical: String) {
        if let Some(scope) = self.scopes.last_mut() {
            let _ = scope.typing_names.insert(name, canonical);
        }
    }

    pub(super) fn record_constructor(&mut self, name: Node<'_>) {
        let name = self.text(name).to_owned();
        if let Some(scope) = self.scopes.last_mut() {
            let _ = scope.constructors.insert(name);
        }
    }

    pub(super) fn is_constructor(&self, name: &str) -> bool {
        self.scopes
            .iter()
            .rev()
            .find(|scope| scope.bindings.contains_key(name))
            .map_or_else(
                || osprey_ast::is_constructor_name(name),
                |scope| scope.constructors.contains(name),
            )
    }

    pub(super) fn assignment_kind(&self, name: &str) -> Option<bool> {
        for scope in self.scopes.iter().rev() {
            if let Some(mutable) = scope.bindings.get(name) {
                return Some(*mutable);
            }
            if scope.function_boundary {
                break;
            }
        }
        None
    }

    pub(super) fn will_be_assigned(&self, name: &str) -> bool {
        self.scopes
            .last()
            .is_some_and(|scope| scope.mutable.contains(name))
    }

    pub(super) fn plan_bindings(&mut self, node: Node<'_>) {
        let mut counts = BTreeMap::new();
        for child in children(node) {
            self.count_writes(child, &mut counts);
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.mutable.extend(
                counts
                    .into_iter()
                    .filter_map(|(name, count)| (count > 1).then_some(name)),
            );
        }
    }

    fn count_writes(&self, node: Node<'_>, counts: &mut BTreeMap<String, usize>) {
        if matches!(
            node.kind(),
            "function_definition" | "class_definition" | "lambda"
        ) {
            return;
        }
        if node.kind() == "assignment" {
            if let Some(name) = node
                .child_by_field_name("left")
                .filter(|n| n.kind() == "identifier")
            {
                *counts.entry(self.text(name).to_owned()).or_default() += 1;
            }
        }
        for child in children(node) {
            self.count_writes(child, counts);
        }
    }

    /// A name no binding may take: an Osprey intrinsic, or the builtin a
    /// Python spelling lowers onto (`length` for `len`, `intDiv` for `//`).
    fn bindable(node: Node<'_>, name: &str) -> Result<()> {
        if reserved(name) {
            return Err(Self::error(
                node,
                format!("`{name}` is a reserved Osprey intrinsic"),
            ));
        }
        if let Some(python) = python_lowering_target(name) {
            return Err(Self::error(
                node,
                format!("binding `{name}` shadows the builtin that `{python}` lowers onto"),
            ));
        }
        Ok(())
    }

    pub(super) fn bind(&mut self, node: Node<'_>, mutable: bool) -> Result<()> {
        let name = self.text(node).to_owned();
        Self::bindable(node, &name)?;
        let Some(scope) = self.scopes.last_mut() else {
            return self.unsupported(node);
        };
        if scope.bindings.insert(name.clone(), mutable).is_some() {
            return Err(Self::error(
                node,
                format!("binding `{name}` cannot be reassigned: it is immutable or declared Final"),
            ));
        }
        Ok(())
    }

    pub(super) fn declaration_name<'t>(&mut self, node: Node<'t>) -> Result<Node<'t>> {
        let name = Self::field(node, "name")?;
        self.bind(name, false)?;
        Ok(name)
    }

    pub(super) fn let_statement(
        &self,
        name: Node<'_>,
        value: Expr,
        mutable: bool,
        ty: Option<TypeExpr>,
    ) -> Stmt {
        Stmt::Let {
            name: self.text(name).into(),
            mutable,
            ty,
            value,
            doc: None,
            position: Some(position(name)),
        }
    }

    pub(super) fn reject_field(&self, node: Node<'_>, name: &str) -> Result<()> {
        if node.child_by_field_name(name).is_some() {
            return self.unsupported(node);
        }
        Ok(())
    }

    pub(super) fn identifier<'t>(&self, node: Node<'t>) -> Result<Node<'t>> {
        self.kind(node, "identifier")
    }

    pub(super) fn kind<'t>(&self, node: Node<'t>, kind: &str) -> Result<Node<'t>> {
        if node.kind() != kind {
            return self.unsupported(node);
        }
        Ok(node)
    }

    pub(super) fn synchronous(node: Node<'_>) -> Result<()> {
        let mut cursor = node.walk();
        if node
            .children(&mut cursor)
            .any(|child| child.kind() == "async")
        {
            return Err(Self::error(
                node,
                "async functions do not have an Osprey lowering; use fibers",
            ));
        }
        Ok(())
    }

    pub(super) fn binding(&mut self, node: Node<'_>, owner: Node<'_>, kind: BindingKind) {
        self.bindings.push(BindingRange {
            owner_position: Some(position(owner)),
            kind,
            name: self.text(node).to_owned(),
            occurrence: 0,
            range: node.byte_range(),
        });
    }
}

fn reserved(name: &str) -> bool {
    matches!(
        name,
        "perform" | "resume" | "update" | "spawn" | "await_fiber" | "yield_fiber" | "send" | "recv"
    )
}

/// The Python spelling whose lowering lands on this Osprey name. Binding the
/// Osprey name would silently redirect that lowering — `len` onto a local
/// `length`, `//` onto a local `intDiv` — so the binding is rejected with a
/// diagnostic naming both names ([FLAVOR-PYTHON-EXPRESSIONS]).
fn python_lowering_target(name: &str) -> Option<&'static str> {
    match name {
        "length" => Some("len"),
        "intDiv" => Some("//"),
        _ => None,
    }
}
