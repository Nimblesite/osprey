//! Effect row value flow.
use super::{
    merge_optional_value, project_element, Analyzer, BTreeMap, BTreeSet, Callable, CallableEnv,
    Expr, FieldAssignment, KnownCallable, Value,
};

impl Analyzer<'_> {
    pub(super) fn merge_collection_elements(
        &self,
        merged: &mut Option<Value>,
        expression: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) {
        let value = self
            .value(expression, scope, env)
            .unwrap_or_else(Value::unknown_value);
        if value.empty_case != Some(super::EmptyCase::Collection) {
            merge_optional_value(
                merged,
                project_element(value).unwrap_or_else(Value::unknown_value),
            );
        }
    }

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
                self.merge_collection_elements(&mut element, operand, scope, env);
            }
        }
        Value {
            empty_case: (collection && element.is_none()).then_some(super::EmptyCase::Collection),
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
        super::worklist::record_read(id);
        let parameters = self
            .index
            .functions
            .get(id)
            .map(|function| function.parameters.clone())
            .unwrap_or_default();
        Value::from_callable(Callable::Known(Box::new(KnownCallable {
            parameters,
            summary: self.rows.get(id).cloned().unwrap_or_default(),
            returned: Some(Box::new(
                self.returns.get(id).cloned().flatten().unwrap_or_default(),
            )),
        })))
    }
}
