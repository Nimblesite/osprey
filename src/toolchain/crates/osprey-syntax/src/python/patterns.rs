//! Python patterns share Osprey's exhaustive matcher and binding rules.
//! Implements [FLAVOR-PYTHON-EXPRESSIONS], [FLAVOR-PYTHON-TYPING],
//! [FLAVOR-PYTHON-CLASS].
use super::context::{children, position, Lowerer, Result};
use osprey_ast::{Expr, MatchArm, Pattern};
use tree_sitter::Node;

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
            let pattern = lowerer.pattern(Self::first(node)?)?;
            let body = lowerer.suite(Self::field(node, "consequence")?, tail)?;
            Ok(MatchArm {
                pattern,
                body: crate::desugar::positioned(body, Some(position(node))),
            })
        })
    }

    fn pattern(&mut self, node: Node<'_>) -> Result<Pattern> {
        match node.kind() {
            "case_pattern" if self.text(node) == "_" => Ok(Pattern::Wildcard),
            "case_pattern" => self.pattern(Self::first(node)?),
            "dotted_name" | "identifier" => self.capture(node),
            "class_pattern" => self.constructor_pattern(node),
            "list_pattern" => self.list_pattern(node),
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

    fn constructor_pattern(&mut self, node: Node<'_>) -> Result<Pattern> {
        let name = Self::first(node)?;
        let (fields, sub_patterns) = self.constructor_fields(node)?;
        Ok(Pattern::Constructor {
            name: self.text(name).replace('.', "::"),
            fields,
            sub_patterns,
        })
    }

    fn constructor_fields(&mut self, node: Node<'_>) -> Result<(Vec<String>, Vec<Pattern>)> {
        let mut fields = Vec::new();
        let mut sub_patterns = Vec::new();
        for child in children(node).into_iter().skip(1) {
            let inner = Self::first(child)?;
            if inner.kind() == "keyword_pattern" {
                let (field, binder) = self.field_pattern(inner)?;
                if fields.contains(&field) {
                    return Err(Self::error(
                        inner,
                        format!("duplicate pattern field `{field}`"),
                    ));
                }
                fields.push(field);
                sub_patterns.push(binder);
            } else {
                sub_patterns.push(self.pattern(child)?);
            }
        }
        if !fields.is_empty() && fields.len() != sub_patterns.len() {
            return self.unsupported(node);
        }
        if !fields.is_empty()
            && fields.iter().zip(&sub_patterns).all(
                |(field, pattern)| matches!(pattern, Pattern::Binding(binder) if binder == field),
            )
        {
            sub_patterns.clear();
        }
        Ok((fields, sub_patterns))
    }

    /// Preserve labels separately from source binders. [FLAVOR-PYTHON-CLASS]
    fn field_pattern(&mut self, node: Node<'_>) -> Result<(String, Pattern)> {
        let field = Self::first(node)?;
        let binder = node
            .child(node.child_count().saturating_sub(1))
            .ok_or_else(|| Self::error(node, "missing pattern binder"))?;
        let pattern = if self.text(binder) == "_" {
            Pattern::Wildcard
        } else if matches!(binder.kind(), "identifier" | "dotted_name") {
            self.capture(binder)?
        } else {
            return Err(Self::error(
                binder,
                "bind a variant field before matching a nested pattern",
            ));
        };
        Ok((self.text(field).to_owned(), pattern))
    }

    fn list_pattern(&mut self, node: Node<'_>) -> Result<Pattern> {
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
                elements.push(self.pattern(child)?);
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
