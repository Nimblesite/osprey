//! State-module ownership checks and per-installer cell materialization.
//! Implements [MODULES-STATE], [MODULES-STATE-MODULE], and
//! [MODULES-STATE-SOURCE-OF-TRUTH].

use crate::imports::ImportScope;
use crate::model::{ProjectGraph, SymbolKey};
use crate::state_support::{
    inject, owned_cell_names, owned_effect_names, pattern_names, push_error,
};
use crate::{ProjectError, SourceMetadata};
use osprey_ast::{Expr, HandlerArm, MatchArm, ModuleItem, ModuleKind, Position, Stmt};
use std::collections::BTreeSet;

pub(crate) fn prepare(
    module_key: &SymbolKey,
    body: &mut Vec<ModuleItem>,
    graph: &ProjectGraph,
    scope: Option<&ImportScope>,
    sources: &[SourceMetadata],
    errors: &mut Vec<ProjectError>,
) {
    let Some(module) = graph.modules.get(module_key) else {
        return;
    };
    if module.kind != ModuleKind::State {
        return;
    }
    let cells: Vec<Stmt> = body
        .iter()
        .filter_map(|item| match item.declaration.as_ref() {
            statement @ Stmt::Let { mutable: true, .. } => Some(statement.clone()),
            _ => None,
        })
        .collect();
    if cells.is_empty() {
        return;
    }
    let cell_names = module.state_cells.clone();
    let aliases = scope.map_or_else(Vec::new, |scope| scope.aliases_for(module_key));
    let cell_paths = owned_cell_names(module_key, &cell_names, &aliases);
    let owned_effects = owned_effect_names(module_key, &module.effects, &aliases);
    if module
        .effects
        .intersection(&module.exports)
        .next()
        .is_none()
    {
        push_error(
            errors,
            sources,
            module.source,
            module.position,
            "a state module with cells needs an exported owned effect",
        );
    }
    let mut installers = BTreeSet::new();
    for item in body.iter() {
        inspect_item(
            item,
            &cell_names,
            &cell_paths,
            &owned_effects,
            module.source,
            sources,
            errors,
            &mut installers,
            false,
        );
    }
    let exported_installers = installers.intersection(&module.exports).next().is_some();
    if !exported_installers {
        push_error(
            errors,
            sources,
            module.source,
            module.position,
            "a state module with cells needs an exported handler installer",
        );
    }
    body.retain(|item| !matches!(item.declaration.as_ref(), Stmt::Let { mutable: true, .. }));
    for item in body {
        if let Stmt::Function { name, body, .. } = item.declaration.as_mut() {
            if installers.contains(name) {
                inject(body, &cells);
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "state inspection carries explicit owner and diagnostic context"
)]
fn inspect_item(
    item: &ModuleItem,
    cells: &BTreeSet<String>,
    cell_paths: &BTreeSet<String>,
    effects: &BTreeSet<String>,
    source: usize,
    sources: &[SourceMetadata],
    errors: &mut Vec<ProjectError>,
    installers: &mut BTreeSet<String>,
    nested_boundary: bool,
) {
    match item.declaration.as_ref() {
        Stmt::Function {
            name,
            parameters,
            body,
            position,
            ..
        } => {
            let mut inspector = Inspector::new(cells, cell_paths, effects, source, sources, errors);
            inspector.allow_owner_handlers = !nested_boundary;
            inspector
                .locals
                .extend(parameters.iter().map(|parameter| parameter.name.clone()));
            inspector.expr(body, false, *position);
            if inspector.found_owner_handler && !nested_boundary {
                let _ = installers.insert(name.clone());
            }
        }
        Stmt::Let {
            mutable: false,
            value,
            position,
            ..
        }
        | Stmt::Expr { value, position } => {
            Inspector::new(cells, cell_paths, effects, source, sources, errors)
                .expr(value, false, *position);
        }
        Stmt::Type {
            variants, position, ..
        } => {
            let mut inspector = Inspector::new(cells, cell_paths, effects, source, sources, errors);
            for field in variants.iter().flat_map(|variant| &variant.fields) {
                if let Some(constraint) = &field.constraint {
                    inspector.expr(constraint, false, *position);
                }
            }
        }
        Stmt::Module { body, .. } => {
            for nested in body {
                inspect_item(
                    nested, cells, cell_paths, effects, source, sources, errors, installers, true,
                );
            }
        }
        _ => {}
    }
}

struct Inspector<'a> {
    cells: &'a BTreeSet<String>,
    cell_paths: &'a BTreeSet<String>,
    effects: &'a BTreeSet<String>,
    source: usize,
    sources: &'a [SourceMetadata],
    errors: &'a mut Vec<ProjectError>,
    locals: BTreeSet<String>,
    found_owner_handler: bool,
    allow_owner_handlers: bool,
}

impl<'a> Inspector<'a> {
    fn new(
        cells: &'a BTreeSet<String>,
        cell_paths: &'a BTreeSet<String>,
        effects: &'a BTreeSet<String>,
        source: usize,
        sources: &'a [SourceMetadata],
        errors: &'a mut Vec<ProjectError>,
    ) -> Self {
        Self {
            cells,
            cell_paths,
            effects,
            source,
            sources,
            errors,
            locals: BTreeSet::new(),
            found_owner_handler: false,
            allow_owner_handlers: true,
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "state lifetime validation exhaustively walks every expression form"
    )]
    fn expr(&mut self, expression: &Expr, in_owner_arm: bool, position: Option<Position>) {
        match expression {
            Expr::Identifier(name) if self.is_cell(name) => {
                self.access(name, in_owner_arm, position);
            }
            Expr::Path(path) => {
                let name = path.segments.join("::");
                if self.cell_paths.contains(&name) {
                    self.access(&name, in_owner_arm, position);
                }
            }
            Expr::InterpolatedStr(parts) => {
                for part in parts {
                    if let osprey_ast::InterpolatedPart::Expr(value) = part {
                        self.expr(value, in_owner_arm, position);
                    }
                }
            }
            Expr::List(items) => self.exprs(items, in_owner_arm, position),
            Expr::Map(entries) => {
                for entry in entries {
                    self.expr(&entry.key, in_owner_arm, position);
                    self.expr(&entry.value, in_owner_arm, position);
                }
            }
            Expr::Object(fields) | Expr::TypeConstructor { fields, .. } => {
                self.fields(fields, in_owner_arm, position);
            }
            Expr::Update { record, fields } => {
                if self.is_cell(record) {
                    self.access(record, in_owner_arm, position);
                }
                self.fields(fields, in_owner_arm, position);
            }
            Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
                self.expr(left, in_owner_arm, position);
                self.expr(right, in_owner_arm, position);
            }
            Expr::Unary { operand, .. } | Expr::Await(operand) | Expr::Recv(operand) => {
                self.expr(operand, in_owner_arm, position);
            }
            Expr::Spawn(operand) => self.escaping(operand, position),
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => {
                self.expr(function, in_owner_arm, position);
                self.exprs(arguments, in_owner_arm, position);
                self.named(named_arguments, in_owner_arm, position);
            }
            Expr::MethodCall {
                target,
                arguments,
                named_arguments,
                ..
            } => {
                self.expr(target, in_owner_arm, position);
                self.exprs(arguments, in_owner_arm, position);
                self.named(named_arguments, in_owner_arm, position);
            }
            Expr::FieldAccess { target, .. } => self.expr(target, in_owner_arm, position),
            Expr::Index { target, index } => {
                self.expr(target, in_owner_arm, position);
                self.expr(index, in_owner_arm, position);
            }
            Expr::Lambda {
                parameters,
                body,
                position: lambda_position,
                ..
            } => {
                let saved = self.locals.clone();
                self.locals
                    .extend(parameters.iter().map(|parameter| parameter.name.clone()));
                self.escaping(body, *lambda_position);
                self.locals = saved;
            }
            Expr::Match { value, arms } => {
                self.expr(value, in_owner_arm, position);
                for arm in arms {
                    self.match_arm(arm, in_owner_arm, position);
                }
            }
            Expr::Block { statements, value } => {
                self.block(statements, value.as_deref(), in_owner_arm);
            }
            Expr::Yield(value) | Expr::Resume(value) => {
                if let Some(value) = value {
                    self.expr(value, in_owner_arm, position);
                }
            }
            Expr::Send { channel, value } => {
                self.expr(channel, in_owner_arm, position);
                self.expr(value, in_owner_arm, position);
            }
            Expr::Select { arms } => {
                for arm in arms {
                    self.match_arm(arm, in_owner_arm, position);
                }
            }
            Expr::Perform {
                arguments,
                named_arguments,
                position: perform_position,
                ..
            } => {
                self.exprs(arguments, in_owner_arm, *perform_position);
                self.named(named_arguments, in_owner_arm, *perform_position);
            }
            Expr::Handler {
                effect,
                arms,
                body,
                position: handler_position,
            } => self.handler(effect, arms, body, *handler_position),
            Expr::Integer(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Bool(_)
            | Expr::Identifier(_) => {}
        }
    }

    fn block(&mut self, statements: &[Stmt], value: Option<&Expr>, in_owner_arm: bool) {
        let saved = self.locals.clone();
        for statement in statements {
            match statement {
                Stmt::Let {
                    name,
                    value,
                    position,
                    ..
                } => {
                    self.expr(value, in_owner_arm, *position);
                    let _ = self.locals.insert(name.clone());
                }
                Stmt::Assignment {
                    name,
                    value,
                    position,
                } => {
                    if self.is_cell(name) {
                        self.access(name, in_owner_arm, *position);
                    }
                    self.expr(value, in_owner_arm, *position);
                }
                Stmt::Expr { value, position } => self.expr(value, in_owner_arm, *position),
                _ => {}
            }
        }
        if let Some(value) = value {
            self.expr(value, in_owner_arm, None);
        }
        self.locals = saved;
    }

    fn handler(
        &mut self,
        effect: &str,
        arms: &[HandlerArm],
        body: &Expr,
        position: Option<Position>,
    ) {
        self.expr(body, false, position);
        let owned = self.allow_owner_handlers && self.effects.contains(effect);
        self.found_owner_handler |= owned;
        for arm in arms {
            self.scoped(arm.params.clone(), |this| {
                this.expr(&arm.body, owned, position);
            });
        }
    }

    fn match_arm(&mut self, arm: &MatchArm, in_owner_arm: bool, position: Option<Position>) {
        let mut names = BTreeSet::new();
        pattern_names(&arm.pattern, &mut names);
        self.scoped(names, |this| this.expr(&arm.body, in_owner_arm, position));
    }

    fn scoped<I>(&mut self, names: I, action: impl FnOnce(&mut Self))
    where
        I: IntoIterator<Item = String>,
    {
        let saved = self.locals.clone();
        self.locals.extend(names);
        action(self);
        self.locals = saved;
    }

    fn escaping(&mut self, expression: &Expr, position: Option<Position>) {
        let previous = self.allow_owner_handlers;
        self.allow_owner_handlers = false;
        self.expr(expression, false, position);
        self.allow_owner_handlers = previous;
    }

    fn exprs(&mut self, values: &[Expr], inside: bool, position: Option<Position>) {
        for value in values {
            self.expr(value, inside, position);
        }
    }

    fn fields(
        &mut self,
        fields: &[osprey_ast::FieldAssignment],
        inside: bool,
        position: Option<Position>,
    ) {
        for field in fields {
            self.expr(&field.value, inside, position);
        }
    }

    fn named(
        &mut self,
        values: &[osprey_ast::NamedArgument],
        inside: bool,
        position: Option<Position>,
    ) {
        for value in values {
            self.expr(&value.value, inside, position);
        }
    }

    fn is_cell(&self, name: &str) -> bool {
        self.cells.contains(name) && !self.locals.contains(name)
    }

    fn access(&mut self, name: &str, allowed: bool, position: Option<Position>) {
        if !allowed {
            push_error(
                self.errors,
                self.sources,
                self.source,
                position,
                format!("state cell `{name}` is only accessible inside its owning handler arms"),
            );
        }
    }
}
