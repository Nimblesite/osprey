//! How a callable's own callback obligations resolve at the call that passes
//! it. A handler value's action — or any callback a callee applies with its
//! own argument — is supplied inside that callee's body, so its latent
//! requests are judged on what the callee actually passes there
//! ([MULTI-HANDLE-MANY-VALUE]).

use super::escape::Analyzer;
use super::facts::{Callee, Esc, Scope};
use osprey_ast::{AstNode, Expr, Stmt};
use std::collections::BTreeSet;

impl Analyzer<'_> {
    /// The captured effect `argument` answers when the argument is a handler
    /// value, named or written inline ([EFFECTS-HANDLER-VALUE]).
    pub(super) fn handler_effect(&self, argument: &Expr, scope: &Scope) -> Option<String> {
        let effect = match self.callee(argument, scope) {
            Callee::Function(resolved) => self
                .table
                .handler_action(resolved)
                .map(|(_, effect)| effect),
            _ => super::table::handler_value(argument),
        }?;
        self.facts.is_captured(&effect).then_some(effect)
    }

    /// `latent` as it leaves the call passing `argument` into function `id`:
    /// an argument that is the caller's own parameter forwards its entries
    /// unchanged, and any other argument's entries resolve against the
    /// arguments `id`'s body applies.
    pub(super) fn applied_latent(
        &mut self,
        id: usize,
        argument: &Expr,
        latent: Esc,
        hidden: &BTreeSet<String>,
        scope: &Scope,
    ) -> Esc {
        if matches!(self.callee(argument, scope), Callee::Param(_)) {
            return latent.behind(hidden);
        }
        let mut out = Esc {
            fixed: latent.fixed.clone(),
            params: std::collections::BTreeMap::new(),
        };
        out.fixed.retain(|effect| !hidden.contains(effect));
        for (entry, answered) in latent.params {
            let mut merged = answered;
            merged.extend(hidden.iter().cloned());
            out.absorb(self.param_latents(id, entry).behind(&merged));
        }
        out
    }

    /// The requests the arguments `id`'s body applies to its parameter
    /// `entry` can leave unanswered.
    fn param_latents(&mut self, id: usize, entry: usize) -> Esc {
        let Some(func) = self.table.func(id) else {
            return Esc::default();
        };
        let scope = Scope::of_function(self.table, id);
        let mut out = Esc::default();
        self.walk_calls(func.body, &scope, entry, &mut out);
        out
    }

    /// Every application of parameter `entry` under `expression`, in the
    /// scopes the escape analysis itself resolves names by.
    fn walk_calls(&mut self, expression: &Expr, scope: &Scope, entry: usize, out: &mut Esc) {
        match expression {
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => {
                if matches!(self.callee(function, scope), Callee::Param(seen) if seen == entry) {
                    let passed = arguments
                        .iter()
                        .chain(named_arguments.iter().map(|named| &named.value));
                    for argument in passed {
                        out.absorb(self.latent(argument, scope).unwrap_or_default());
                    }
                }
                self.walk_children(expression, scope, entry, out);
            }
            Expr::Lambda {
                parameters, body, ..
            } => {
                let names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
                let inner = match self.table.lambda(expression) {
                    Some(id) => scope.entering(id, &names),
                    None => scope.binding(names),
                };
                self.walk_calls(body, &inner, entry, out);
            }
            Expr::Block {
                statements, value, ..
            } => {
                let mut scope = scope.clone();
                for statement in statements {
                    self.walk_statement(statement, &mut scope, entry, out);
                }
                if let Some(value) = value {
                    self.walk_calls(value, &scope, entry, out);
                }
            }
            Expr::Match { value, arms } => {
                self.walk_calls(value, scope, entry, out);
                for arm in arms {
                    let inner = scope.binding(osprey_ast::freevars::pattern_bindings(&arm.pattern));
                    self.walk_calls(&arm.body, &inner, entry, out);
                }
            }
            Expr::Handler {
                arms,
                body,
                return_clause,
                finally,
                ..
            } => {
                self.walk_calls(body, scope, entry, out);
                for arm in arms {
                    let inner = scope.binding(arm.params.iter().cloned());
                    self.walk_calls(&arm.body, &inner, entry, out);
                }
                for clause in return_clause.iter().chain(finally.iter()) {
                    self.walk_calls(clause, scope, entry, out);
                }
            }
            _ => self.walk_children(expression, scope, entry, out),
        }
    }

    fn walk_statement(&mut self, statement: &Stmt, scope: &mut Scope, entry: usize, out: &mut Esc) {
        match statement {
            Stmt::Let { name, value, .. } => {
                self.walk_calls(value, scope, entry, out);
                *scope = self.bound(scope, name, value);
            }
            Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
                self.walk_calls(value, scope, entry, out);
            }
            _ => {}
        }
    }

    fn walk_children(&mut self, expression: &Expr, scope: &Scope, entry: usize, out: &mut Esc) {
        let mut kids = Vec::new();
        AstNode::Expression(expression).for_each_child(|child| kids.push(child));
        for kid in kids {
            if let AstNode::Expression(child) = kid {
                self.walk_calls(child, scope, entry, out);
            }
        }
    }
}
