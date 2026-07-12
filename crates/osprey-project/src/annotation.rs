//! Signature annotation application before shared type checking.

use crate::resolve::{Context, Locals, Resolver};
use osprey_ast::{Expr, SignatureItem, Stmt};

impl Resolver<'_> {
    pub(crate) fn rewrite_exprs(
        &mut self,
        expressions: &mut [Expr],
        context: &Context,
        locals: &mut Locals,
    ) {
        for expression in expressions {
            self.rewrite_expr(expression, context, locals);
        }
    }

    pub(crate) fn apply_annotation(
        &mut self,
        statement: &mut Stmt,
        annotation: &SignatureItem,
        context: &Context,
    ) {
        match (statement, annotation) {
            (Stmt::Let { ty, .. }, SignatureItem::Value { ty: contract, .. }) if ty.is_none() => {
                *ty = Some(contract.clone());
            }
            (
                Stmt::Function {
                    parameters,
                    return_type,
                    effects,
                    position,
                    ..
                },
                SignatureItem::Function {
                    parameters: contracts,
                    return_type: contract_return,
                    effects: contract_effects,
                    ..
                },
            ) => {
                if parameters.len() != contracts.len() {
                    self.error(
                        context.source,
                        *position,
                        "function implementation arity does not match its signature",
                    );
                }
                for (parameter, contract) in parameters.iter_mut().zip(contracts) {
                    if parameter.ty.is_none() {
                        parameter.ty = Some(contract.clone());
                    }
                }
                if return_type.is_none() {
                    *return_type = Some(contract_return.clone());
                }
                if effects.is_empty() {
                    effects.clone_from(contract_effects);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn declaration_name(statement: &Stmt) -> Option<&str> {
    match statement {
        Stmt::Let { name, .. }
        | Stmt::Function { name, .. }
        | Stmt::Extern { name, .. }
        | Stmt::Type { name, .. }
        | Stmt::Effect { name, .. } => Some(name),
        _ => None,
    }
}
