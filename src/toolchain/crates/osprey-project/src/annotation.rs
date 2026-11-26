//! Signature annotation application before shared type checking.

use crate::collect_support::Diagnostics;
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
                *ty = Some(contract.as_contract_annotation());
            }
            (
                Stmt::Function {
                    type_params,
                    parameters,
                    return_type,
                    effects,
                    effect_tail,
                    effect_row_present,
                    position,
                    ..
                },
                SignatureItem::Function {
                    type_params: contract_params,
                    parameters: contracts,
                    return_type: contract_return,
                    effects: contract_effects,
                    effect_tail: contract_tail,
                    effect_row_present: contract_row_present,
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
                // An inferred implementation takes the signature's binders with
                // its types, so `A` in `pick<A, B> : (A, B) -> B` is the
                // function's own parameter, generalized at each use, and not an
                // unknown rigid name ([MODULES-SIGNATURE]).
                if type_params.is_empty() {
                    type_params.clone_from(contract_params);
                }
                for (parameter, contract) in parameters.iter_mut().zip(contracts) {
                    if parameter.ty.is_none() {
                        parameter.ty = Some(contract.as_contract_annotation());
                    }
                }
                if return_type.is_none() {
                    *return_type = Some(contract_return.as_contract_annotation());
                }
                if !*effect_row_present && *contract_row_present {
                    effects.clone_from(contract_effects);
                    effect_tail.clone_from(contract_tail);
                    *effect_row_present = true;
                }
            }
            _ => {}
        }
    }
}

pub(crate) use osprey_ast::stmt_meta::declaration_name;
