//! Rebase every executable and annotation coordinate through decoded strings.
use super::FragmentMap;
use crate::Flavor;
use osprey_ast::{mutate::children_mut, Expr, Parameter, Pattern, Position, Stmt, TypeExpr};

pub(super) fn rebase_expr(expression: &mut Expr, map: &FragmentMap<'_>, flavor: Flavor) {
    match expression {
        Expr::List(_, position) | Expr::Perform { position, .. } => slot(position, map, flavor),
        Expr::TypeApply {
            position,
            type_args,
            ..
        } => {
            slot(position, map, flavor);
            types(type_args, map, flavor);
        }
        Expr::Lambda {
            position,
            parameters,
            return_type,
            ..
        } => {
            slot(position, map, flavor);
            signature(parameters, return_type, map, flavor);
        }
        Expr::TypeConstructor { type_args, .. } => types(type_args, map, flavor),
        Expr::Block { statements, .. } => statements
            .iter_mut()
            .for_each(|statement| rebase_stmt(statement, map, flavor)),
        Expr::Handler { position, arms, .. } => {
            slot(position, map, flavor);
            for arm in arms {
                slot(&mut arm.position, map, flavor);
            }
        }
        Expr::Match { arms, .. } | Expr::Select { arms } => arms
            .iter_mut()
            .for_each(|arm| pattern(&mut arm.pattern, map, flavor)),
        _ => {}
    }
    children_mut(expression, &mut |child| rebase_expr(child, map, flavor));
}

fn slot(position: &mut Option<Position>, map: &FragmentMap<'_>, flavor: Flavor) {
    if let Some(inner) = position.filter(|position| position.line > 0) {
        *position = map.map_position(inner, super::fragment_binding(flavor).len(), flavor);
    }
}

fn signature(
    parameters: &mut [Parameter],
    result: &mut Option<TypeExpr>,
    map: &FragmentMap<'_>,
    flavor: Flavor,
) {
    for parameter in parameters {
        optional_type(&mut parameter.ty, map, flavor);
    }
    optional_type(result, map, flavor);
}

fn optional_type(ty: &mut Option<TypeExpr>, map: &FragmentMap<'_>, flavor: Flavor) {
    if let Some(ty) = ty {
        ty_expr(ty, map, flavor);
    }
}

fn types(tys: &mut [TypeExpr], map: &FragmentMap<'_>, flavor: Flavor) {
    for ty in tys {
        ty_expr(ty, map, flavor);
    }
}

fn ty_expr(ty: &mut TypeExpr, map: &FragmentMap<'_>, flavor: Flavor) {
    slot(&mut ty.position, map, flavor);
    types(&mut ty.generic_params, map, flavor);
    types(&mut ty.parameter_types, map, flavor);
    for nested in [&mut ty.array_element, &mut ty.return_type]
        .into_iter()
        .flatten()
    {
        ty_expr(nested, map, flavor);
    }
}

fn pattern(pat: &mut Pattern, map: &FragmentMap<'_>, flavor: Flavor) {
    match pat {
        Pattern::TypeAnnotated { ty, .. } => ty_expr(ty, map, flavor),
        Pattern::Constructor { sub_patterns, .. }
        | Pattern::List {
            elements: sub_patterns,
            ..
        } => {
            for pat in sub_patterns {
                pattern(pat, map, flavor);
            }
        }
        Pattern::Literal(expression) => rebase_expr(expression, map, flavor),
        _ => {}
    }
}

fn rebase_stmt(statement: &mut Stmt, map: &FragmentMap<'_>, flavor: Flavor) {
    match statement {
        Stmt::Namespace { position, .. }
        | Stmt::Let { position, .. }
        | Stmt::Assignment { position, .. }
        | Stmt::Function { position, .. }
        | Stmt::Extern { position, .. }
        | Stmt::Type { position, .. }
        | Stmt::Effect { position, .. }
        | Stmt::Module { position, .. }
        | Stmt::Signature { position, .. }
        | Stmt::Expr { position, .. } => slot(position, map, flavor),
        Stmt::Import(_) => {}
    }
    statement_annotations(statement, map, flavor);
}

fn statement_annotations(statement: &mut Stmt, map: &FragmentMap<'_>, flavor: Flavor) {
    match statement {
        Stmt::Let { ty, .. } => optional_type(ty, map, flavor),
        Stmt::Function {
            parameters,
            return_type,
            effects,
            ..
        } => {
            signature(parameters, return_type, map, flavor);
            for effect in effects {
                slot(&mut effect.position, map, flavor);
                types(&mut effect.type_args, map, flavor);
            }
        }
        Stmt::Extern {
            parameters,
            return_type,
            ..
        } => {
            for parameter in parameters {
                ty_expr(&mut parameter.ty, map, flavor);
            }
            optional_type(return_type, map, flavor);
        }
        Stmt::Type { alias, .. } => optional_type(alias, map, flavor),
        Stmt::Namespace { body, .. } => body
            .iter_mut()
            .for_each(|statement| rebase_stmt(statement, map, flavor)),
        Stmt::Module { body, .. } => body
            .iter_mut()
            .for_each(|item| rebase_stmt(&mut item.declaration, map, flavor)),
        _ => {}
    }
}
