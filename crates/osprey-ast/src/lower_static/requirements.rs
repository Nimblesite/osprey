//! Requirements and liveness shared by static specializations.

use crate::stage_rows::body_requirements;
use crate::{Expr, Program, Stage, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// Declaration stage does not restrict explicit interpretation selection.
pub(super) fn all_requirements(
    program: &Program,
    bindings: &BTreeMap<String, Expr>,
) -> BTreeMap<String, Vec<String>> {
    let mut required = crate::stage_rows::requirements(program, Stage::Static);
    for (name, operations) in crate::stage_rows::requirements(program, Stage::Dynamic) {
        let combined = required.entry(name).or_default();
        combined.extend(operations);
        combined.sort();
        combined.dedup();
    }
    factory_requirements(required, bindings)
}

fn factory_requirements(
    mut required: BTreeMap<String, Vec<String>>,
    bindings: &BTreeMap<String, Expr>,
) -> BTreeMap<String, Vec<String>> {
    for (name, value) in bindings {
        if let Expr::Call { function, .. } = value {
            if let Expr::Identifier(factory) = function.as_ref() {
                let operations = required.get(factory).cloned().unwrap_or_default();
                let _ = required.insert(name.clone(), operations);
            }
        }
    }
    required
}

/// Specializing one caller does not consume dynamic callers of the same helper.
/// Keep originals reachable from surviving statements, including their callees.
pub(super) fn retain_referenced_originals(program: &mut Program, consumed: &BTreeSet<String>) {
    #[derive(Default)]
    struct References(BTreeSet<String>);
    impl crate::AstVisitor for References {
        fn expression(&mut self, expression: &Expr) {
            if let Expr::Identifier(name) = expression {
                let _ = self.0.insert(name.clone());
            }
        }
    }
    let mut retained = BTreeSet::new();
    loop {
        let mut roots = program.clone();
        prune(&mut roots, consumed, &retained);
        let mut referenced = References::default();
        crate::walk_program(&roots, &mut referenced);
        let previous = retained.len();
        retained.extend(referenced.0.intersection(consumed).cloned());
        if retained.len() == previous {
            break;
        }
    }
    prune(program, consumed, &retained);
}

/// Functions that still request a `static effect` after the rewrite, directly
/// or through a function they reference. A reachable request was discharged,
/// so each of these is dead code; the caller prunes them unless surviving code
/// references them, since no runtime interpretation of a static operation
/// exists ([STAGE-RESIDUE]).
pub(super) fn static_performers(
    program: &Program,
    effects: &BTreeMap<String, crate::stage::EffectDecl>,
) -> BTreeSet<String> {
    let bodies = function_definitions(program);
    let references: BTreeMap<&String, BTreeSet<String>> = bodies
        .iter()
        .map(|(name, definition)| (name, referenced_names(definition)))
        .collect();
    let mut performers: BTreeSet<String> = bodies
        .iter()
        .filter(|(_, definition)| {
            matches!(definition, Stmt::Function { body, .. } if crate::stage_rows::any_perform(body, |effect| {
                effects
                    .get(crate::effect_name::base(effect))
                    .is_some_and(|declared| declared.stage == Stage::Static)
            }))
        })
        .map(|(name, _)| name.clone())
        .collect();
    loop {
        let callers: Vec<String> = references
            .iter()
            .filter(|(_, referenced)| !referenced.is_disjoint(&performers))
            .map(|(name, _)| (*name).clone())
            .collect();
        let previous = performers.len();
        performers.extend(callers);
        if performers.len() == previous {
            return performers;
        }
    }
}

fn referenced_names(statement: &Stmt) -> BTreeSet<String> {
    #[derive(Default)]
    struct References(BTreeSet<String>);
    impl crate::AstVisitor for References {
        fn expression(&mut self, expression: &Expr) {
            if let Expr::Identifier(name) = expression {
                let _ = self.0.insert(name.clone());
            }
        }
    }
    let mut references = References::default();
    crate::walk_statement(statement, &mut references);
    references.0
}

/// Every named function definition in the program, at any nesting depth.
pub(crate) fn function_definitions(program: &Program) -> BTreeMap<String, Stmt> {
    #[derive(Default)]
    struct Collector(BTreeMap<String, Stmt>);
    impl crate::AstVisitor for Collector {
        fn statement(&mut self, statement: &Stmt) {
            if let Stmt::Function { name, .. } = statement {
                let _ = self.0.insert(name.clone(), statement.clone());
            }
        }
    }
    let mut collector = Collector::default();
    crate::walk_program(program, &mut collector);
    collector.0
}

fn prune(program: &mut Program, consumed: &BTreeSet<String>, retained: &BTreeSet<String>) {
    fn statements(
        values: &mut Vec<Stmt>,
        consumed: &BTreeSet<String>,
        retained: &BTreeSet<String>,
    ) {
        values.retain(|statement| {
            !matches!(statement, Stmt::Function { name, .. } | Stmt::Let { name, .. }
                if consumed.contains(name) && !retained.contains(name))
        });
        for statement in values {
            crate::mutate::statement_children_mut(statement, &mut |value| {
                expression(value, consumed, retained);
            });
        }
    }
    fn expression(value: &mut Expr, consumed: &BTreeSet<String>, retained: &BTreeSet<String>) {
        if let Expr::Block {
            statements: block,
            value: result,
            ..
        } = value
        {
            statements(block, consumed, retained);
            if let Some(result) = result {
                expression(result, consumed, retained);
            }
        } else {
            crate::mutate::children_mut(value, &mut |child| expression(child, consumed, retained));
        }
    }
    statements(&mut program.statements, consumed, retained);
}

impl super::Lowering {
    pub(super) fn remaining_requirements(&self, body: &Expr) -> Vec<String> {
        let mut required = body_requirements(
            &self.effects,
            &self.callables,
            body,
            Stage::Static,
            &self.requirements,
        );
        required.extend(body_requirements(
            &self.effects,
            &self.callables,
            body,
            Stage::Dynamic,
            &self.requirements,
        ));
        required.sort();
        required.dedup();
        required
    }

    /// The runtime dispatches `body` performs, a handler answering them or not.
    pub(super) fn dispatches_of(&self, body: &Expr) -> Vec<String> {
        crate::stage_rows::body_dispatches(&self.effects, &self.callables, body, &self.dispatches)
    }

    /// Everything a static arm still needs at run time: unanswered
    /// requirements and runtime dispatches alike ([STAGE-STATIC-MONOTONE]).
    pub(super) fn arm_requirements(&self, body: &Expr) -> Vec<String> {
        let mut required = self.remaining_requirements(body);
        required.extend(self.dispatches_of(body));
        required.sort();
        required.dedup();
        required
    }
}
