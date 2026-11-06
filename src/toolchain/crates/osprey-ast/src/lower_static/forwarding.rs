//! Helpers that forward a callback to a static interpretation's installer, as
//! `fn under(name, work) = match name { "lists" => onLists(work)  _ => onGpu(work) }`
//! does. The callback reaches its region only through the helper, so the
//! helper is applied at its call site first and the installer's own
//! application then sees the callback. Implements [STAGE-LOWER-ORDER].

use super::{
    bindings::{installer, substitute},
    Lowering,
};
use crate::{Expr, Parameter, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// Every function that passes one of its own parameters to an installer, or to
/// another such function.
pub(super) fn collect(
    definitions: &BTreeMap<String, Stmt>,
    bindings: &BTreeMap<String, Expr>,
) -> BTreeSet<String> {
    let mut targets: BTreeSet<String> = bindings
        .keys()
        .filter(|name| installer(bindings, &Expr::Identifier((*name).clone())).is_some())
        .cloned()
        .collect();
    let mut forwarders = BTreeSet::new();
    loop {
        let found: Vec<String> = definitions
            .iter()
            .filter(|(name, definition)| !targets.contains(*name) && forwards(definition, &targets))
            .map(|(name, _)| name.clone())
            .collect();
        if found.is_empty() {
            return forwarders;
        }
        targets.extend(found.iter().cloned());
        forwarders.extend(found);
    }
}

fn forwards(definition: &Stmt, targets: &BTreeSet<String>) -> bool {
    struct Forwards<'a> {
        parameters: BTreeSet<&'a str>,
        targets: &'a BTreeSet<String>,
        found: bool,
    }
    impl crate::AstVisitor for Forwards<'_> {
        fn expression(&mut self, expression: &Expr) {
            if let Expr::Call {
                function,
                arguments,
                ..
            } = expression
            {
                self.found |= matches!(function.as_ref(), Expr::Identifier(name) if self.targets.contains(name))
                    && arguments.iter().any(|argument| {
                        matches!(argument, Expr::Identifier(name) if self.parameters.contains(name.as_str()))
                    });
            }
        }
    }
    let Stmt::Function { parameters, .. } = definition else {
        return false;
    };
    let mut visitor = Forwards {
        parameters: parameters
            .iter()
            .map(|parameter| parameter.name.as_str())
            .collect(),
        targets,
        found: false,
    };
    crate::walk_statement(definition, &mut visitor);
    visitor.found
}

impl Lowering {
    /// Apply a forwarding helper whose call passes a callback: callbacks are
    /// substituted so the installer inside sees them, and every other argument
    /// is bound once, in order, so it is still evaluated exactly once.
    pub(super) fn apply_forwarder(&mut self, expression: &mut Expr) -> bool {
        let Expr::Call {
            function,
            arguments,
            named_arguments,
        } = expression
        else {
            return false;
        };
        let Expr::Identifier(name) = function.as_ref() else {
            return false;
        };
        let name = name.clone();
        let Some(Stmt::Function {
            parameters,
            body,
            position,
            ..
        }) = self.forwarded_definition(&name)
        else {
            return false;
        };
        if !named_arguments.is_empty()
            || parameters.len() != arguments.len()
            || !arguments.iter().any(|argument| self.is_callback(argument))
        {
            return false;
        }
        let applied = self.inline(&parameters, &body, arguments);
        if !self.spend_fuel(&name, "forwarding", position) {
            return false;
        }
        let _ = self.consumed.insert(name);
        *expression = applied;
        true
    }

    fn forwarded_definition(&self, name: &str) -> Option<Stmt> {
        self.forwarders
            .contains(name)
            .then(|| self.definitions.get(name).cloned())
            .flatten()
    }

    pub(super) fn is_callback(&self, argument: &Expr) -> bool {
        match argument {
            Expr::Lambda { .. } => true,
            Expr::Identifier(name) => {
                self.definitions.contains_key(name) || self.bindings.contains_key(name)
            }
            _ => false,
        }
    }

    /// Inline one application: callback arguments are substituted so a static
    /// region inside the body sees them, and every other argument is bound
    /// once, in order, so it is still evaluated exactly once.
    pub(super) fn inline(&self, parameters: &[Parameter], body: &Expr, arguments: &[Expr]) -> Expr {
        let mut applied = body.clone();
        let mut statements = Vec::new();
        for (parameter, argument) in parameters.iter().zip(arguments) {
            if self.is_callback(argument) {
                substitute(&mut applied, &parameter.name, argument);
            } else {
                statements.push(Stmt::Let {
                    name: parameter.name.clone(),
                    mutable: false,
                    ty: parameter.ty.clone(),
                    value: argument.clone(),
                    doc: None,
                    position: None,
                });
            }
        }
        Expr::Block {
            statements,
            value: Some(Box::new(applied)),
            position: None,
        }
    }
}
