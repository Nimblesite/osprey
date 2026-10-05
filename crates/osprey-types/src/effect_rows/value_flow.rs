//! Effect row value flow.
use super::{
    merge_optional_value, merge_value, project_element, walk_children, Analyzer, BTreeMap,
    BTreeSet, Callable, CallableEnv, Expr, FieldAssignment, KnownCallable, Stmt, Value,
};

impl Analyzer<'_> {
    /// `+` concatenates lists and merges maps, preserving callable elements.
    /// Numeric, string and comparison operators produce values without them.
    /// Implements [EFFECTS-PROVENANCE].
    pub(super) fn binary_value(
        &self,
        expression: &Expr,
        op: &str,
        left: &Expr,
        right: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Value {
        let collection = op == "+" && self.collection_result(expression);
        let mut element = None;
        if collection {
            for operand in [left, right] {
                if let Some(value) = self.value(operand, scope, env).and_then(project_element) {
                    merge_optional_value(&mut element, value);
                }
            }
        }
        Value {
            element: element.map(Box::new),
            field_names: Some(BTreeSet::new()),
            ..Value::default()
        }
    }

    pub(super) fn collection_result(&self, expression: &Expr) -> bool {
        matches!(
            self.instances.expression_types.borrow().get(&std::ptr::from_ref(expression).addr()),
            Some(crate::ty::Type::Con { name, .. }) if name == crate::ty::names::LIST || name == crate::ty::names::MAP
        )
    }

    /// Keep the complete field set independently of callable provenance: an
    /// opaque field is not evidence that a same-named free function should run.
    pub(super) fn record_value(
        &self,
        fields: &[FieldAssignment],
        scope: &[String],
        env: &CallableEnv,
    ) -> Value {
        let field_names = Some(fields.iter().map(|field| field.name.clone()).collect());
        let fields: BTreeMap<_, _> = fields
            .iter()
            .filter_map(|field| {
                self.value(&field.value, scope, env)
                    .filter(Value::carries_provenance)
                    .map(|value| (field.name.clone(), value))
            })
            .collect();
        Value {
            field_names,
            fields,
            ..Value::default()
        }
    }

    pub(super) fn function_value(&self, id: usize) -> Value {
        let parameters = self
            .index
            .functions
            .get(id)
            .map(|function| function.parameters.clone())
            .unwrap_or_default();
        Value::from_callable(Callable::Known(Box::new(KnownCallable {
            parameters,
            summary: self.rows.get(id).cloned().unwrap_or_default(),
            returned: self.returns.get(id).cloned().flatten().map(Box::new),
        })))
    }

    pub(super) fn flow_callable_assignments(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) {
        match expression {
            Expr::Block {
                statements, value, ..
            } => {
                for statement in statements {
                    match statement {
                        Stmt::Assignment { name, value, .. } => {
                            if let Some(incoming) = self.value(value, scope, env) {
                                let mut merged = env.values.remove(name);
                                merge_optional_value(&mut merged, incoming);
                                if let Some(merged) = merged {
                                    let _ = env.values.insert(name.clone(), merged);
                                }
                            }
                            self.flow_callable_assignments(value, scope, env);
                        }
                        Stmt::Let { value, .. } | Stmt::Expr { value, .. } => {
                            self.flow_callable_assignments(value, scope, env);
                        }
                        _ => {}
                    }
                }
                if let Some(value) = value {
                    self.flow_callable_assignments(value, scope, env);
                }
            }
            Expr::Handler {
                arms,
                body,
                return_clause,
                ..
            } => {
                for arm in arms {
                    self.flow_callable_assignments(&arm.body, scope, env);
                }
                self.flow_callable_assignments(body, scope, env);
                if let Some(clause) = return_clause {
                    if let Expr::Lambda { body, .. } = clause.as_ref() {
                        self.flow_callable_assignments(body, scope, env);
                    }
                }
            }
            Expr::Send { channel, value } => {
                let sites = self
                    .value(channel, scope, env)
                    .map(|channel| channel.channel_sites)
                    .unwrap_or_default();
                let payload = self
                    .value(value, scope, env)
                    .unwrap_or_else(Value::unknown_callable);
                for site in sites {
                    merge_value(
                        env.channel_payloads.entry(site).or_default(),
                        payload.clone(),
                    );
                }
                self.flow_callable_assignments(channel, scope, env);
                self.flow_callable_assignments(value, scope, env);
            }
            Expr::Match { value, arms } => {
                self.flow_callable_assignments(value, scope, env);
                for arm in arms {
                    self.flow_callable_assignments(&arm.body, scope, env);
                }
            }
            // A closure body has not executed merely because the closure value
            // was constructed, so its assignments cannot flow yet.
            Expr::Lambda { .. } => {}
            _ => walk_children(expression, |child| {
                self.flow_callable_assignments(child, scope, env);
            }),
        }
    }
}
