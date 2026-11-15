//! Statements that update a local: a receiver statement `x.m(…);` and an
//! assignment `x = …;` rebind it, and a handler arm's assignment writes
//! handler state. Implements [FLAVOR-TS-RECEIVER], [FLAVOR-TS-FUNCTIONS] and
//! [EFFECTS-HANDLER-STATE].
use super::context::{Lowerer, Result};
use super::expressions::binary_spelling;
use osprey_ast::{Expr, Stmt};
use oxc_ast::ast::{
    AssignmentExpression, AssignmentTarget, Expression, ExpressionStatement, Statement,
};
use oxc_span::{GetSpan, Span};

/// The local a statement-level method chain `x.m(…).n(…);` starts from.
pub(super) fn receiver_root<'b>(expression: &'b Expression<'_>) -> Option<&'b str> {
    let Expression::CallExpression(call) = expression else {
        return None;
    };
    let Expression::StaticMemberExpression(member) = &call.callee else {
        return None;
    };
    match &member.object {
        Expression::Identifier(identifier) => Some(identifier.name.as_str()),
        inner => receiver_root(inner),
    }
}

pub(super) fn statement_root<'b>(statement: &'b Statement<'_>) -> Option<&'b str> {
    match statement {
        Statement::ExpressionStatement(expression) => receiver_root(&expression.expression),
        _ => None,
    }
}

/// The local a statement `x = …;` or `x += …;` reassigns.
pub(super) fn assigned_root<'b>(statement: &'b Statement<'_>) -> Option<&'b str> {
    let Statement::ExpressionStatement(expression) = statement else {
        return None;
    };
    let Expression::AssignmentExpression(assignment) = &expression.expression else {
        return None;
    };
    match &assignment.left {
        AssignmentTarget::AssignmentTargetIdentifier(target) => Some(target.name.as_str()),
        _ => None,
    }
}

/// Every receiver root in `statements`, through branches but not closures.
pub(super) fn receiver_roots<'b>(statements: &'b [Statement<'_>], roots: &mut Vec<&'b str>) {
    for statement in statements {
        roots.extend(statement_root(statement));
        match statement {
            Statement::BlockStatement(block) => receiver_roots(&block.body, roots),
            Statement::IfStatement(branch) => {
                receiver_roots(std::slice::from_ref(&branch.consequent), roots);
                if let Some(alternate) = &branch.alternate {
                    receiver_roots(std::slice::from_ref(alternate), roots);
                }
            }
            Statement::SwitchStatement(switch) => {
                for case in &switch.cases {
                    receiver_roots(&case.consequent, roots);
                }
            }
            _ => {}
        }
    }
}

/// A receiver statement's rebinding that nothing reads afterwards is a
/// discard: `app.listen(port);` as a last statement binds no name.
pub(super) fn discard_unread(statements: &mut [Stmt], value: Option<&Expr>, rebound: &[usize]) {
    for &index in rebound.iter().rev() {
        let (head, rest) = statements.split_at_mut(index.saturating_add(1).min(statements.len()));
        let Some(Stmt::Let { name, .. }) = head.last_mut() else {
            continue;
        };
        let rest = Expr::Block {
            statements: rest.to_vec(),
            value: value.cloned().map(Box::new),
            position: None,
        };
        let mut read = std::collections::BTreeSet::new();
        osprey_ast::freevars::free_idents(&rest, &mut read);
        if !read.contains(name.as_str()) {
            "_".clone_into(name);
        }
    }
}

impl Lowerer<'_> {
    pub(super) fn expression_statement(
        &mut self,
        statement: &ExpressionStatement<'_>,
    ) -> Result<Stmt> {
        if let Expression::AssignmentExpression(assignment) = &statement.expression {
            return self.assignment(assignment, statement.span);
        }
        if let Some(root) = receiver_root(&statement.expression) {
            if self.updates(root, statement.span)? {
                let value = self.expr(&statement.expression)?;
                self.bind(root);
                return Ok(self.let_statement(root, value, None, statement.span));
            }
        }
        Ok(Stmt::Expr {
            value: self.expr(&statement.expression)?,
            doc: None,
            position: Some(self.at(statement.span)),
        })
    }

    /// `x = value;` rebinds the local `x`, and `x += value;` rebinds it to
    /// `x + value`. A `const`, a field, an element and a captured variable are
    /// not reassignable. Implements [FLAVOR-TS-FUNCTIONS].
    fn assignment(&mut self, assignment: &AssignmentExpression<'_>, span: Span) -> Result<Stmt> {
        let AssignmentTarget::AssignmentTargetIdentifier(target) = &assignment.left else {
            return self.unsupported(
                assignment.left.span(),
                "assignment to a field or element; build a new value with spread",
            );
        };
        let name = target.name.as_str();
        if self.in_arm() && !self.bound_in_function(name) && !self.constant(name) {
            let value = self.assigned_value(assignment, name, span)?;
            if self.write_state(name) {
                return Ok(Stmt::Assignment {
                    name: name.to_owned(),
                    value,
                    position: Some(self.at(span)),
                });
            }
        }
        if !self.bound_in_function(name) {
            return Err(self.error(
                span,
                format!("`{name}` is not a local of this function; a function cannot reassign a variable it captured"),
            ));
        }
        if self.constant(name) {
            return Err(self.error(
                span,
                format!("`{name}` is a `const`; declare it with `let` to reassign it"),
            ));
        }
        let _ = self.updates(name, span)?;
        let value = self.assigned_value(assignment, name, span)?;
        self.bind(name);
        Ok(self.let_statement(name, value, None, span))
    }

    /// The value `x = value` or `x op= value` gives `x`.
    fn assigned_value(
        &mut self,
        assignment: &AssignmentExpression<'_>,
        name: &str,
        span: Span,
    ) -> Result<Expr> {
        Ok(match assignment.operator.to_binary_operator() {
            None => self.expr(&assignment.right)?,
            Some(operator) => {
                let op = binary_spelling(operator).ok_or_else(|| {
                    self.error(
                        span,
                        "TypeScript compound assignment operator has no Osprey lowering",
                    )
                })?;
                Expr::Binary {
                    left: Box::new(Expr::Identifier(name.to_owned())),
                    op: op.to_owned(),
                    right: Box::new(self.expr(&assignment.right)?),
                    position: Some(self.at(span)),
                }
            }
        })
    }

    /// Whether `root.m(…);` rebinds the local `root` to the call's value, so
    /// later statements see the update. Implements [FLAVOR-TS-RECEIVER].
    fn updates(&self, root: &str, span: Span) -> Result<bool> {
        if !self.bound_in_function(root) {
            return Ok(false);
        }
        if self.captured(root) {
            return Err(self.error(
                span,
                format!("`{root}` is updated after a function captured it; the function would keep the old value"),
            ));
        }
        let returned = self.in_tail() && self.updated_parameter().as_deref() == Some(root);
        if self.bound_here(root) || returned {
            return Ok(true);
        }
        Err(self.error(
            span,
            format!("`{root}` is updated inside a branch, where the update would be lost; update it outside the branch"),
        ))
    }
}
