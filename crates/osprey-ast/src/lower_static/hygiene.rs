//! Resolve lexical value bindings before static terms move between scopes.
use crate::mutate::{children_mut, statement_children_mut};
use crate::{Expr, MatchArm, Parameter, Pattern, Program, Stmt};
use std::collections::BTreeMap;

type Scope = BTreeMap<String, String>;

#[derive(Default)]
struct Resolver {
    next: u32,
    labels: BTreeMap<String, Scope>,
    constructors: BTreeMap<String, Vec<String>>,
}

pub(super) fn resolve(program: &Program) -> Program {
    let mut naming = Resolver::default();
    naming.collect_constructors(program);
    let mut resolved = program.clone();
    naming.statements(&mut resolved.statements, &mut Scope::new(), true);
    // A call can precede its declaration, so argument labels follow resolution.
    for statement in &mut resolved.statements {
        statement_children_mut(statement, &mut |value| naming.argument_labels(value));
    }
    resolved
}

impl Resolver {
    fn fresh(&mut self, name: &str) -> String {
        self.next = self.next.saturating_add(1);
        format!("{name}$stage{}", self.next)
    }

    fn bind(&mut self, name: &mut String, scope: &mut Scope) {
        let resolved = self.fresh(name);
        let _ = scope.insert(name.clone(), resolved.clone());
        *name = resolved;
    }

    fn reference(name: &mut String, scope: &Scope) {
        if let Some(resolved) = scope.get(name) {
            name.clone_from(resolved);
        }
    }

    fn statements(&mut self, statements: &mut [Stmt], scope: &mut Scope, top: bool) {
        // Named functions are recursive and visible throughout their block.
        for statement in &mut *statements {
            if let Stmt::Function { name, .. } = statement {
                if top {
                    let _ = scope.insert(name.clone(), name.clone());
                } else {
                    self.bind(name, scope);
                }
            }
        }
        for statement in statements {
            self.statement(statement, scope, top);
        }
    }

    fn statement(&mut self, statement: &mut Stmt, scope: &mut Scope, top: bool) {
        match statement {
            Stmt::Let { name, value, .. } => {
                self.expression(value, scope);
                if top {
                    let _ = scope.insert(name.clone(), name.clone());
                } else {
                    self.bind(name, scope);
                }
            }
            Stmt::Assignment { name, value, .. } => {
                Self::reference(name, scope);
                self.expression(value, scope);
            }
            Stmt::Function {
                name,
                parameters,
                body,
                ..
            } => {
                let labels = self.function(parameters, body, scope);
                let _ = self.labels.insert(name.clone(), labels);
            }
            Stmt::Namespace { body, .. } => self.statements(body, &mut scope.clone(), true),
            Stmt::Module { body, .. } => {
                let mut nested = scope.clone();
                for item in body {
                    self.statement(&mut item.declaration, &mut nested, true);
                }
            }
            _ => statement_children_mut(statement, &mut |value| self.expression(value, scope)),
        }
    }

    fn function(&mut self, parameters: &mut [Parameter], body: &mut Expr, scope: &Scope) -> Scope {
        let mut nested = scope.clone();
        let mut labels = Scope::new();
        for parameter in parameters {
            let original = parameter.name.clone();
            self.bind(&mut parameter.name, &mut nested);
            let _ = labels.insert(original, parameter.name.clone());
        }
        self.expression(body, &nested);
        labels
    }

    fn expression(&mut self, expression: &mut Expr, scope: &Scope) {
        match expression {
            Expr::Identifier(name) => Self::reference(name, scope),
            Expr::Block { statements, value } => {
                let mut nested = scope.clone();
                self.statements(statements, &mut nested, false);
                if let Some(value) = value {
                    self.expression(value, &nested);
                }
            }
            Expr::Lambda {
                parameters, body, ..
            } => {
                let _ = self.function(parameters, body, scope);
            }
            Expr::Handler { arms, body, .. } => {
                for arm in arms {
                    let mut nested = scope.clone();
                    for name in &mut arm.params {
                        self.bind(name, &mut nested);
                    }
                    self.expression(&mut arm.body, &nested);
                }
                self.expression(body, scope);
            }
            Expr::Match { value, arms } => {
                self.expression(value, scope);
                self.arms(arms, scope);
            }
            Expr::Select { arms } => self.arms(arms, scope),
            Expr::TypeConstructor { name, fields, .. }
            | Expr::Update {
                record: name,
                fields,
            } => {
                Self::reference(name, scope);
                for field in fields {
                    self.expression(&mut field.value, scope);
                }
            }
            _ => children_mut(expression, &mut |child| self.expression(child, scope)),
        }
    }

    fn arms(&mut self, arms: &mut [MatchArm], scope: &Scope) {
        for arm in arms {
            let mut nested = scope.clone();
            self.pattern(&mut arm.pattern, &mut nested);
            self.expression(&mut arm.body, &nested);
        }
    }

    fn pattern(&mut self, pattern: &mut Pattern, scope: &mut Scope) {
        match pattern {
            Pattern::Binding(name) if self.constructors.contains_key(name) => {}
            Pattern::Binding(name) | Pattern::TypeAnnotated { name, .. } => self.bind(name, scope),
            Pattern::Constructor {
                name,
                fields,
                sub_patterns,
            } => {
                if name == "Success" || name == "Error" {
                    for binder in &mut *fields {
                        self.bind(binder, scope);
                    }
                } else if let Some(order) = self
                    .constructors
                    .get(name)
                    .filter(|order| !order.is_empty() && !fields.is_empty())
                {
                    *sub_patterns = order
                        .iter()
                        .map(|field| {
                            if fields.contains(field) {
                                Pattern::Binding(field.clone())
                            } else {
                                Pattern::Wildcard
                            }
                        })
                        .collect();
                    fields.clear();
                }
                for pattern in sub_patterns {
                    self.pattern(pattern, scope);
                }
            }
            Pattern::Structural { fields, .. } => {
                for (_, binder) in fields.iter_mut().filter(|(_, name)| !name.is_empty()) {
                    self.bind(binder, scope);
                }
            }
            Pattern::List { elements, rest } => {
                for pattern in elements {
                    self.pattern(pattern, scope);
                }
                if let Some(name) = rest {
                    self.bind(name, scope);
                }
            }
            Pattern::Wildcard | Pattern::Literal(_) => {}
        }
    }

    fn argument_labels(&self, expression: &mut Expr) {
        if let Expr::Call {
            function,
            named_arguments,
            ..
        } = expression
        {
            let callee = match function.as_ref() {
                Expr::TypeApply { function, .. } => function.as_ref(),
                other => other,
            };
            if let Expr::Identifier(name) = callee {
                if let Some(labels) = self.labels.get(name) {
                    for argument in named_arguments {
                        Self::reference(&mut argument.name, labels);
                    }
                }
            }
        }
        children_mut(expression, &mut |child| self.argument_labels(child));
    }

    fn collect_constructors(&mut self, program: &Program) {
        struct Collector<'a>(&'a mut BTreeMap<String, Vec<String>>);
        impl crate::AstVisitor for Collector<'_> {
            fn statement(&mut self, statement: &Stmt) {
                if let Stmt::Type { variants, .. } = statement {
                    for variant in variants {
                        let _ = self.0.insert(
                            variant.name.clone(),
                            variant
                                .fields
                                .iter()
                                .map(|field| field.name.clone())
                                .collect(),
                        );
                    }
                }
            }
        }
        for name in ["Success", "Error", "Some", "None"] {
            let _ = self.constructors.insert(name.to_owned(), Vec::new());
        }
        crate::walk_program(program, &mut Collector(&mut self.constructors));
    }
}
