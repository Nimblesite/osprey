//! Python patterns share Osprey's exhaustive matcher and binding rules.
//! Implements [FLAVOR-PYTHON-EXPRESSIONS], [FLAVOR-PYTHON-TYPING],
//! [FLAVOR-PYTHON-CLASS].
use super::context::{children, position, Lowerer, Result};
use osprey_ast::{Expr, MatchArm, Pattern, Stmt};
use tree_sitter::Node;

/// A constructor-field renaming: the field `field` is bound to the written
/// name `binder`, which the canonical constructor pattern cannot spell (its
/// named fields bind under their own names). The frontend binds the field
/// under its own name and prepends `let binder = field` to the arm body.
type Renames = Vec<(String, String)>;

impl Lowerer<'_> {
    pub(super) fn matching(&mut self, node: Node<'_>, tail: bool) -> Result<Expr> {
        let value = Box::new(self.expr(Self::field(node, "subject")?)?);
        let arms = children(Self::field(node, "body")?)
            .into_iter()
            .map(|arm| self.match_arm(arm, tail))
            .collect::<Result<_>>()?;
        Ok(Expr::Match { value, arms })
    }

    fn match_arm(&mut self, node: Node<'_>, tail: bool) -> Result<MatchArm> {
        self.reject_field(node, "guard")?;
        self.scoped(false, |lowerer| {
            let mut renames = Vec::new();
            let pattern = lowerer.pattern(Self::first(node)?, &mut renames)?;
            let body = lowerer.suite(Self::field(node, "consequence")?, tail)?;
            Ok(MatchArm {
                pattern,
                body: crate::desugar::positioned(with_renames(body, &renames), Some(position(node))),
            })
        })
    }

    fn pattern(&mut self, node: Node<'_>, renames: &mut Renames) -> Result<Pattern> {
        match node.kind() {
            "case_pattern" if self.text(node) == "_" => Ok(Pattern::Wildcard),
            "case_pattern" => self.pattern(Self::first(node)?, renames),
            "dotted_name" | "identifier" => self.capture(node),
            "class_pattern" => self.constructor_pattern(node, renames),
            "list_pattern" => self.list_pattern(node, renames),
            "true" | "false" | "integer" | "float" | "string" => self.literal_pattern(node),
            _ => self.unsupported(node),
        }
    }

    fn capture(&mut self, node: Node<'_>) -> Result<Pattern> {
        let name = if node.kind() == "dotted_name" {
            Self::first(node)?
        } else {
            node
        };
        if self.text(name) != self.text(node) {
            return self.unsupported(node);
        }
        self.bind(name, false)?;
        Ok(Pattern::Binding(self.text(name).into()))
    }

    fn literal_pattern(&mut self, node: Node<'_>) -> Result<Pattern> {
        let expr = self.expr(node)?;
        if !matches!(
            expr,
            Expr::Integer(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Str(_)
        ) {
            return self.unsupported(node);
        }
        Ok(Pattern::Literal(Box::new(expr)))
    }

    fn constructor_pattern(&mut self, node: Node<'_>, renames: &mut Renames) -> Result<Pattern> {
        let name = Self::first(node)?;
        let (fields, sub_patterns) = self.constructor_fields(node, renames)?;
        Ok(Pattern::Constructor {
            name: self.text(name).replace('.', "::"),
            fields,
            sub_patterns,
        })
    }

    fn constructor_fields(
        &mut self,
        node: Node<'_>,
        renames: &mut Renames,
    ) -> Result<(Vec<String>, Vec<Pattern>)> {
        let mut fields = Vec::new();
        let mut sub_patterns = Vec::new();
        for child in children(node).into_iter().skip(1) {
            let inner = Self::first(child)?;
            if inner.kind() == "keyword_pattern" {
                fields.push(self.field_pattern(inner, renames)?);
            } else {
                sub_patterns.push(self.pattern(child, renames)?);
            }
        }
        if !fields.is_empty() && !sub_patterns.is_empty() {
            return self.unsupported(node);
        }
        Ok((fields, sub_patterns))
    }

    /// A `field = binder` keyword pattern. The canonical constructor pattern
    /// binds each named field under its own name, so a differing binder is a
    /// rename performed in the arm body ([FLAVOR-PYTHON-CLASS]).
    fn field_pattern(&mut self, node: Node<'_>, renames: &mut Renames) -> Result<String> {
        let parts = children(node);
        let [field, binder] = parts.as_slice() else {
            return self.unsupported(node);
        };
        if !matches!(binder.kind(), "identifier" | "dotted_name") {
            return Err(Self::error(
                *binder,
                "a nested pattern cannot destructure a variant field; bind it and match in a second expression",
            ));
        }
        let field_name = self.text(*field).to_owned();
        let binder_name = self.text(*binder).to_owned();
        self.bind(*field, false)?;
        if binder_name != field_name {
            self.bind(*binder, false)?;
            renames.push((field_name.clone(), binder_name));
        }
        Ok(field_name)
    }

    fn list_pattern(&mut self, node: Node<'_>, renames: &mut Renames) -> Result<Pattern> {
        let mut elements = Vec::new();
        let mut rest = None;
        for child in children(node) {
            if rest.is_some() {
                return self.unsupported(child);
            }
            let inner = Self::first(child).unwrap_or(child);
            if inner.kind() == "splat_pattern" {
                let name = Self::first(inner)?;
                self.bind(name, false)?;
                rest = Some(self.text(name).into());
            } else {
                elements.push(self.pattern(child, renames)?);
            }
        }
        Ok(Pattern::List { elements, rest })
    }

    pub(super) fn validate_returns(node: Node<'_>) -> Result<()> {
        if contains_return(node) && !returns(node) {
            return Err(Self::error(
                node,
                "return must terminate every branch of a tail suite",
            ));
        }
        Ok(())
    }
}

/// Prepends `let binder = field` statements for each renamed field so the arm
/// body sees the binder the source wrote.
fn with_renames(body: Expr, renames: &[(String, String)]) -> Expr {
    if renames.is_empty() {
        return body;
    }
    let lets = renames
        .iter()
        .map(|(field, binder)| Stmt::Let {
            name: binder.clone(),
            mutable: false,
            ty: None,
            value: Expr::Identifier(field.clone()),
            doc: None,
            position: None,
        })
        .collect();
    prepend_statements(lets, body)
}

/// `body` as a block that runs `statements` first, merging into an existing block.
fn prepend_statements(mut statements: Vec<Stmt>, body: Expr) -> Expr {
    match body {
        Expr::Block {
            statements: mut rest,
            value,
            position,
        } => {
            statements.append(&mut rest);
            Expr::Block {
                statements,
                value,
                position,
            }
        }
        other => Expr::Block {
            statements,
            value: Some(Box::new(other)),
            position: None,
        },
    }
}

fn contains_return(node: Node<'_>) -> bool {
    match node.kind() {
        "return_statement" => true,
        "function_definition" | "class_definition" | "lambda" => false,
        _ => children(node).into_iter().any(contains_return),
    }
}

fn returns(node: Node<'_>) -> bool {
    match node.kind() {
        "return_statement" => true,
        "block" => children(node).last().is_some_and(|node| returns(*node)),
        "with_statement" | "else_clause" => node.child_by_field_name("body").is_some_and(returns),
        "case_clause" => node.child_by_field_name("consequence").is_some_and(returns),
        "match_statement" => node
            .child_by_field_name("body")
            .is_some_and(|body| children(body).into_iter().all(returns)),
        "if_statement" | "elif_clause" => returns_if(node),
        _ => false,
    }
}

fn returns_if(node: Node<'_>) -> bool {
    let branches = children(node)
        .into_iter()
        .filter(|n| matches!(n.kind(), "elif_clause" | "else_clause"))
        .collect::<Vec<_>>();
    let exhaustive =
        node.kind() == "elif_clause" || branches.last().is_some_and(|n| n.kind() == "else_clause");
    exhaustive
        && node.child_by_field_name("consequence").is_some_and(returns)
        && branches.into_iter().all(returns)
}
