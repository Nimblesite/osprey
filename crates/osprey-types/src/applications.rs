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
use std::collections::{HashMap, HashSet};

type Bindings = HashMap<VarId, Type>;

/// The instantiations every recorded use of a generalized binding AGREES on,
/// for the variables a published site type still leaves open.
///
/// A `let` quantifies its unconstrained variables, so a site type inside its
/// initializer — a lambda's ABI above all — keeps the quantified variable
/// while every later use instantiates it fresh. One agreeing, concrete
/// instantiation across all uses is the ABI the one evaluated value actually
/// has, so it resolves the variable in the published tables; uses that
/// disagree leave it open and the backend still rejects the closure loudly.
/// Implements [TYPE-GENERICS-APPLY] (docs/specs/language/0004-TypeSystem.md).
pub(crate) fn alias_resolutions(
    sites: &HashMap<usize, Bindings>,
    candidates: &HashSet<VarId>,
    ctx: &mut InferCtx,
) -> Bindings {
    let mut agreed: HashMap<VarId, Option<Type>> = HashMap::new();
    for bindings in sites.values() {
        record_agreements(bindings, candidates, ctx, &mut agreed);
    }
    agreed
        .into_iter()
        .filter_map(|(var, ty)| ty.map(|ty| (var, ty)))
        .collect()
}

/// Fold one origin's bindings into `agreed`, marking a variable `None` when
/// a use instantiates it at a different type, or at a type that is itself
/// still generic: a use inside a generic function is a different type at
/// each of ITS instantiations, so it can never agree with a concrete one.
fn record_agreements(
    bindings: &Bindings,
    candidates: &HashSet<VarId>,
    ctx: &mut InferCtx,
    agreed: &mut HashMap<VarId, Option<Type>>,
) {
    for (var, ty) in bindings {
        if candidates.contains(var) {
            let ty = Some(ctx.apply(ty)).filter(|ty| !crate::has_type_var(ty));
            let candidate = agreed.entry(*var).or_insert_with(|| ty.clone());
            if *candidate != ty {
                *candidate = None;
            }
        }
    }
}

/// Every origin binding, filed under each variable its type mentions. Growing
/// a site's bindings can only rewrite the origin bindings that mention one of
/// them, so composing through this index visits those alone instead of every
/// binding of every site.
pub(crate) struct Mentions<'a>(HashMap<VarId, Vec<(usize, VarId, &'a Type)>>);

impl<'a> Mentions<'a> {
    pub(crate) fn of(origins: &'a HashMap<usize, Bindings>) -> Self {
        let mut index: HashMap<VarId, Vec<(usize, VarId, &'a Type)>> = HashMap::new();
        for (site, origin) in origins {
            for (var, ty) in origin {
                let mut mentioned = HashSet::new();
                let _ = crate::ty::map_type_vars(ty, &mut |v| {
                    let _ = mentioned.insert(v);
                    Type::Var(v)
                });
                for v in mentioned {
                    index.entry(v).or_default().push((*site, *var, ty));
                }
            }
        }
        Mentions(index)
    }
}

/// Compose origins through let aliases and generic wrapper parameters.
/// A source variable with competing replacements remains unspecialized.
pub(crate) fn expanded(bindings: &Bindings, mentions: &Mentions<'_>) -> Bindings {
    let mut result = bindings.clone();
    loop {
        let candidates = replacements(&result, mentions);
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

/// What `result` rewrites each still-unbound origin variable to, or `None`
/// when two origins disagree. An origin binding mentioning none of `result`'s
/// variables is unchanged by it, so only the indexed ones are tried.
fn replacements(result: &Bindings, mentions: &Mentions<'_>) -> HashMap<VarId, Option<Type>> {
    let mut tried = HashSet::new();
    let mut candidates: HashMap<VarId, Option<Type>> = HashMap::new();
    for key in result.keys() {
        for &(site, var, ty) in mentions.0.get(key).into_iter().flatten() {
            if result.contains_key(&var) || !tried.insert((site, var)) {
                continue;
            }
            let replacement = crate::env::subst_vars(ty, result);
            if replacement == *ty {
                continue;
            }
            let _ = candidates
                .entry(var)
                .and_modify(|candidate| {
                    if candidate.as_ref() != Some(&replacement) {
                        *candidate = None;
                    }
                })
                .or_insert(Some(replacement));
        }
    }
    candidates
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
    let mentions = Mentions::of(&origins);
    collector
        .calls
        .into_iter()
        .map(|(site, bindings)| (site, expanded(&bindings, &mentions)))
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
    let _ = types
        .applications
        .insert((0, column), std::sync::Arc::new(bindings));
    *function = Expr::TypeApply {
        function: Box::new(function.clone()),
        type_args: Vec::new(),
        position: Some(Position { line: 0, column }),
    };
}
