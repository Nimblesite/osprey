//! Implements [FLAVOR-PYTHON-IF-JOIN] (#433): a name every branch of an `if`
//! binds is bound after it, so `if`/`else` that assigns `result` in each branch
//! lowers to one `let result = <if value>`.
use super::context::{children, position, Lowerer, Result};
use osprey_ast::{Expr, FieldAssignment, Stmt};
use std::collections::BTreeMap;
use tree_sitter::Node;

impl Lowerer<'_> {
    /// The fresh names every branch of `node` binds, in first-branch order,
    /// each as the node that names it in the first branch.
    pub(super) fn joined_names<'t>(&self, node: Node<'t>) -> Vec<Node<'t>> {
        let suites = branch_suites(node);
        let mut binds = suites.iter().map(|suite| self.suite_binds(*suite));
        let first = binds.next().unwrap_or_default();
        let rest: Vec<Vec<Node<'t>>> = binds.collect();
        if rest.is_empty() {
            return Vec::new();
        }
        first
            .into_iter()
            .filter(|name| rest.iter().all(|other| self.names_in(other, *name)))
            .collect()
    }

    fn names_in(&self, names: &[Node<'_>], name: Node<'_>) -> bool {
        names
            .iter()
            .any(|other| self.text(*other) == self.text(name))
    }

    fn suite_binds<'t>(&self, suite: Node<'t>) -> Vec<Node<'t>> {
        self.body_nodes(suite)
            .into_iter()
            .flat_map(|statement| match statement.kind() {
                "if_statement" => self.joined_names(statement),
                "expression_statement" => assigned_name(statement)
                    .filter(|name| self.assignment_kind(self.text(*name)).is_none())
                    .into_iter()
                    .collect(),
                _ => Vec::new(),
            })
            .collect()
    }

    /// Writes an `if` makes to a joined name count once, as its one binding.
    pub(super) fn count_joined_writes(&self, node: Node<'_>, counts: &mut BTreeMap<String, usize>) {
        let extra = branch_suites(node).len().saturating_sub(1);
        for name in self.joined_names(node) {
            if let Some(count) = counts.get_mut(self.text(name)) {
                *count = count.saturating_sub(extra).max(1);
            }
        }
    }

    /// The statements binding every joined name of the `if` at `node`.
    pub(super) fn joined_if(&mut self, node: Node<'_>, names: &[Node<'_>]) -> Result<Vec<Stmt>> {
        let joined = joined_value(names.iter().map(|name| self.text(*name)));
        let value = self.if_expression_joined(node, &joined)?;
        let [name] = names else {
            return self.joined_fields(node, names, value);
        };
        Ok(vec![self.joined_binding(*name, value)?])
    }

    fn joined_fields(
        &mut self,
        node: Node<'_>,
        names: &[Node<'_>],
        value: Expr,
    ) -> Result<Vec<Stmt>> {
        let record = self.fresh("join");
        let mut statements = vec![Stmt::Let {
            name: record.clone(),
            mutable: false,
            ty: None,
            value,
            doc: None,
            position: Some(position(node)),
        }];
        for name in names {
            let field = Expr::FieldAccess {
                target: Box::new(Expr::Identifier(record.clone())),
                field: self.text(*name).into(),
            };
            statements.push(self.joined_binding(*name, field)?);
        }
        Ok(statements)
    }

    fn joined_binding(&mut self, name: Node<'_>, value: Expr) -> Result<Stmt> {
        let mutable = self.will_be_assigned(self.text(name));
        self.bind(name, mutable)?;
        self.binding(name, name, crate::BindingKind::Variable);
        Ok(self.let_statement(name, value, mutable, None))
    }
}

/// The suites of an `if` with an `else`, one per branch; empty without one,
/// since a branch that is never taken binds nothing.
fn branch_suites(node: Node<'_>) -> Vec<Node<'_>> {
    let clauses = children(node);
    if !clauses.iter().any(|clause| clause.kind() == "else_clause") {
        return Vec::new();
    }
    let first = node.child_by_field_name("consequence");
    let rest = clauses.iter().filter_map(|clause| match clause.kind() {
        "elif_clause" => clause.child_by_field_name("consequence"),
        "else_clause" => clause.child_by_field_name("body"),
        _ => None,
    });
    first.into_iter().chain(rest).collect()
}

fn assigned_name(statement: Node<'_>) -> Option<Node<'_>> {
    children(statement)
        .into_iter()
        .next()
        .filter(|node| node.kind() == "assignment")
        .and_then(|assignment| assignment.child_by_field_name("left"))
        .filter(|left| left.kind() == "identifier")
}

/// What each branch ends with: the one joined name, or a record of them all.
fn joined_value<'a>(names: impl Iterator<Item = &'a str>) -> Expr {
    let mut fields: Vec<FieldAssignment> = names
        .map(|name| FieldAssignment {
            name: name.into(),
            value: Expr::Identifier(name.into()),
        })
        .collect();
    match fields.len() {
        1 => fields.remove(0).value,
        _ => Expr::Object(fields),
    }
}

/// A branch's lowered suite, ending in `value`.
pub(super) fn ending_with(suite: Expr, value: &Expr) -> Expr {
    let tail = Some(Box::new(value.clone()));
    match suite {
        Expr::Block {
            statements,
            value: None,
            position,
        } => Expr::Block {
            statements,
            value: tail,
            position,
        },
        other => Expr::Block {
            statements: vec![Stmt::Expr {
                value: other,
                doc: None,
                position: None,
            }],
            value: tail,
            position: None,
        },
    }
}
