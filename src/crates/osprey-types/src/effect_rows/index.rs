//! Effect row index.
use super::{DeclaredEffect, Function, HashMap, Index, IndexedCallable, ModuleItem, Program, Stmt};

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
                    self.register_callable(qualified, name, IndexedCallable::Function(id));
                }
                Stmt::Effect {
                    name,
                    type_params,
                    operations,
                    ..
                } => {
                    let _ = self.effects.insert(name.clone(), type_params.len());
                    let names = operations.iter().map(|op| op.name.clone()).collect();
                    let _ = self.effect_operations.insert(name.clone(), names);
                }
                Stmt::Extern {
                    name, parameters, ..
                } => {
                    let names: Vec<_> = parameters.iter().map(|p| p.name.clone()).collect();
                    self.register_callable(
                        qualify(scope, name),
                        name,
                        IndexedCallable::Extern {
                            parameters: names,
                            signature: name.clone(),
                        },
                    );
                }
                Stmt::Type {
                    name,
                    variants,
                    type_params,
                    ..
                } => self.collect_type(name, type_params, variants),
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

    fn collect_type(
        &mut self,
        name: &str,
        parameters: &[osprey_ast::TypeParam],
        variants: &'a [osprey_ast::TypeVariant],
    ) {
        self.field_rows.register_parameters(name, parameters);
        for variant in variants {
            self.field_rows
                .collect(&variant.name, parameters, &variant.fields);
            let fields = variant
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect();
            let _ = self.constructors.insert(variant.name.clone(), fields);
        }
    }

    pub(super) fn collect_module_items(&mut self, items: &'a [ModuleItem], scope: &[String]) {
        for item in items {
            self.collect_stmts(std::slice::from_ref(item.declaration.as_ref()), scope);
        }
    }

    pub(super) fn resolve(&self, scope: &[String], name: &str) -> Option<usize> {
        match resolve_named(&self.qualified, &self.bare, scope, name)? {
            IndexedCallable::Function(id) => Some(*id),
            IndexedCallable::Extern { .. } => None,
        }
    }

    pub(super) fn extern_parameters(&self, scope: &[String], name: &str) -> Option<&Vec<String>> {
        match resolve_named(&self.qualified, &self.bare, scope, name)? {
            IndexedCallable::Extern { parameters, .. } => Some(parameters),
            IndexedCallable::Function(_) => None,
        }
    }

    pub(super) fn extern_signature_name(&self, scope: &[String], name: &str) -> Option<&str> {
        match resolve_named(&self.qualified, &self.bare, scope, name)? {
            IndexedCallable::Extern { signature, .. } => Some(signature),
            IndexedCallable::Function(_) => None,
        }
    }

    fn register_callable(&mut self, qualified: String, name: &str, callable: IndexedCallable) {
        let _ = self.qualified.insert(qualified, callable.clone());
        self.bare.entry(name.to_owned()).or_default().push(callable);
    }
}

fn resolve_named<'a, T>(
    qualified: &'a HashMap<String, T>,
    bare: &'a HashMap<String, Vec<T>>,
    scope: &[String],
    name: &str,
) -> Option<&'a T> {
    if name.contains("::") {
        return qualified.get(name);
    }
    for depth in (0..=scope.len()).rev() {
        let Some(prefix) = scope.get(..depth) else {
            continue;
        };
        let candidate = qualify(prefix, name);
        if let Some(value) = qualified.get(&candidate) {
            return Some(value);
        }
    }
    bare.get(name)
        .and_then(|values| (values.len() == 1).then(|| values.first()).flatten())
}

pub(super) fn qualify(scope: &[String], name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", scope.join("::"))
    }
}
