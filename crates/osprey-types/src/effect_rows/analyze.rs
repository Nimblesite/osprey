//! Effect row analyze.
use super::{
    bind_pattern, site_arguments, Analyzer, Callable, CallableEnv, Expr, Function, HashMap,
    HashSet, InterpolatedPart, Position, Requirement, Summary, Value,
};

impl Analyzer<'_> {
    pub(super) fn instance_arguments(
        &self,
        effect: &str,
        position: Option<Position>,
        sites: &HashMap<(u32, u32), Vec<String>>,
        role: &str,
    ) -> Vec<String> {
        if let Some(arguments) = site_arguments(position, sites) {
            return arguments;
        }
        if self.index.effects.get(effect).copied().unwrap_or_default() == 0 {
            return Vec::new();
        }
        let location = position.map_or_else(
            || "unknown".to_string(),
            |position| format!("{}:{}", position.line, position.column),
        );
        vec![format!("$unresolved-{role}-{location}")]
    }

    pub(super) fn perform_instance_arguments(
        &self,
        effect: &str,
        position: Option<Position>,
    ) -> Vec<Vec<String>> {
        if let Some(arguments) = position
            .and_then(|position| {
                self.instances
                    .performs
                    .get(&(position.line, position.column))
            })
            .filter(|arguments| !arguments.is_empty())
        {
            return arguments.clone();
        }
        if self.index.effects.get(effect).copied().unwrap_or_default() == 0 {
            return vec![Vec::new()];
        }
        let location = position.map_or_else(
            || "unknown".to_string(),
            |position| format!("{}:{}", position.line, position.column),
        );
        vec![vec![format!("$unresolved-perform-{location}")]]
    }

    /// A function's starting environment: the file-scope bindings with the
    /// function's own parameters bound over the top, so a parameter shadows a
    /// top-level `let` of the same name.
    pub(super) fn scoped_env(&self, parameters: &[String]) -> CallableEnv {
        let mut env = self.file_scope.clone();
        env.bind_parameters(parameters);
        env
    }

    pub(super) fn function_body(&self, function: &Function<'_>) -> Summary {
        self.expression(
            function.body,
            &function.scope,
            &self.scoped_env(&function.parameters),
        )
    }

    pub(super) fn function_return(&self, function: &Function<'_>) -> Option<Value> {
        let mut env = self.scoped_env(&function.parameters);
        self.returned_value(function.body, &function.scope, &mut env)
    }

    pub(super) fn returned_value(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) -> Option<Value> {
        if let Expr::Block {
            statements, value, ..
        } = expression
        {
            // Bind value provenance in execution order, but do not confuse the
            // statements' immediate effects with the trailing value's latent
            // callable/deferred row.
            let _ = self.statements(statements, scope, env);
            return value
                .as_deref()
                .and_then(|value| self.value(value, scope, env));
        }
        self.value(expression, scope, env)
    }

    /// An expression's own requirements: its children's, plus the `Arith`
    /// operation its operator may perform. Implements [ARITH-EFFECT-DISCHARGE].
    pub(super) fn expression(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        osprey_ast::with_stack(|| {
            let mut summary = self.expression_children(expression, scope, env);
            let types = self.instances.expression_types.borrow();
            let ty = types.get(&std::ptr::from_ref(expression).addr());
            if let Some(operation) = crate::arithmetic::operation(expression, ty) {
                let _ = summary.required.insert(Requirement::new(
                    osprey_ast::ARITH_EFFECT,
                    operation,
                    Vec::new(),
                ));
            }
            summary
        })
    }

    #[expect(
        clippy::too_many_lines,
        reason = "the exhaustive AST effect fold is clearest as one variant-complete match"
    )]
    pub(super) fn expression_children(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        match expression {
            Expr::Integer(_)
            | Expr::Float(_)
            | Expr::Str(_)
            | Expr::Bool(_)
            | Expr::Identifier(_)
            | Expr::Path(_)
            // Constructing a closure is pure. Its latent row is included only
            // when the closure is called or passed to an eager callback slot.
            | Expr::Lambda { .. } => Summary::default(),
            Expr::InterpolatedStr(parts) => {
                let mut out = Summary::default();
                for part in parts {
                    if let InterpolatedPart::Expr(expression) = part {
                        out.union(self.expression(expression, scope, env));
                    }
                }
                out
            }
            Expr::List(items, _) => self.expressions(items, scope, env),
            Expr::Map(entries) => {
                let mut out = Summary::default();
                for entry in entries {
                    out.union(self.expression(&entry.key, scope, env));
                    out.union(self.expression(&entry.value, scope, env));
                }
                out
            }
            Expr::Object(fields)
            | Expr::TypeConstructor { fields, .. }
            | Expr::Update { fields, .. } => {
                self.union_over(fields.iter().map(|field| &field.value), scope, env)
            }
            Expr::Binary { left, right, .. } => {
                let mut out = self.expression(left, scope, env);
                out.union(self.expression(right, scope, env));
                out
            }
            Expr::Pipe { left, right } => self.pipe_call(left, right, scope, env),
            Expr::TypeApply { function: operand, .. } | Expr::Unary { operand, .. }
            | Expr::FieldAccess {
                target: operand, ..
            }
            | Expr::Spawn(operand)
            | Expr::Await(operand)
            | Expr::Recv(operand) => self.expression(operand, scope, env),
            Expr::Yield(value) | Expr::Resume(value) => value
                .as_deref()
                .map_or_else(Summary::default, |v| self.expression(v, scope, env)),
            Expr::Send { channel, value }
            | Expr::Index {
                target: channel,
                index: value,
            } => {
                let mut out = self.expression(channel, scope, env);
                out.union(self.expression(value, scope, env));
                out
            }
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => match crate::methods::parts(expression) {
                Some(parts) => self.method_call(expression, &parts, scope, env),
                None => self.call(function, arguments, named_arguments, scope, env),
            },
            Expr::MethodCall {
                target,
                method,
                arguments,
                named_arguments,
            } => self.method_call(
                expression,
                &crate::methods::Parts {
                    target,
                    method,
                    arguments,
                    named: named_arguments,
                    application: None,
                },
                scope,
                env,
            ),
            Expr::Match { value, arms } => {
                let mut out = self.expression(value, scope, env);
                let matched = self.value(value, scope, env);
                for arm in arms {
                    let mut local = env.clone();
                    bind_pattern(&arm.pattern, matched.as_ref(), self.index, &mut local);
                    out.union(self.expression(&arm.body, scope, &local));
                }
                out
            }
            Expr::Select { arms } => {
                self.union_over(arms.iter().map(|arm| &arm.body), scope, env)
            }
            Expr::Block { statements, value, .. } => {
                let mut local = env.clone();
                let mut out = self.statements(statements, scope, &mut local);
                if let Some(value) = value {
                    out.union(self.expression(value, scope, &local));
                }
                out
            }
            Expr::Perform {
                effect,
                operation,
                arguments,
                named_arguments,
                position,
            } => {
                let mut out = self.expressions(arguments, scope, env);
                out.union(self.named_expressions(named_arguments, scope, env));
                for instance in self.perform_instance_arguments(effect, *position) {
                    let _ = out
                        .required
                        .insert(Requirement::new(effect, operation, instance));
                }
                out
            }
            Expr::Handler {
                effect,
                arms,
                body,
                return_clause,
                position,
                ..
            } => {
                // Arm bodies run outside this handler's discharge. In
                // particular, a same-effect arm is diagnosed separately rather
                // than being incorrectly removed here.
                let handled: HashSet<String> =
                    arms.iter().map(|arm| arm.operation.clone()).collect();
                let effect_arguments = self.instance_arguments(
                    effect,
                    *position,
                    &self.instances.handlers,
                    "handler",
                );
                let local = self.handler_body_env(effect, arms, body, *position, scope, env);
                let body_summary = self.expression(body, scope, &local);
                let effect = osprey_ast::effect_name::base(effect);
                if let Some(position) = position {
                    let candidates = body_summary.required.iter().filter(|requirement| {
                        requirement.effect == effect && handled.contains(&requirement.operation)
                            && self.instances.argument_types.contains_key(&requirement.arguments)
                    }).map(|requirement| requirement.arguments.clone());
                    self.instances.handler_inference.borrow_mut()
                        .entry((position.line, position.column)).or_default().extend(candidates);
                }
                let mut out = body_summary.without_operations(
                    effect,
                    &effect_arguments,
                    &handled,
                );
                for arm in arms {
                    let local = self.handler_arm_env(effect, arm, body, scope, env);
                    out.union(self.expression(&arm.body, scope, &local));
                }
                if let Some(clause) = return_clause {
                    let callee = self.callable(clause, scope, env).unwrap_or(Callable::Unknown);
                    out.union(self.invoke_with_values(callee, &[self.value(body, scope, &local)]));
                }
                out
            }
        }
    }
}
