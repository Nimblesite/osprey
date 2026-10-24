//! Lexical Python suites lower to Osprey blocks and flat functions.
use super::context::{children, position, unit, Lowerer, Result};
use crate::{AnnotationTarget, BindingKind};
use osprey_ast::{Expr, Program, Stmt};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn program(&mut self, node: Node<'_>) -> Program {
        let statements = self.file_statements(node);
        match self.file_program(node, statements) {
            Ok(program) => program,
            Err(error) => {
                self.errors.push(error);
                Program {
                    statements: Vec::new(),
                    doc: None,
                }
            }
        }
    }

    fn file_statements(&mut self, node: Node<'_>) -> Vec<Stmt> {
        self.plan_bindings(node);
        let nodes = self.body_nodes(node);
        let mut statements = Vec::new();
        let mut index = 0;
        while let Some(child) = nodes.get(index).copied() {
            match self.statement(child) {
                Ok(Some(statement)) => {
                    let next = nodes.get(index + 1).copied();
                    let (statement, skip) = self.with_attribute_doc(statement, next);
                    if skip {
                        index += 1;
                    }
                    statements.push(statement);
                }
                Ok(None) => (),
                Err(error) => self.errors.push(error),
            }
            index += 1;
        }
        statements
    }

    /// PEP 257 attribute docstrings: a string-literal statement directly after
    /// a module constant or a `type X = ...` alias documents that declaration
    /// ([FLAVOR-PYTHON-CST]). Returns whether the docstring was consumed.
    fn with_attribute_doc(&mut self, statement: Stmt, next: Option<Node<'_>>) -> (Stmt, bool) {
        let Some(next) = next else {
            return (statement, false);
        };
        let Some(doc) = self.attribute_doc_or_error(next) else {
            return (statement, false);
        };
        attach_attribute_doc(statement, doc)
    }

    fn attribute_doc_or_error(&mut self, next: Node<'_>) -> Option<osprey_ast::DocComment> {
        match self.attribute_docstring(next) {
            Ok(doc) => doc,
            Err(error) => {
                self.errors.push(error);
                None
            }
        }
    }

    fn statement(&mut self, node: Node<'_>) -> Result<Option<Stmt>> {
        let statement = match node.kind() {
            "function_definition" => self.function(node)?,
            "class_definition" => self.record(node)?,
            "decorated_definition" => self.decorated(node)?,
            "type_alias_statement" => self.type_alias(node)?,
            "import_from_statement" | "import_statement" => return self.imported(node),
            "expression_statement" => return self.expression_statement(Self::first(node)?),
            "pass_statement" => return Ok(None),
            "if_statement" => Self::expression_statement_value(self.if_expression(node, false)?),
            "with_statement" => Self::expression_statement_value(self.with_handler(node, false)?),
            "match_statement" => Self::expression_statement_value(self.matching(node, false)?),
            "for_statement" => Self::expression_statement_value(self.for_statement(node)?),
            _ => return self.unsupported(node),
        };
        Ok(Some(statement))
    }

    fn expression_statement_value(value: Expr) -> Stmt {
        Stmt::Expr {
            value,
            doc: None,
            position: None,
        }
    }

    fn expression_statement(&mut self, node: Node<'_>) -> Result<Option<Stmt>> {
        if node.kind() == "assignment" {
            if self.text(Self::field(node, "left")?) == "__all__" {
                self.export_list(node)?;
                return Ok(None);
            }
            return self.assignment(node).map(Some);
        }
        let value = self.expr(node)?;
        Ok(Some(Stmt::Expr {
            value,
            doc: None,
            position: Some(position(node)),
        }))
    }

    fn assignment(&mut self, node: Node<'_>) -> Result<Stmt> {
        let name = self.identifier(Self::field(node, "left")?)?;
        let value = self.expr(Self::field(node, "right")?)?;
        let annotation = node.child_by_field_name("type");
        if annotation.is_none() && self.assignment_kind(self.text(name)) == Some(true) {
            return Ok(Stmt::Assignment {
                name: self.text(name).into(),
                value,
                position: Some(position(name)),
            });
        }
        self.new_binding(name, value, annotation)
    }

    fn new_binding(
        &mut self,
        name: Node<'_>,
        value: Expr,
        annotation: Option<Node<'_>>,
    ) -> Result<Stmt> {
        let (ty, final_binding) = self.binding_type(annotation)?;
        let mutable = !final_binding && self.will_be_assigned(self.text(name));
        self.bind(name, mutable)?;
        self.binding(name, name, BindingKind::Variable);
        if let Some(ty) = annotation.filter(|_| !final_binding) {
            self.annotation(ty, name, AnnotationTarget::Binding);
        }
        Ok(self.let_statement(name, value, mutable, ty))
    }

    pub(super) fn function(&mut self, node: Node<'_>) -> Result<Stmt> {
        Self::synchronous(node)?;
        let _ = self.declaration_name(node)?;
        self.scoped(true, |lowerer| lowerer.function_body(node))
    }

    fn function_body(&mut self, node: Node<'_>) -> Result<Stmt> {
        let parameters = self.parameters(node)?;
        let return_type = self.return_annotation(node)?;
        let body = self.suite(Self::field(node, "body")?, true)?;
        let doc = self.docstring(Self::field(node, "body")?, osprey_ast::DocScope::Outer)?;
        Ok(Stmt::Function {
            name: self.text(Self::field(node, "name")?).into(),
            type_params: self.type_parameters(node)?,
            parameters,
            return_type,
            effects: Vec::new(),
            effect_tail: None,
            effect_row_present: false,
            body,
            doc,
            position: Some(position(node)),
        })
    }

    fn return_annotation(&mut self, node: Node<'_>) -> Result<Option<osprey_ast::TypeExpr>> {
        let Some(ty) = node.child_by_field_name("return_type") else {
            return Ok(None);
        };
        self.annotation(ty, node, AnnotationTarget::Return);
        self.type_expr(ty).map(Some)
    }

    pub(super) fn suite(&mut self, node: Node<'_>, tail: bool) -> Result<Expr> {
        if tail {
            Self::validate_returns(node)?;
        }
        self.plan_bindings(node);
        self.suite_nodes(node, tail)
    }

    fn suite_nodes(&mut self, node: Node<'_>, tail: bool) -> Result<Expr> {
        let nodes = self.body_nodes(node);
        let mut statements = Vec::new();
        for (index, child) in nodes.iter().enumerate() {
            let last = index + 1 == nodes.len();
            if let Some(value) = self.tail_value(*child, last && tail)? {
                return Ok(suite_block(*child, statements, Some(value)));
            }
            if let Some(statement) = self.statement(*child)? {
                statements.push(statement);
            }
        }
        Ok(suite_block(node, statements, None))
    }

    fn tail_value(&mut self, node: Node<'_>, tail: bool) -> Result<Option<Expr>> {
        if !tail {
            return Ok(None);
        }
        let value = match node.kind() {
            "if_statement" => self.if_expression(node, true)?,
            "with_statement" => self.with_handler(node, true)?,
            "match_statement" => self.matching(node, true)?,
            "return_statement" => self.returned(node)?,
            _ => return Ok(None),
        };
        Ok(Some(value))
    }

    fn returned(&mut self, node: Node<'_>) -> Result<Expr> {
        let value = children(node).into_iter().next();
        value.map_or_else(|| Ok(unit()), |value| self.expr(value))
    }

    /// `for name in iterator:` lowers to `forEach(iterator, callback)`; the
    /// iteration variable is local to the callback ([FLAVOR-PYTHON-EXPRESSIONS]).
    fn for_statement(&mut self, node: Node<'_>) -> Result<Expr> {
        Self::reject_async_for(node)?;
        let left = Self::field(node, "left")?;
        let name = self.loop_target(left)?;
        let iter = self.expr(Self::field(node, "right")?)?;
        let body = self.scoped(false, |lowerer| {
            lowerer.bind(left, false)?;
            lowerer.suite(Self::field(node, "body")?, false)
        })?;
        Ok(super::context::apply(
            Expr::Identifier("forEach".into()),
            vec![iter, loop_callback(&name, body, node)],
        ))
    }

    fn reject_async_for(node: Node<'_>) -> Result<()> {
        if node
            .children(&mut node.walk())
            .any(|child| child.kind() == "async")
        {
            return Err(Self::error(
                node,
                "async `for` has no Osprey lowering; use fibers or forEach",
            ));
        }
        if node.child_by_field_name("alternative").is_some() {
            return Err(Self::error(
                node,
                "`for ... else` has no Osprey lowering; track completion with an explicit flag",
            ));
        }
        Ok(())
    }

    fn if_expression(&mut self, node: Node<'_>, tail: bool) -> Result<Expr> {
        let condition = self.expr(Self::field(node, "condition")?)?;
        let then = self.scoped(false, |lowerer| {
            lowerer.suite(Lowerer::field(node, "consequence")?, tail)
        })?;
        let alternatives: Vec<_> = children(node)
            .into_iter()
            .filter(|n| matches!(n.kind(), "elif_clause" | "else_clause"))
            .collect();
        let otherwise = self.alternatives(&alternatives, tail)?;
        Ok(crate::desugar::bool_match(condition, then, otherwise))
    }

    fn alternatives(&mut self, nodes: &[Node<'_>], tail: bool) -> Result<Expr> {
        let Some((node, rest)) = nodes.split_first() else {
            return Ok(unit());
        };
        if node.kind() == "else_clause" {
            return self.scoped(false, |lowerer| {
                lowerer.suite(Lowerer::field(*node, "body")?, tail)
            });
        }
        let condition = self.expr(Self::field(*node, "condition")?)?;
        let then = self.scoped(false, |lowerer| {
            lowerer.suite(Lowerer::field(*node, "consequence")?, tail)
        })?;
        Ok(crate::desugar::bool_match(
            condition,
            then,
            self.alternatives(rest, tail)?,
        ))
    }
}

fn suite_block(node: Node<'_>, statements: Vec<Stmt>, value: Option<Expr>) -> Expr {
    crate::desugar::block(statements, value.map(Box::new), Some(position(node)))
}

/// The callback a `for` statement hands to `forEach`: one iteration variable,
/// the loop body, and no explicit return type.
fn loop_callback(name: &str, body: Expr, node: Node<'_>) -> Expr {
    Expr::Lambda {
        parameters: vec![osprey_ast::Parameter {
            name: name.into(),
            ty: None,
            inline_constraint: false,
        }],
        return_type: None,
        body: Box::new(body),
        position: Some(position(node)),
    }
}

/// Attach an attribute docstring to the preceding declaration, reporting
/// whether it was consumed (so the string statement is not a value).
fn attach_attribute_doc(mut statement: Stmt, doc: osprey_ast::DocComment) -> (Stmt, bool) {
    match &mut statement {
        Stmt::Let { doc: slot, .. }
        | Stmt::Type {
            alias: Some(_),
            doc: slot,
            ..
        } => *slot = Some(doc),
        _ => return (statement, false),
    }
    (statement, true)
}
