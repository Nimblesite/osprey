//! Retain inferred call substitutions across backend AST cloning.
//!
//! Implements [TYPE-GENERICS-APPLY] for implicit and explicit applications.
//! Expression ordinals are local to the unchanged source tree; the backend
//! copy carries them in synthetic application positions outside source lines.

use crate::ctx::InferCtx;
use crate::info::ProgramTypes;
use crate::ty::{Type, VarId};
use osprey_ast::{walk_program, AstVisitor};
use osprey_ast::{Expr, Position, Program};
use std::collections::HashMap;

type Bindings = HashMap<VarId, Type>;

/// Compose origins through let aliases and generic wrapper parameters.
/// A source variable with competing replacements remains unspecialized.
pub(crate) fn expanded(bindings: &Bindings, origins: &HashMap<usize, Bindings>) -> Bindings {
    let mut result = bindings.clone();
    loop {
        let mut candidates: HashMap<VarId, Option<Type>> = HashMap::new();
        for origin in origins.values() {
            for (var, ty) in origin {
                if result.contains_key(var) {
                    continue;
                }
                let replacement = crate::env::subst_vars(ty, &result);
                if replacement == *ty {
                    continue;
                }
                let _ = candidates
                    .entry(*var)
                    .and_modify(|candidate| {
                        if candidate.as_ref() != Some(&replacement) {
                            *candidate = None;
                        }
                    })
                    .or_insert(Some(replacement));
            }
        }
        let before = result.len();
        result.extend(
            candidates
                .into_iter()
                .filter_map(|(var, ty)| ty.map(|ty| (var, ty))),
        );
        if before == result.len() {
            return result;
        }
    }
}

pub(crate) fn collect(
    program: &Program,
    sites: &HashMap<usize, Bindings>,
    ctx: &mut InferCtx,
) -> HashMap<usize, Bindings> {
    struct Collector<'a> {
        sites: &'a HashMap<usize, Bindings>,
        ctx: &'a mut InferCtx,
        index: usize,
        calls: HashMap<usize, Bindings>,
    }
    impl AstVisitor for Collector<'_> {
        fn expression(&mut self, expression: &Expr) {
            let site = match expression {
                Expr::Call { function, .. } => Some(function.as_ref()),
                Expr::Pipe { right, .. } => Some(right.as_ref()),
                Expr::MethodCall { .. } | Expr::Identifier(_) | Expr::Path(_) => Some(expression),
                _ => None,
            };
            if let Some(bindings) = site
                .filter(|e| {
                    matches!(
                        e,
                        Expr::Identifier(_) | Expr::Path(_) | Expr::MethodCall { .. }
                    )
                })
                .and_then(|e| self.sites.get(&std::ptr::from_ref(e).addr()))
                .filter(|b| !b.is_empty())
            {
                let bindings: Bindings = bindings
                    .iter()
                    .map(|(v, t)| (*v, self.ctx.apply(t)))
                    .collect();
                let standalone = matches!(expression, Expr::Identifier(_) | Expr::Path(_));
                if !standalone || bindings.values().all(|ty| !crate::has_type_var(ty)) {
                    let _ = self.calls.insert(self.index, bindings);
                }
            }
            self.index += 1;
        }
    }
    let mut collector = Collector {
        sites,
        ctx,
        index: 0,
        calls: HashMap::new(),
    };
    walk_program(program, &mut collector);
    let origins: HashMap<_, _> = sites
        .iter()
        .map(|(site, bindings)| {
            (
                *site,
                bindings
                    .iter()
                    .map(|(var, ty)| (*var, collector.ctx.apply(ty)))
                    .collect(),
            )
        })
        .collect();
    collector
        .calls
        .into_iter()
        .map(|(site, bindings)| (site, expanded(&bindings, &origins)))
        .collect()
}

pub(crate) fn elaborate(program: &Program, types: &mut ProgramTypes) -> Program {
    let mut result = program.clone();
    let mut index = 0;
    for statement in &mut result.statements {
        osprey_ast::mutate::statement_children_mut(statement, &mut |e| {
            elaborate_expr(e, &mut index, types);
        });
    }
    result
}

fn elaborate_expr(expression: &mut Expr, index: &mut usize, types: &mut ProgramTypes) {
    let current = *index;
    *index += 1;
    osprey_ast::mutate::children_mut(expression, &mut |child| elaborate_expr(child, index, types));
    if let Some(target) = types.methods.get(&current) {
        crate::methods::lower(expression, target);
    }
    let Some(bindings) = types.call_bindings.get(&current).cloned() else {
        return;
    };
    let Ok(column) = u32::try_from(current) else {
        return;
    };
    let function = if matches!(expression, Expr::Identifier(_) | Expr::Path(_))
        || matches!(
            types.methods.get(&current),
            Some(crate::methods::Target::Deferred(_))
        ) {
        expression
    } else {
        match expression {
            Expr::Call { function, .. } => function.as_mut(),
            Expr::Pipe { right, .. } => right.as_mut(),
            _ => return,
        }
    };
    let _ = types.applications.insert((0, column), bindings);
    *function = Expr::TypeApply {
        function: Box::new(function.clone()),
        type_args: Vec::new(),
        position: Some(Position { line: 0, column }),
    };
}
