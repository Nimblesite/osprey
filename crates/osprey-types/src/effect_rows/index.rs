//! Effect row index.
use super::{DeclaredEffect, Function, HashMap, Index, ModuleItem, Program, Stmt};

impl<'a> Index<'a> {
    pub(super) fn collect(program: &'a Program) -> Self {
        let mut index = Self {
            operations: osprey_ast::OperationTable::collect(program),
            effects: HashMap::from([(osprey_ast::ARITH_EFFECT.to_owned(), 0)]),
            ..Self::default()
        };
        index.collect_stmts(&program.statements, &[]);
        index
    }

    pub(super) fn collect_stmts(&mut self, statements: &'a [Stmt], scope: &[String]) {
        for statement in statements {
            match statement {
                Stmt::Function {
                    name,
                    parameters,
                    effects,
                    effect_tail,
                    effect_row_present,
                    body,
                    position,
                    ..
                } => {
                    let qualified = qualify(scope, name);
                    let id = self.functions.len();
                    self.functions.push(Function {
                        name: name.clone(),
                        qualified: qualified.clone(),
                        scope: scope.to_vec(),
                        parameters: parameters.iter().map(|p| p.name.clone()).collect(),
                        declared_effects: effects
                            .iter()
                            .map(|effect| DeclaredEffect {
                                name: effect.name.clone(),
                                arguments: (!effect.type_args.is_empty()).then(|| {
                                    effect
                                        .type_args
                                        .iter()
                                        .map(|argument| {
                                            crate::convert::type_expr_to_type(
                                                argument,
                                                &HashMap::new(),
                                            )
                                            .to_string()
                                        })
                                        .collect()
                                }),
                            })
                            .collect(),
                        effect_tail: effect_tail.clone(),
                        effect_row_present: *effect_row_present,
                        body,
                        position: *position,
                    });
                    let _ = self.qualified.insert(qualified, id);
                    self.bare.entry(name.clone()).or_default().push(id);
                }
                Stmt::Effect {
                    name, type_params, ..
                } => {
                    let _ = self.effects.insert(name.clone(), type_params.len());
                }
                Stmt::Type { variants, .. } => {
                    for variant in variants {
                        let _ = self.constructors.insert(
                            variant.name.clone(),
                            variant
                                .fields
                                .iter()
                                .map(|field| field.name.clone())
                                .collect(),
                        );
                    }
                }
                Stmt::Namespace { name, body, .. } => {
                    let mut nested = scope.to_vec();
                    nested.push(name.label().to_string());
                    self.collect_stmts(body, &nested);
                }
                Stmt::Module { path, body, .. } => {
                    let mut nested = scope.to_vec();
                    nested.extend(path.segments.iter().cloned());
                    self.collect_module_items(body, &nested);
                }
                _ => {}
            }
        }
    }

    pub(super) fn collect_module_items(&mut self, items: &'a [ModuleItem], scope: &[String]) {
        for item in items {
            self.collect_stmts(std::slice::from_ref(item.declaration.as_ref()), scope);
        }
    }

    pub(super) fn resolve(&self, scope: &[String], name: &str) -> Option<usize> {
        if name.contains("::") {
            return self.qualified.get(name).copied();
        }
        for depth in (0..=scope.len()).rev() {
            let Some(prefix) = scope.get(..depth) else {
                continue;
            };
            let candidate = qualify(prefix, name);
            if let Some(id) = self.qualified.get(&candidate) {
                return Some(*id);
            }
        }
        self.bare
            .get(name)
            .and_then(|ids| (ids.len() == 1).then(|| ids.first().copied()).flatten())
    }
}

pub(super) fn qualify(scope: &[String], name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", scope.join("::"))
    }
}
