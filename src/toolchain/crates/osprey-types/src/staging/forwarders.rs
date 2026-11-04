//! Helpers that hand a callback to a signal query. A query's may-read set is
//! its callback's, so a query whose callback is a helper's parameter knows
//! nothing on its own, and every refresh through the helper rebuilt. Each
//! call passing a named function instead calls a copy of the helper with
//! that function substituted, so the query inside reads the callback's own
//! signals, exactly as it would written out at the call. A callback that is
//! a lambda or a runtime value keeps the conservative unknown set.
//! Implements [STAGE-SIGNALS-EXACT].

use super::signals::{callee_name, is_intrinsic};
use osprey_ast::{mutate, walk_statement, AstVisitor, Expr, Program, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// A bound on the copies one program gets, not an expected size.
const MAX_COPIES: usize = 4096;

/// Which argument of each query is its callback.
fn work_index(query: &str) -> usize {
    if query == "signalRefresh" {
        2
    } else {
        0
    }
}

/// `program` with every signal-forwarding call redirected to its copy, or
/// `None` when no helper forwards a callback.
pub(super) fn specialized(program: &Program) -> Option<Program> {
    let resolved = osprey_ast::stage::resolve_bindings(program);
    let definitions = osprey_ast::stage::function_definitions(&resolved);
    let forwarders = forwarders(&definitions);
    if forwarders.is_empty() {
        return None;
    }
    let mut copies = Copies {
        definitions,
        forwarders,
        made: BTreeMap::new(),
        produced: Vec::new(),
    };
    let mut result = resolved;
    for statement in &mut result.statements {
        mutate::statement_children_mut(statement, &mut |e| copies.rewrite(e));
    }
    if copies.produced.is_empty() {
        return None;
    }
    result.statements.extend(copies.produced);
    Some(result)
}

/// Each helper's parameters that reach a query's callback, directly or
/// through another helper.
fn forwarders(definitions: &BTreeMap<String, Stmt>) -> BTreeMap<String, BTreeSet<usize>> {
    let mut found: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    loop {
        let mut grew = false;
        for (name, definition) in definitions {
            let indices = forwarded(definition, &found);
            if !indices.is_empty() && found.get(name) != Some(&indices) {
                let _ = found.insert(name.clone(), indices);
                grew = true;
            }
        }
        if !grew {
            return found;
        }
    }
}

/// The parameters of `definition` passed as a callback a query reads.
fn forwarded(definition: &Stmt, found: &BTreeMap<String, BTreeSet<usize>>) -> BTreeSet<usize> {
    struct Reach<'a> {
        parameters: Vec<&'a str>,
        found: &'a BTreeMap<String, BTreeSet<usize>>,
        indices: BTreeSet<usize>,
    }
    impl AstVisitor for Reach<'_> {
        fn expression(&mut self, expression: &Expr) {
            for argument in callback_arguments(expression, self.found) {
                if let Expr::Identifier(name) = argument {
                    let at = self.parameters.iter().position(|p| p == name);
                    self.indices.extend(at);
                }
            }
        }
    }
    let Stmt::Function { parameters, .. } = definition else {
        return BTreeSet::new();
    };
    let mut reach = Reach {
        parameters: parameters.iter().map(|p| p.name.as_str()).collect(),
        found,
        indices: BTreeSet::new(),
    };
    walk_statement(definition, &mut reach);
    reach.indices
}

/// The arguments of a call that a query reads as its callback.
fn callback_arguments<'a>(
    expression: &'a Expr,
    found: &BTreeMap<String, BTreeSet<usize>>,
) -> Vec<&'a Expr> {
    let Expr::Call {
        function,
        arguments,
        ..
    } = expression
    else {
        return Vec::new();
    };
    let Some(name) = callee_name(function) else {
        return Vec::new();
    };
    let indices: Vec<usize> = if is_intrinsic(name) {
        vec![work_index(name)]
    } else {
        found.get(name).into_iter().flatten().copied().collect()
    };
    indices.iter().filter_map(|i| arguments.get(*i)).collect()
}

/// The copies made so far, keyed by helper and the callbacks substituted.
struct Copies {
    definitions: BTreeMap<String, Stmt>,
    forwarders: BTreeMap<String, BTreeSet<usize>>,
    made: BTreeMap<(String, BTreeMap<usize, String>), String>,
    produced: Vec<Stmt>,
}

impl Copies {
    fn rewrite(&mut self, expression: &mut Expr) {
        mutate::children_mut(expression, &mut |child| self.rewrite(child));
        if let Some(redirected) = self.redirected(expression) {
            *expression = redirected;
        }
    }

    /// A forwarding call passing named functions, redirected to its copy.
    fn redirected(&mut self, expression: &Expr) -> Option<Expr> {
        let Expr::Call {
            function,
            arguments,
            named_arguments,
        } = expression
        else {
            return None;
        };
        let Expr::Identifier(helper) = function.as_ref() else {
            return None;
        };
        let bound = self.named_callbacks(helper, arguments, named_arguments.is_empty())?;
        let copy = self.copy(helper, &bound)?;
        let rest = arguments
            .iter()
            .enumerate()
            .filter(|(index, _)| !bound.contains_key(index))
            .map(|(_, argument)| argument.clone())
            .collect();
        Some(Expr::Call {
            function: Box::new(Expr::Identifier(copy)),
            arguments: rest,
            named_arguments: Vec::new(),
        })
    }

    /// The forwarded arguments of a call to `helper` that name a function.
    fn named_callbacks(
        &self,
        helper: &str,
        arguments: &[Expr],
        positional: bool,
    ) -> Option<BTreeMap<usize, String>> {
        let indices = self.forwarders.get(helper)?;
        let Some(Stmt::Function { parameters, .. }) = self.definitions.get(helper) else {
            return None;
        };
        if !positional || arguments.len() != parameters.len() {
            return None;
        }
        let bound: BTreeMap<usize, String> = indices
            .iter()
            .filter_map(|index| match arguments.get(*index) {
                Some(Expr::Identifier(name)) if self.definitions.contains_key(name) => {
                    Some((*index, name.clone()))
                }
                _ => None,
            })
            .collect();
        (!bound.is_empty()).then_some(bound)
    }

    /// The copy of `helper` with `bound`'s callbacks substituted, made once.
    fn copy(&mut self, helper: &str, bound: &BTreeMap<usize, String>) -> Option<String> {
        let key = (helper.to_owned(), bound.clone());
        if let Some(name) = self.made.get(&key) {
            return Some(name.clone());
        }
        if self.made.len() >= MAX_COPIES {
            return None;
        }
        let name = format!("{helper}__sig{}", self.made.len() + 1);
        let _ = self.made.insert(key, name.clone());
        let mut definition = self.definitions.get(helper)?.clone();
        substituted(&mut definition, &name, bound);
        if let Stmt::Function { body, .. } = &mut definition {
            self.rewrite(body);
        }
        self.produced.push(definition);
        Some(name)
    }
}

/// Rename `definition` to `name`, drop the parameters `bound` replaces, and
/// put each callback where its parameter was read. Binding resolution made
/// every parameter name unique, so no other binding is touched.
fn substituted(definition: &mut Stmt, name: &str, bound: &BTreeMap<usize, String>) {
    let Stmt::Function {
        name: own,
        type_params,
        parameters,
        return_type,
        body,
        doc,
        ..
    } = definition
    else {
        return;
    };
    // A substituted callback fixes what the helper's binders stood for, so
    // the copy keeps only its concrete annotations and inference derives the
    // rest from the callback.
    let binders: Vec<String> = type_params.drain(..).map(|param| param.name).collect();
    for parameter in parameters.iter_mut() {
        erase_generic(&mut parameter.ty, &binders);
    }
    erase_generic(return_type, &binders);
    for (index, callback) in bound {
        if let Some(parameter) = parameters.get(*index) {
            replace(body, &parameter.name, callback);
        }
    }
    let mut index = 0;
    parameters.retain(|_| {
        index += 1;
        !bound.contains_key(&(index - 1))
    });
    name.clone_into(own);
    *doc = None;
}

fn erase_generic(annotation: &mut Option<osprey_ast::TypeExpr>, binders: &[String]) {
    if annotation.as_ref().is_some_and(|ty| mentions(ty, binders)) {
        *annotation = None;
    }
}

fn mentions(ty: &osprey_ast::TypeExpr, binders: &[String]) -> bool {
    binders.contains(&ty.name)
        || ty.generic_params.iter().any(|arg| mentions(arg, binders))
        || ty
            .parameter_types
            .iter()
            .any(|param| mentions(param, binders))
        || ty
            .return_type
            .as_deref()
            .is_some_and(|ret| mentions(ret, binders))
        || ty
            .array_element
            .as_deref()
            .is_some_and(|elem| mentions(elem, binders))
}

fn replace(expression: &mut Expr, parameter: &str, callback: &str) {
    if matches!(expression, Expr::Identifier(name) if name == parameter) {
        *expression = Expr::Identifier(callback.to_owned());
    } else {
        mutate::children_mut(expression, &mut |child| replace(child, parameter, callback));
    }
}
