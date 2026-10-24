//! List comprehensions lower to the existing iterator pipeline; nested `for`
//! clauses lower to nested folds preserving evaluation order.
//! Implements [FLAVOR-PYTHON-EXPRESSIONS].
use super::context::{apply, children, position, Lowerer, Result};
use osprey_ast::{Expr, Parameter};
use tree_sitter::Node;

impl Lowerer<'_> {
    /// `[f(x) for x in items if p(x)]` lowers to the existing iterator
    /// map/filter/toList pipeline; nested `for` clauses lower to nested folds
    /// preserving evaluation order ([FLAVOR-PYTHON-EXPRESSIONS]).
    pub(super) fn list_comprehension(&mut self, node: Node<'_>) -> Result<Expr> {
        self.scoped(true, |lowerer| {
            let body = lowerer.expr(Lowerer::field(node, "body")?)?;
            let clauses: Vec<Node<'_>> = children(node).into_iter().skip(1).collect();
            lowerer.comprehension(node, body, &clauses)
        })
    }

    fn comprehension(&mut self, node: Node<'_>, body: Expr, clauses: &[Node<'_>]) -> Result<Expr> {
        let Some((first, rest)) = clauses.split_first() else {
            return self.unsupported(node);
        };
        if first.kind() == "for_in_clause" && rest.iter().all(|c| c.kind() == "if_clause") {
            return self.pipeline_comprehension(*first, rest, body);
        }
        self.fold_comprehension(node, clauses, body)
    }

    /// A single `for` clause followed only by `if` filters: the canonical
    /// `toList(map(filter(items, pred), mapper))` pipeline.
    fn pipeline_comprehension(
        &mut self,
        first: Node<'_>,
        filters: &[Node<'_>],
        body: Expr,
    ) -> Result<Expr> {
        let (name, iter) = self.for_in_clause(first)?;
        let mut source = iter;
        for filter in filters {
            let condition = self.expr(Self::first(*filter)?)?;
            source = apply(
                Expr::Identifier("filter".into()),
                vec![source, callback(&name, condition, *filter)],
            );
        }
        let mapped = apply(
            Expr::Identifier("map".into()),
            vec![source, callback(&name, body, first)],
        );
        Ok(apply(Expr::Identifier("toList".into()), vec![mapped]))
    }

    /// Multiple `for` clauses (or filters interleaved with them): nested folds
    /// thread one accumulator list through every clause.
    fn fold_comprehension(
        &mut self,
        node: Node<'_>,
        clauses: &[Node<'_>],
        body: Expr,
    ) -> Result<Expr> {
        self.comprehension_from(
            node,
            clauses,
            body,
            Expr::List(Vec::new(), Some(position(node))),
            0,
        )
    }

    fn comprehension_from(
        &mut self,
        owner: Node<'_>,
        clauses: &[Node<'_>],
        body: Expr,
        seed: Expr,
        index: usize,
    ) -> Result<Expr> {
        let Some(clause) = clauses.get(index) else {
            return self.append_seed(owner, body, seed);
        };
        match clause.kind() {
            "for_in_clause" => self.fold_clause(owner, clauses, body, seed, index, *clause),
            "if_clause" => self.filter_clause(owner, clauses, body, &seed, index, clause),
            _ => self.unsupported(*clause),
        }
    }

    fn append_seed(&self, owner: Node<'_>, body: Expr, seed: Expr) -> Result<Expr> {
        let Expr::Identifier(acc) = seed else {
            return self.unsupported(owner);
        };
        Ok(apply(
            Expr::Identifier("listAppend".into()),
            vec![Expr::Identifier(acc), body],
        ))
    }

    fn fold_clause(
        &mut self,
        owner: Node<'_>,
        clauses: &[Node<'_>],
        body: Expr,
        seed: Expr,
        index: usize,
        clause: Node<'_>,
    ) -> Result<Expr> {
        let (name, iter) = self.for_in_clause(clause)?;
        let acc = osprey_ast::generated_name("acc", index);
        let inner = self.comprehension_from(
            owner,
            clauses,
            body,
            Expr::Identifier(acc.clone()),
            index + 1,
        )?;
        Ok(apply(
            Expr::Identifier("fold".into()),
            vec![iter, seed, callback_two(&acc, &name, inner, clause)],
        ))
    }

    fn filter_clause(
        &mut self,
        owner: Node<'_>,
        clauses: &[Node<'_>],
        body: Expr,
        seed: &Expr,
        index: usize,
        clause: &Node<'_>,
    ) -> Result<Expr> {
        let condition = self.expr(Self::first(*clause)?)?;
        let then = self.comprehension_from(owner, clauses, body, seed.clone(), index + 1)?;
        let Expr::Identifier(acc) = seed else {
            return self.unsupported(*clause);
        };
        Ok(crate::desugar::bool_match(
            condition,
            then,
            Expr::Identifier(acc.clone()),
        ))
    }

    fn for_in_clause(&mut self, node: Node<'_>) -> Result<(String, Expr)> {
        let name = self.loop_target(Self::field(node, "left")?)?;
        let iter = self.expr(Self::field(node, "right")?)?;
        Ok((name, iter))
    }

    pub(super) fn loop_target(&self, node: Node<'_>) -> Result<String> {
        if node.kind() != "identifier" {
            return Err(Self::error(
                node,
                "tuple targets in `for` have no Osprey lowering; iterate one binding at a time",
            ));
        }
        Ok(self.text(node).to_owned())
    }
}

fn callback(name: &str, body: Expr, owner: Node<'_>) -> Expr {
    Expr::Lambda {
        parameters: vec![Parameter {
            name: name.into(),
            ty: None,
            inline_constraint: false,
        }],
        return_type: None,
        body: Box::new(body),
        position: Some(position(owner)),
    }
}

fn callback_two(first: &str, second: &str, body: Expr, owner: Node<'_>) -> Expr {
    Expr::Lambda {
        parameters: vec![
            Parameter {
                name: first.into(),
                ty: None,
                inline_constraint: false,
            },
            Parameter {
                name: second.into(),
                ty: None,
                inline_constraint: false,
            },
        ],
        return_type: None,
        body: Box::new(body),
        position: Some(position(owner)),
    }
}
