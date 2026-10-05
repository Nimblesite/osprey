//! Proven removal of forwarding thunks. Implements [TYPE-WARNINGS-CALLBACKS].
//!
//! Calling a named function must still happen after its handler is installed.
//! Passing that function directly preserves this delay. Only an otherwise
//! empty zero-argument wrapper passed directly to a proved invoker is eligible.

use std::collections::HashSet;

use osprey_ast::{walk_program, AstVisitor, Expr, Position, Program, Stmt};

use crate::pattern::pattern_binder_names;
use crate::redundant::{preserves_callbacks, published, Snapshot};
use crate::TypeWarning;

#[path = "callback_proof.rs"]
mod proof;

/// Shared rule identifier for compiler and editor diagnostics.
pub const REDUNDANT_CALLBACK: &str = "redundant-callback";

struct Site {
    name: String,
    position: Position,
}

#[derive(Default)]
struct Candidates {
    names: HashSet<String>,
    definitions: HashSet<String>,
    shadowed: HashSet<String>,
    invokers: HashSet<String>,
    bindings: HashSet<String>,
    ambiguous: HashSet<String>,
    sites: Vec<(Site, Option<String>)>,
}

/// Warn only when replacing forwarding wrappers jointly preserves every
/// remaining inferred type, method choice and checked effect requirement.
/// Consumers must be immutable handler values or named functions whose body
/// only invokes their sole callback parameter. Stored wrappers, local callees,
/// ambiguous names and generic applications are left alone.
#[must_use]
pub fn redundant_callbacks(program: &Program) -> Vec<TypeWarning> {
    let sites = candidates(program);
    if sites.is_empty() {
        return Vec::new();
    }
    let Some(baseline) = published(program) else {
        return Vec::new();
    };
    let mut removed = Vec::new();
    sites
        .into_iter()
        .filter(|site| proof::exact_callback(&baseline, &site.name, site.position))
        .filter_map(|site| proven(program, &baseline, &mut removed, site))
        .collect()
}

fn proven(
    program: &Program,
    baseline: &Snapshot,
    removed: &mut Vec<Position>,
    site: Site,
) -> Option<TypeWarning> {
    removed.push(site.position);
    if preserves_callbacks(baseline, &rewrite(program, removed), removed) {
        Some(warning(site))
    } else {
        let _ = removed.pop();
        None
    }
}

fn warning(site: Site) -> TypeWarning {
    let name = osprey_ast::symbol::demangle(&site.name).unwrap_or(site.name);
    TypeWarning {
        message: format!(
            "redundant callback wrapper: pass `{name}` directly; it already takes no arguments"
        ),
        position: Some(site.position),
        rule: REDUNDANT_CALLBACK,
    }
}

fn candidates(program: &Program) -> Vec<Site> {
    let mut found = Candidates::default();
    for statement in &program.statements {
        named_functions(statement, &mut found.names);
    }
    walk_program(program, &mut found);
    found
        .sites
        .into_iter()
        .filter(|(site, invoker)| {
            found.names.contains(&site.name)
                && !found.shadowed.contains(&site.name)
                && invoker.as_ref().is_none_or(|name| {
                    found.invokers.contains(name) && !found.ambiguous.contains(name)
                })
        })
        .map(|(site, _)| site)
        .collect()
}

fn named_functions(statement: &Stmt, names: &mut HashSet<String>) {
    if let Some(name) = named_function(statement) {
        let _ = names.insert(name.to_owned());
    }
    match statement {
        Stmt::Namespace { body, .. } => {
            for statement in body {
                named_functions(statement, names);
            }
        }
        Stmt::Module { body, .. } => {
            for item in body {
                named_functions(&item.declaration, names);
            }
        }
        _ => {}
    }
}

fn named_function(statement: &Stmt) -> Option<&str> {
    match statement {
        Stmt::Function {
            name,
            parameters,
            type_params,
            ..
        } if parameters.is_empty() && type_params.is_empty() => Some(name),
        _ => None,
    }
}

impl Candidates {
    fn bind(&mut self, name: &str) {
        if !self.bindings.insert(name.to_owned()) {
            let _ = self.ambiguous.insert(name.to_owned());
        }
    }

    fn declaration(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Let {
                name,
                mutable,
                value,
                ..
            } => {
                self.bind(name);
                if !mutable && handler_value(value) {
                    let _ = self.invokers.insert(name.clone());
                }
                let _ = self.shadowed.insert(name.clone());
            }
            Stmt::Assignment { name, .. } => {
                let _ = self.ambiguous.insert(name.clone());
                let _ = self.shadowed.insert(name.clone());
            }
            _ => {}
        }
    }

    fn function(&mut self, name: &str, parameters: &[osprey_ast::Parameter], body: &Expr) {
        self.bind(name);
        if !self.definitions.insert(name.to_owned()) {
            let _ = self.shadowed.insert(name.to_owned());
        }
        if simple_invoker(parameters, body) {
            let _ = self.invokers.insert(name.to_owned());
        }
        for parameter in parameters {
            self.bind(&parameter.name);
            let _ = self.shadowed.insert(parameter.name.clone());
        }
    }
}

impl AstVisitor for Candidates {
    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Function {
                name,
                parameters,
                body,
                ..
            } => self.function(name, parameters, body),
            _ => self.declaration(statement),
        }
    }

    fn expression(&mut self, expression: &Expr) {
        if let Some(site) = invocation(expression) {
            self.sites.push(site);
        }
        for name in shadowing_names(expression) {
            self.bind(&name);
            let _ = self.shadowed.insert(name);
        }
    }
}

fn simple_invoker(parameters: &[osprey_ast::Parameter], body: &Expr) -> bool {
    parameters.len() == 1
        && parameters
            .first()
            .is_some_and(|p| forwarded_name(body) == Some(p.name.as_str()))
}

fn handler_value(expression: &Expr) -> bool {
    let Expr::Lambda {
        parameters, body, ..
    } = expression
    else {
        return false;
    };
    let Expr::Handler { body, .. } = body.as_ref() else {
        return false;
    };
    parameters
        .first()
        .is_some_and(|p| p.name == "$handler_action")
        && simple_invoker(parameters, body)
}

fn invocation(expression: &Expr) -> Option<(Site, Option<String>)> {
    let Expr::Call {
        function,
        arguments,
        named_arguments,
    } = expression
    else {
        return None;
    };
    if arguments.len() != 1 || !named_arguments.is_empty() {
        return None;
    }
    let site = site(arguments.first()?)?;
    let invoker = match function.as_ref() {
        Expr::Identifier(name) => Some(name.clone()),
        inline if handler_value(inline) => None,
        _ => return None,
    };
    Some((site, invoker))
}

fn shadowing_names(expression: &Expr) -> Vec<String> {
    match expression {
        Expr::Lambda { parameters, .. } => parameters.iter().map(|p| p.name.clone()).collect(),
        Expr::Match { arms, .. } | Expr::Select { arms } => arms
            .iter()
            .flat_map(|arm| pattern_binder_names(&arm.pattern))
            .collect(),
        Expr::Handler { arms, .. } => arms
            .iter()
            .flat_map(|arm| arm.params.iter().cloned())
            .collect(),
        _ => Vec::new(),
    }
}

fn site(expression: &Expr) -> Option<Site> {
    let Expr::Lambda {
        parameters,
        return_type: None,
        body,
        position: Some(position),
    } = expression
    else {
        return None;
    };
    if !parameters.is_empty() {
        return None;
    }
    Some(Site {
        name: forwarded_name(body)?.to_owned(),
        position: *position,
    })
}

fn forwarded_name(body: &Expr) -> Option<&str> {
    let Expr::Call {
        function,
        arguments,
        named_arguments,
    } = body
    else {
        return None;
    };
    let Expr::Identifier(name) = function.as_ref() else {
        return None;
    };
    (arguments.is_empty() && named_arguments.is_empty()).then_some(name)
}

fn rewrite(program: &Program, removed: &[Position]) -> Program {
    let mut candidate = program.clone();
    for statement in &mut candidate.statements {
        osprey_ast::mutate::statement_children_mut(statement, &mut |expression| {
            rewrite_expression(expression, removed);
        });
    }
    candidate
}

fn rewrite_expression(expression: &mut Expr, removed: &[Position]) {
    if let Some(site) = site(expression) {
        if removed.contains(&site.position) {
            *expression = Expr::Identifier(site.name);
            return;
        }
    }
    osprey_ast::mutate::children_mut(expression, &mut |child| rewrite_expression(child, removed));
}
