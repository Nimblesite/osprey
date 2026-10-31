//! Effect row value flow.
use super::{
    merge_optional_value, project_element, source_named_callee, Analyzer, BTreeMap, BTreeSet,
    Callable, CallableEnv, Expr, FieldAssignment, KnownCallable, Value,
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
        self.function_model(id, || {
            Some(Box::new(
                self.returns.get(id).cloned().flatten().unwrap_or_default(),
            ))
        })
    }

    fn function_model(&self, id: usize, returned: impl FnOnce() -> Option<Box<Value>>) -> Value {
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
            returned: returned(),
        })))
    }

    pub(super) fn call_effects_callable(
        &self,
        function: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Callable> {
        let value = if source_named_callee(function, scope, env, self.index) {
            self.summary_reference_value(function, scope, env)
        } else {
            self.value(function, scope, env)
        };
        value.and_then(Self::callable_from_value)
    }

    fn summary_reference_value(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        self.value_with_raw(expression, env, || {
            self.raw_summary_reference(expression, scope, env)
        })
    }

    fn raw_summary_reference(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        let id = match expression {
            Expr::Identifier(name) => self.index.resolve(scope, name),
            Expr::Path(path) => self.index.resolve(scope, &path.to_string()),
            Expr::TypeApply { function, .. } => {
                return self.summary_reference_value(function, scope, env)
            }
            _ => None,
        }?;
        Some(self.function_model(id, || None))
    }
}

#[cfg(test)]
mod unused_return_tests {
    use super::super::{CallArguments, ExprArgs, Index, Instances, Summary};
    use super::*;
    use osprey_ast::NamedArgument;

    fn returned_tree(width: usize) -> Value {
        assert!(width < super::super::MAX_PROVENANCE_NODES);
        Value {
            fields: (0..width)
                .map(|index| (format!("field{index}"), Value::channel(index)))
                .collect(),
            ..Value::default()
        }
    }

    fn with_return_analyzer<T>(width: usize, run: impl FnOnce(Analyzer<'_>) -> T) -> T {
        let parsed = osprey_syntax::parse_program("fn maker()=0\n");
        assert_eq!(parsed.errors, []);
        let index = Index::collect(&parsed.program);
        assert_eq!(index.resolve(&[], "maker"), Some(0));
        let rows = [Summary::default()];
        let returns = [Some(returned_tree(width))];
        let instances = Instances::default();
        run(model_analyzer(&index, &instances, &rows, &returns))
    }

    fn model_analyzer<'a>(
        index: &'a Index<'a>,
        instances: &'a Instances,
        rows: &'a [Summary],
        returns: &'a [Option<Value>],
    ) -> Analyzer<'a> {
        Analyzer {
            rows,
            returns,
            ..super::super::field_rows::tests::analyzer(index, instances)
        }
    }

    fn metadata(instances: &Instances) -> impl PartialEq + std::fmt::Debug {
        (
            instances.expression_types.borrow().clone(),
            instances.signal_queries.borrow().clone(),
            instances.handler_inference.borrow().clone(),
            instances.field_row_violations.borrow().clone(),
            instances.methods.clone(),
        )
    }

    fn owned_summary(analyzer: &Analyzer<'_>, function: &Expr) -> Summary {
        match analyzer.callable(function, &[], &CallableEnv::default()) {
            Some(callee) => analyzer.invoke_with_arguments(callee, CallArguments::default()),
            None => Summary {
                unresolved_dynamic_call: true,
                ..Summary::default()
            },
        }
    }

    fn independent_oracle(analyzer: &Analyzer<'_>, function: &Expr) -> (Summary, BTreeSet<usize>) {
        let instances = analyzer.instances.clone();
        let oracle = Analyzer {
            index: analyzer.index,
            rows: analyzer.rows,
            returns: analyzer.returns,
            instances: &instances,
            file_scope: CallableEnv::default(),
        };
        let (summary, reads) =
            super::super::worklist::recording(|| owned_summary(&oracle, function));
        assert_eq!(summary, Summary::default());
        assert_eq!(reads, BTreeSet::from([0]));
        assert_eq!(metadata(&instances), metadata(analyzer.instances));
        (summary, reads)
    }

    fn direct_summary(analyzer: &Analyzer<'_>, function: &Expr) -> Summary {
        analyzer.call(
            function,
            ExprArgs::new(&[]),
            &[],
            &[],
            &CallableEnv::default(),
        )
    }

    fn unused_return_clones(width: usize) -> usize {
        with_return_analyzer(width, |analyzer| {
            let function = Expr::Identifier("maker".to_owned());
            let expected = independent_oracle(&analyzer, &function);
            let before = metadata(analyzer.instances);
            let (actual, clones) = super::super::provenance::measured_clones(|| {
                super::super::worklist::recording(|| direct_summary(&analyzer, &function))
            });
            assert_eq!(actual, expected);
            assert_eq!(metadata(analyzer.instances), before);
            clones
        })
    }

    #[test]
    fn a_direct_summary_does_not_clone_an_unused_return_tree() {
        let observed = [8, 64, 128].map(unused_return_clones);
        assert_eq!(
            observed,
            [0, 0, 0],
            "unused return widths8/64/128 must not be cloned"
        );
    }

    const ARGUMENT: &str = "{\nlet deps=signalDependencies(fn()=>perform Alarm.ring())\n\
        let bad=Control{press:fn()=>perform Alarm.ring()}\n\
        handle Alarm {ring=>7}\nperform Alarm.ring()\n}";

    fn generic_source(callee: &str, named: bool) -> String {
        let label = if named { "value:" } else { "" };
        let declaration = format!("fn maker<T>(value)={callee}({label}{ARGUMENT})");
        let declaration = if callee.starts_with("tools::") {
            format!("namespace tools {{{declaration}}}")
        } else {
            declaration
        };
        format!(
            "effect Alarm {{ring:fn()->int}}\ntype Control={{press:fn()->int![]}}\n{declaration}\n"
        )
    }

    fn call_parts(expression: &Expr) -> (&Expr, ExprArgs<'_>, &[NamedArgument]) {
        match expression {
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => (function, ExprArgs::new(arguments), named_arguments),
            _ => panic!("the test fixture must be a call"),
        }
    }

    fn specialized_instances(function: &Expr) -> Instances {
        let mut instances = Instances::default();
        let _ = instances.argument_types.insert(Vec::new(), Vec::new());
        let (inner, target) = match function {
            Expr::TypeApply { function, .. } => (function.as_ref(), "Middle"),
            _ => (function, "int"),
        };
        insert_binding(&mut instances, inner, "T", target);
        if target == "Middle" {
            insert_binding(&mut instances, function, target, "int");
        }
        instances
    }

    fn insert_binding(instances: &mut Instances, expression: &Expr, name: &str, target: &str) {
        let _ = instances.instantiations.insert(
            super::super::expression_site(expression),
            [(name.to_owned(), target.to_owned())].into(),
        );
    }

    fn reference_call(analyzer: &Analyzer<'_>, expression: &Expr) -> Summary {
        let (function, arguments, named) = call_parts(expression);
        let env = CallableEnv::default();
        let mut summary = analyzer.expression(function, &[], &env);
        summary.union(analyzer.union_over(arguments.iter(), &[], &env));
        summary.union(analyzer.named_expressions(named, &[], &env));
        let callee = analyzer
            .callable(function, &[], &env)
            .expect("named fixture resolves");
        let arguments = analyzer.callsite_arguments(function, arguments, named, &[], &env);
        summary.union(analyzer.invoke_with_arguments(callee, arguments));
        summary
    }

    fn optimized_call(analyzer: &Analyzer<'_>, expression: &Expr) -> Summary {
        let (function, arguments, named) = call_parts(expression);
        analyzer.call(function, arguments, named, &[], &CallableEnv::default())
    }

    fn generic_case(index: &Index<'_>, body: &Expr) {
        let (function, _, _) = call_parts(body);
        let original = specialized_instances(function);
        let optimized = original.clone();
        let mut row = Summary::default();
        let _ = row.required.insert(super::super::Requirement::new(
            "Audit",
            "note",
            vec!["T".to_owned()],
        ));
        let rows = [row];
        let returns = [Some(returned_tree(8))];
        let before = model_analyzer(index, &original, &rows, &returns);
        let after = model_analyzer(index, &optimized, &rows, &returns);
        assert_specialized_call(&before, &after, body);
    }

    fn assert_specialized_call(before: &Analyzer<'_>, after: &Analyzer<'_>, body: &Expr) {
        let expected = super::super::worklist::recording(|| reference_call(before, body));
        let actual = super::super::worklist::recording(|| optimized_call(after, body));
        assert_eq!(actual, expected);
        assert_eq!(actual.1, BTreeSet::from([0]));
        assert!(actual.0.required.contains(&super::super::Requirement::new(
            "Audit",
            "note",
            vec!["int".to_owned()]
        )));
        super::super::expr_args::assert_metadata_written(before.instances);
        super::super::expr_args::assert_metadata_written(after.instances);
        super::super::expr_args::assert_model_metadata_matches(before.instances, after.instances);
    }

    #[test]
    fn direct_summary_preserves_generic_bindings_and_actual_argument_metadata() {
        for callee in ["maker", "maker<int>", "tools::maker", "tools::maker<int>"] {
            for named in [false, true] {
                let samples = super::super::field_rows::tests::fixture_function_values(
                    &generic_source(callee, named),
                    generic_case,
                );
                assert_eq!(samples.len(), 1);
            }
        }
    }
}
