//! Static algebraic-effect discharge.
//!
//! Value inference predates effect rows and deliberately keeps their runtime
//! instantiation machinery separate. This pass computes the latent operation
//! requirements of function bodies, propagates them through calls, discharges
//! only the operations supplied by a handler, and proves the selected program
//! entry is pure. Operation-level requirements are essential: handlers are
//! partial, so an inner arm for `Policy.score` must not swallow a
//! `Policy.label` that belongs to an outer handler.

use crate::error::TypeError;
use osprey_ast::{
    Expr, HandlerArm, InterpolatedPart, ModuleItem, NamedArgument, Pattern, Position, Program, Stmt,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

#[derive(Default)]
pub(crate) struct Instances {
    /// A source position can identify more than one interpolation fragment.
    /// Preserve every independently inferred candidate instead of letting the
    /// final fragment overwrite the others.
    pub(crate) performs: HashMap<(u32, u32), Vec<Vec<String>>>,
    pub(crate) handlers: HashMap<(u32, u32), Vec<String>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Requirement {
    effect: String,
    operation: String,
    arguments: Vec<String>,
}

type Requirements = BTreeSet<Requirement>;

/// Provenance is an abstract value, not a runtime value tree. Recursive
/// closure producers can otherwise add one `returned` layer on every fixed
/// point iteration. At this height we widen the remaining tail to unknown,
/// which is conservative because invoking or consuming it is rejected.
const MAX_PROVENANCE_DEPTH: usize = 32;

/// One invocation of a function parameter, possibly below one or more
/// handlers. Keeping the exclusion set symbolic lets `apply(callback)` carry
/// the callback's effects to each call site without making ordinary value HM
/// inference effect-aware.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ParameterUse {
    /// Zero is the callable being invoked; larger values are successively
    /// enclosing callable scopes captured by a returned closure.
    level: usize,
    index: usize,
    projection: Vec<Projection>,
    excluded: Requirements,
}

/// A callable can be stored below a record field or collection element. Keep
/// that access path symbolic while the aggregate itself is a function
/// parameter, then resolve it against the concrete argument at the call site.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Projection {
    Field(String),
    Element,
    SuccessValue,
    FiberValue,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Summary {
    required: Requirements,
    parameter_uses: BTreeSet<ParameterUse>,
    unresolved_dynamic_call: bool,
}

impl Summary {
    fn union(&mut self, other: Self) {
        self.required.extend(other.required);
        self.parameter_uses.extend(other.parameter_uses);
        self.unresolved_dynamic_call |= other.unresolved_dynamic_call;
    }

    fn without_operations(
        mut self,
        effect: &str,
        arguments: &[String],
        operations: &HashSet<String>,
    ) -> Self {
        let excluded: Requirements = self
            .required
            .iter()
            .filter(|r| {
                r.effect == effect && r.arguments == arguments && operations.contains(&r.operation)
            })
            .cloned()
            .collect();
        self.required.retain(|r| !excluded.contains(r));
        if !operations.is_empty() {
            self.parameter_uses = self
                .parameter_uses
                .into_iter()
                .map(|mut use_| {
                    // A callback requirement is not concrete until its call
                    // site. Record every operation this handler can discharge.
                    use_.excluded
                        .extend(operations.iter().map(|operation| Requirement {
                            effect: effect.to_string(),
                            operation: operation.clone(),
                            arguments: arguments.to_vec(),
                        }));
                    use_
                })
                .collect();
        }
        self
    }

    fn excluding(mut self, excluded: &Requirements) -> Self {
        self.required.retain(|r| !excluded.contains(r));
        self.parameter_uses = self
            .parameter_uses
            .into_iter()
            .map(|mut use_| {
                use_.excluded.extend(excluded.iter().cloned());
                use_
            })
            .collect();
        self
    }

    fn widen(&mut self) {
        let before = self.parameter_uses.len();
        self.parameter_uses.retain(|use_| {
            use_.level < MAX_PROVENANCE_DEPTH && use_.projection.len() < MAX_PROVENANCE_DEPTH
        });
        if self.parameter_uses.len() != before {
            self.unresolved_dynamic_call = true;
        }
    }
}

#[derive(Clone)]
struct DeclaredEffect {
    name: String,
    /// `None` is an uninstantiated/wildcard row entry; `Some` is an exact
    /// generic instance contract.
    arguments: Option<Vec<String>>,
}

#[derive(Clone)]
struct Function {
    name: String,
    qualified: String,
    scope: Vec<String>,
    parameters: Vec<String>,
    declared_effects: Vec<DeclaredEffect>,
    body: Expr,
    position: Option<Position>,
}

#[derive(Default)]
struct Index {
    functions: Vec<Function>,
    qualified: HashMap<String, usize>,
    bare: HashMap<String, Vec<usize>>,
    effects: HashMap<String, usize>,
    constructors: HashMap<String, Vec<String>>,
    /// `extern fn` symbols. FFI cannot perform an Osprey operation, so calling
    /// one is proven effect-free even though it has no body to analyse.
    externs: HashSet<String>,
}

impl Index {
    fn collect(program: &Program) -> Self {
        let mut index = Self::default();
        index.collect_stmts(&program.statements, &[]);
        index
    }

    fn collect_stmts(&mut self, statements: &[Stmt], scope: &[String]) {
        for statement in statements {
            match statement {
                Stmt::Extern { name, .. } => {
                    let _ = self.externs.insert(name.clone());
                }
                Stmt::Function {
                    name,
                    parameters,
                    effects,
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
                        body: body.clone(),
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

    fn collect_module_items(&mut self, items: &[ModuleItem], scope: &[String]) {
        for item in items {
            self.collect_stmts(std::slice::from_ref(item.declaration.as_ref()), scope);
        }
    }

    fn resolve(&self, scope: &[String], name: &str) -> Option<usize> {
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

fn qualify(scope: &[String], name: &str) -> String {
    if scope.is_empty() {
        name.to_string()
    } else {
        format!("{}::{name}", scope.join("::"))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct KnownCallable {
    parameters: Vec<String>,
    summary: Summary,
    returned: Option<Box<Value>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Callable {
    Known(Box<KnownCallable>),
    Unknown,
    Parameter {
        level: usize,
        index: usize,
        projection: Vec<Projection>,
    },
}

/// The callable-bearing portion of a value. This is deliberately a mergeable
/// lattice instead of an enum: a control-flow join may produce a callable in
/// one arm and a record/list in another, and discarding either provenance
/// would make an effect escape possible.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Value {
    callable: Option<Callable>,
    fields: BTreeMap<String, Value>,
    element: Option<Box<Value>>,
    result_payload: Option<Box<Value>>,
    fiber_payload: Option<Box<Value>>,
    channel_sites: BTreeSet<usize>,
    deferred: Summary,
}

impl Value {
    fn from_callable(callable: Callable) -> Self {
        Self {
            callable: Some(callable),
            ..Self::default()
        }
    }

    fn parameter(level: usize, index: usize) -> Self {
        Self::from_callable(Callable::Parameter {
            level,
            index,
            projection: Vec::new(),
        })
    }

    fn unknown_callable() -> Self {
        Self::from_callable(Callable::Unknown)
    }

    fn widened(self) -> Self {
        widen_value(self, 0)
    }

    fn channel(site: usize) -> Self {
        Self {
            channel_sites: [site].into_iter().collect(),
            ..Self::default()
        }
    }

    fn union(&mut self, other: Self) {
        if let Some(callable) = other.callable {
            merge_callable(&mut self.callable, callable);
        }
        for (name, value) in other.fields {
            merge_value(self.fields.entry(name).or_default(), value);
        }
        if let Some(element) = other.element {
            merge_boxed_value(&mut self.element, *element);
        }
        if let Some(value) = other.result_payload {
            merge_boxed_value(&mut self.result_payload, *value);
        }
        if let Some(value) = other.fiber_payload {
            merge_boxed_value(&mut self.fiber_payload, *value);
        }
        self.channel_sites.extend(other.channel_sites);
        self.deferred.union(other.deferred);
    }
}

#[derive(Clone, Default)]
struct CallableEnv {
    values: HashMap<String, Value>,
    shadowed: HashSet<String>,
    channel_payloads: BTreeMap<usize, Value>,
}

impl CallableEnv {
    fn for_parameters(parameters: &[String]) -> Self {
        let mut env = Self::default();
        for (index, parameter) in parameters.iter().enumerate() {
            let _ = env.shadowed.insert(parameter.clone());
            let _ = env
                .values
                .insert(parameter.clone(), Value::parameter(0, index));
        }
        env
    }

    fn enter_lambda(&self, parameters: &[String]) -> Self {
        let mut env = self.clone();
        for value in env.values.values_mut() {
            shift_value_levels(value, 1, 0);
        }
        for value in env.channel_payloads.values_mut() {
            shift_value_levels(value, 1, 0);
        }
        for (index, parameter) in parameters.iter().enumerate() {
            let _ = env.shadowed.insert(parameter.clone());
            let _ = env
                .values
                .insert(parameter.clone(), Value::parameter(0, index));
        }
        env
    }
}

struct Analyzer<'a> {
    index: &'a Index,
    rows: &'a [Summary],
    returns: &'a [Option<Value>],
    instances: &'a Instances,
}

impl Analyzer<'_> {
    fn instance_arguments(
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

    fn perform_instance_arguments(
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

    fn function_body(&self, function: &Function) -> Summary {
        self.expression(
            &function.body,
            &function.scope,
            &CallableEnv::for_parameters(&function.parameters),
        )
    }

    fn function_return(&self, function: &Function) -> Option<Value> {
        let mut env = CallableEnv::for_parameters(&function.parameters);
        self.returned_value(&function.body, &function.scope, &mut env)
    }

    fn returned_value(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) -> Option<Value> {
        if let Expr::Block { statements, value } = expression {
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

    #[expect(
        clippy::too_many_lines,
        reason = "the exhaustive AST effect fold is clearest as one variant-complete match"
    )]
    fn expression(&self, expression: &Expr, scope: &[String], env: &CallableEnv) -> Summary {
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
            Expr::List(items) => self.expressions(items, scope, env),
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
                let mut out = Summary::default();
                for field in fields {
                    out.union(self.expression(&field.value, scope, env));
                }
                out
            }
            Expr::Binary { left, right, .. } => {
                let mut out = self.expression(left, scope, env);
                out.union(self.expression(right, scope, env));
                out
            }
            Expr::Pipe { left, right } => self.pipe_call(left, right, scope, env),
            Expr::Unary { operand, .. }
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
            } => self.call(function, arguments, named_arguments, scope, env),
            Expr::MethodCall {
                target,
                method,
                arguments,
                named_arguments,
            } => {
                let mut all_arguments = Vec::with_capacity(arguments.len() + 1);
                all_arguments.push((**target).clone());
                all_arguments.extend(arguments.iter().cloned());
                self.call(
                    &Expr::Identifier(method.clone()),
                    &all_arguments,
                    named_arguments,
                    scope,
                    env,
                )
            }
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
                let mut out = Summary::default();
                for arm in arms {
                    out.union(self.expression(&arm.body, scope, env));
                }
                out
            }
            Expr::Block { statements, value } => {
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
                    let _ = out.required.insert(Requirement {
                        effect: effect.clone(),
                        operation: operation.clone(),
                        arguments: instance,
                    });
                }
                out
            }
            Expr::Handler {
                effect,
                arms,
                body,
                position,
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
                let mut out = self.expression(body, scope, env).without_operations(
                    effect,
                    &effect_arguments,
                    &handled,
                );
                for arm in arms {
                    let local = self.handler_arm_env(effect, arm, body, scope, env);
                    out.union(self.expression(&arm.body, scope, &local));
                }
                out
            }
        }
    }

    fn call(
        &self,
        function: &Expr,
        arguments: &[Expr],
        named_arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let mut out = self.expression(function, scope, env);
        out.union(self.expressions(arguments, scope, env));
        out.union(self.named_expressions(named_arguments, scope, env));

        if let Some(callee) = self.callable(function, scope, env) {
            out.union(self.invoke(callee, arguments, named_arguments, scope, env));
        } else if !statically_named_callee(function, env, self.index) {
            // A computed value that successfully type-checks as a function must
            // carry effect provenance. If an unsupported transport erased that
            // provenance, fail closed instead of assuming the call is pure.
            out.unresolved_dynamic_call = true;
        }
        if let Some(name) = expression_name(function) {
            if iterator_consumer(name) {
                if let Some(iterator) = arguments
                    .first()
                    .and_then(|argument| self.value(argument, scope, env))
                {
                    out.union(iterator.deferred);
                }
            }
            for index in eager_callback_slots(name) {
                if let Some(argument) = arguments.get(*index) {
                    if let Some(callback) = self.callable(argument, scope, env) {
                        out.union(self.invoke(callback, &[], &[], scope, env));
                    }
                }
            }
        }
        out
    }

    fn invoke(
        &self,
        callee: Callable,
        arguments: &[Expr],
        named_arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let parameters: &[String] = match &callee {
            Callable::Known(known) => &known.parameters,
            Callable::Parameter { .. } | Callable::Unknown => &[],
        };
        let values = self.argument_values(parameters, arguments, named_arguments, scope, env);
        self.invoke_with_values(callee, &values)
    }

    fn invoke_with_values(&self, callee: Callable, arguments: &[Option<Value>]) -> Summary {
        match callee {
            Callable::Known(known) => {
                self.substitute_summary_at(known.summary.clone(), 0, arguments)
            }
            Callable::Unknown => Summary {
                unresolved_dynamic_call: true,
                ..Summary::default()
            },
            Callable::Parameter {
                level,
                index,
                projection,
            } => Summary {
                required: Requirements::new(),
                parameter_uses: [ParameterUse {
                    level,
                    index,
                    projection,
                    excluded: Requirements::new(),
                }]
                .into_iter()
                .collect(),
                unresolved_dynamic_call: false,
            },
        }
    }

    fn argument_values(
        &self,
        parameters: &[String],
        arguments: &[Expr],
        named_arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Vec<Option<Value>> {
        let mut values = vec![None; parameters.len().max(arguments.len())];
        for (index, argument) in arguments.iter().enumerate() {
            if let Some(slot) = values.get_mut(index) {
                *slot = self.value(argument, scope, env);
            }
        }
        for argument in named_arguments {
            if let Some(index) = parameters.iter().position(|name| name == &argument.name) {
                if let Some(slot) = values.get_mut(index) {
                    *slot = self.value(&argument.value, scope, env);
                }
            }
        }
        values
    }

    fn call_value(
        &self,
        function: &Expr,
        arguments: &[Expr],
        named_arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        if let Some(name) = expression_name(function) {
            if let Some(value) = self.builtin_call_value(name, arguments, scope, env) {
                return Some(value);
            }
        }
        let Some(Callable::Known(known)) = self.callable(function, scope, env) else {
            // The RESULT of a call we cannot resolve is not thereby a callable.
            // This branch catches every builtin without a modelled value
            // (`print`, `listAppend`, …), whose result is plain data; calling
            // it an unknown callable makes each one poison the merges it flows
            // into. Staying fail-closed does not depend on this marker: a value
            // with no callable that is later invoked fails
            // `statically_named_callee` and is reported at that call.
            return Some(Value::default());
        };
        let arguments =
            self.argument_values(&known.parameters, arguments, named_arguments, scope, env);
        known
            .returned
            .as_deref()
            .cloned()
            .map(|returned| self.substitute_value_at(returned, 0, &arguments))
            // A known callee with no recorded return provenance says nothing
            // about whether its RESULT is callable — most such results are
            // ordinary data, and `returned` is also what the depth cutoff drops
            // first. Yielding a provenance-free value keeps that distinction:
            // it stays fail-closed, because invoking a value with no callable
            // still fails `statically_named_callee` at the call and is reported
            // there, while an `int` that is merely returned no longer poisons
            // every merge it flows into with an unresolved-callable verdict.
            .or_else(|| Some(Value::default()))
    }

    fn builtin_call_value(
        &self,
        name: &str,
        arguments: &[Expr],
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        let element = |index| {
            arguments
                .get(index)
                .and_then(|argument| self.value(argument, scope, env))
                .and_then(project_element)
        };
        let aggregate = |element: Option<Value>| Value {
            element: element.map(Box::new),
            ..Value::default()
        };
        Some(match name {
            "range" | "List" | "Map" => Value::default(),
            "map" | "filter" => self.lazy_iterator_value(name, arguments, scope, env),
            "mapList" | "filterList" => {
                let mut value = self.lazy_iterator_value(name, arguments, scope, env);
                value.deferred = Summary::default();
                value
            }
            "listAppend" | "listPrepend" => {
                let mut merged = element(0);
                if let Some(value) = arguments
                    .get(1)
                    .and_then(|argument| self.value(argument, scope, env))
                {
                    merge_optional_value(&mut merged, value);
                }
                aggregate(merged)
            }
            "listConcat" | "mapMerge" => {
                let mut merged = element(0);
                if let Some(value) = element(1) {
                    merge_optional_value(&mut merged, value);
                }
                aggregate(merged)
            }
            "listReverse" | "mapRemove" => arguments
                .first()
                .and_then(|argument| self.value(argument, scope, env))
                .unwrap_or_default(),
            "listGet" | "mapGet" => Value {
                result_payload: Some(Box::new(element(0).unwrap_or_else(Value::unknown_callable))),
                ..Value::default()
            },
            "mapSet" => {
                let mut merged = element(0);
                if let Some(value) = arguments
                    .get(2)
                    .and_then(|argument| self.value(argument, scope, env))
                {
                    merge_optional_value(&mut merged, value);
                }
                aggregate(merged)
            }
            "mapValues" => aggregate(element(0)),
            _ => return None,
        })
    }

    fn lazy_iterator_value(
        &self,
        name: &str,
        arguments: &[Expr],
        scope: &[String],
        env: &CallableEnv,
    ) -> Value {
        let source = arguments
            .first()
            .and_then(|argument| self.value(argument, scope, env))
            .unwrap_or_default();
        let source_element = source.element.as_deref().cloned();
        let mut deferred = source.deferred;
        let callback = arguments
            .get(1)
            .and_then(|argument| self.callable(argument, scope, env));
        let callback_arguments = vec![source_element.clone()];
        if let Some(callback) = callback.clone() {
            deferred.union(self.invoke_with_values(callback, &callback_arguments));
        }
        let element = if matches!(name, "map" | "mapList") {
            callback.and_then(|callback| match callback {
                Callable::Known(known) => known
                    .returned
                    .as_deref()
                    .cloned()
                    .map(|returned| self.substitute_value_at(returned, 0, &callback_arguments)),
                Callable::Parameter { .. } | Callable::Unknown => None,
            })
        } else {
            source_element
        };
        Value {
            element: element.map(Box::new),
            deferred,
            ..Value::default()
        }
    }

    fn pipe_call(&self, left: &Expr, right: &Expr, scope: &[String], env: &CallableEnv) -> Summary {
        match right {
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => {
                let mut all_arguments = Vec::with_capacity(arguments.len() + 1);
                all_arguments.push(left.clone());
                all_arguments.extend(arguments.iter().cloned());
                self.call(function, &all_arguments, named_arguments, scope, env)
            }
            _ => self.call(right, std::slice::from_ref(left), &[], scope, env),
        }
    }

    fn pipe_value(
        &self,
        left: &Expr,
        right: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> Option<Value> {
        match right {
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => {
                let mut all_arguments = Vec::with_capacity(arguments.len() + 1);
                all_arguments.push(left.clone());
                all_arguments.extend(arguments.iter().cloned());
                self.call_value(function, &all_arguments, named_arguments, scope, env)
            }
            _ => self.call_value(right, std::slice::from_ref(left), &[], scope, env),
        }
    }

    fn callable(&self, expression: &Expr, scope: &[String], env: &CallableEnv) -> Option<Callable> {
        self.value(expression, scope, env)
            .and_then(|value| value.callable)
    }

    fn channel_payload(channel: Value, env: &CallableEnv) -> Option<Value> {
        let mut payload = None;
        for site in channel.channel_sites {
            if let Some(value) = env.channel_payloads.get(&site) {
                merge_optional_value(&mut payload, value.clone());
            }
        }
        payload
    }

    #[expect(
        clippy::too_many_lines,
        reason = "value provenance mirrors the exhaustive expression fold"
    )]
    fn value(&self, expression: &Expr, scope: &[String], env: &CallableEnv) -> Option<Value> {
        match expression {
            Expr::Identifier(name) => {
                if let Some(value) = env.values.get(name) {
                    return Some(value.clone());
                }
                if env.shadowed.contains(name) {
                    return None;
                }
                self.index
                    .resolve(scope, name)
                    .map(|id| self.function_value(id))
            }
            Expr::Path(path) => self
                .index
                .resolve(scope, &path.to_string())
                .map(|id| self.function_value(id)),
            Expr::Lambda {
                parameters, body, ..
            } => {
                let names: Vec<String> = parameters.iter().map(|p| p.name.clone()).collect();
                let mut local = env.enter_lambda(&names);
                let summary = self.expression(body, scope, &local);
                let returned = self.returned_value(body, scope, &mut local).map(Box::new);
                Some(Value::from_callable(Callable::Known(Box::new(
                    KnownCallable {
                        parameters: names,
                        summary,
                        returned,
                    },
                ))))
            }
            Expr::FieldAccess { target, field } => self
                .value(target, scope, env)
                .and_then(|value| project_field(value, field)),
            Expr::List(items) => {
                let mut element = None;
                for item in items {
                    if let Some(value) = self.value(item, scope, env) {
                        merge_optional_value(&mut element, value);
                    }
                }
                element.map(|element| Value {
                    element: Some(Box::new(element)),
                    ..Value::default()
                })
            }
            Expr::Map(entries) => {
                let mut element = None;
                for entry in entries {
                    if let Some(value) = self.value(&entry.value, scope, env) {
                        merge_optional_value(&mut element, value);
                    }
                }
                element.map(|element| Value {
                    element: Some(Box::new(element)),
                    ..Value::default()
                })
            }
            Expr::Object(fields) => {
                let fields: BTreeMap<_, _> = fields
                    .iter()
                    .filter_map(|field| {
                        self.value(&field.value, scope, env)
                            .map(|value| (field.name.clone(), value))
                    })
                    .collect();
                (!fields.is_empty()).then_some(Value {
                    fields,
                    ..Value::default()
                })
            }
            Expr::TypeConstructor { name, fields, .. }
                if !self.index.constructors.contains_key(name) && env.shadowed.contains(name) =>
            {
                let mut updated = self
                    .value(&Expr::Identifier(name.clone()), scope, env)
                    .unwrap_or_default();
                for field in fields {
                    if let Some(value) = self.value(&field.value, scope, env) {
                        let _ = updated.fields.insert(field.name.clone(), value);
                    } else {
                        let _ = updated.fields.remove(&field.name);
                    }
                }
                Some(updated)
            }
            Expr::TypeConstructor { fields, .. } => {
                let fields: BTreeMap<_, _> = fields
                    .iter()
                    .filter_map(|field| {
                        self.value(&field.value, scope, env)
                            .map(|value| (field.name.clone(), value))
                    })
                    .collect();
                (!fields.is_empty()).then_some(Value {
                    fields,
                    ..Value::default()
                })
            }
            Expr::Update { record, fields } => {
                let mut updated = self
                    .value(&Expr::Identifier(record.clone()), scope, env)
                    .unwrap_or_default();
                for field in fields {
                    if let Some(value) = self.value(&field.value, scope, env) {
                        let _ = updated.fields.insert(field.name.clone(), value);
                    } else {
                        let _ = updated.fields.remove(&field.name);
                    }
                }
                Some(updated)
            }
            Expr::Index { target, .. } => Some(Value {
                result_payload: Some(Box::new(
                    self.value(target, scope, env)
                        .and_then(project_element)
                        .unwrap_or_else(Value::unknown_callable),
                )),
                ..Value::default()
            }),
            Expr::Spawn(value) => Some(Value {
                fiber_payload: Some(Box::new(
                    self.value(value, scope, env)
                        .unwrap_or_else(Value::unknown_callable),
                )),
                ..Value::default()
            }),
            Expr::Await(fiber) => Some(
                self.value(fiber, scope, env)
                    .and_then(project_fiber_value)
                    .unwrap_or_else(Value::unknown_callable),
            ),
            Expr::Recv(channel) => Some(
                self.value(channel, scope, env)
                    .and_then(|channel| Self::channel_payload(channel, env))
                    .unwrap_or_else(Value::unknown_callable),
            ),
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } if expression_name(function) == Some("Channel")
                && !env.shadowed.contains("Channel") =>
            {
                Some(Value::channel(expression_site(expression)))
            }
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => self.call_value(function, arguments, named_arguments, scope, env),
            Expr::MethodCall {
                target,
                method,
                arguments,
                named_arguments,
            } => {
                let mut all_arguments = Vec::with_capacity(arguments.len() + 1);
                all_arguments.push((**target).clone());
                all_arguments.extend(arguments.iter().cloned());
                self.call_value(
                    &Expr::Identifier(method.clone()),
                    &all_arguments,
                    named_arguments,
                    scope,
                    env,
                )
            }
            Expr::Pipe { left, right } => self.pipe_value(left, right, scope, env),
            Expr::Match { value, arms } => {
                let matched = self.value(value, scope, env);
                let mut merged = None;
                for arm in arms {
                    let mut local = env.clone();
                    bind_pattern(&arm.pattern, matched.as_ref(), self.index, &mut local);
                    if let Some(value) = self.value(&arm.body, scope, &local) {
                        merge_optional_value(&mut merged, value);
                    }
                }
                merged
            }
            // A handler governs evaluation of its body, not later use of a
            // value produced by that body or one of its operation arms.
            Expr::Handler {
                effect, arms, body, ..
            } => {
                let mut merged = self.value(body, scope, env);
                for arm in arms {
                    let local = self.handler_arm_env(effect, arm, body, scope, env);
                    if let Some(value) = self.value(&arm.body, scope, &local) {
                        merge_optional_value(&mut merged, value);
                    }
                }
                merged
            }
            Expr::Block { statements, value } => {
                let mut local = env.clone();
                let _ = self.statements(statements, scope, &mut local);
                value
                    .as_deref()
                    .and_then(|value| self.value(value, scope, &local))
            }
            Expr::Resume(value) | Expr::Yield(value) => value
                .as_deref()
                .and_then(|value| self.value(value, scope, env)),
            Expr::Binary { left, right, .. } => {
                let mut merged = self.value(left, scope, env);
                if let Some(value) = self.value(right, scope, env) {
                    merge_optional_value(&mut merged, value);
                }
                merged
            }
            _ => None,
        }
    }

    fn function_value(&self, id: usize) -> Value {
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

    fn flow_callable_assignments(
        &self,
        expression: &Expr,
        scope: &[String],
        env: &mut CallableEnv,
    ) {
        match expression {
            Expr::Block { statements, value } => {
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
            Expr::Handler { arms, body, .. } => {
                for arm in arms {
                    self.flow_callable_assignments(&arm.body, scope, env);
                }
                self.flow_callable_assignments(body, scope, env);
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

    fn substitute_summary_at(
        &self,
        mut summary: Summary,
        target_level: usize,
        arguments: &[Option<Value>],
    ) -> Summary {
        let uses = std::mem::take(&mut summary.parameter_uses);
        for mut use_ in uses {
            if use_.level == target_level {
                let callback = arguments
                    .get(use_.index)
                    .and_then(Option::as_ref)
                    .cloned()
                    .and_then(|value| project_path(value, &use_.projection))
                    .and_then(|value| value.callable);
                if let Some(callback) = callback {
                    let mut invoked = self
                        .invoke_with_values(callback, &[])
                        .excluding(&use_.excluded);
                    shift_summary_levels(&mut invoked, target_level);
                    summary.union(invoked);
                } else if use_.index >= arguments.len() {
                    // No argument occupies this slot yet — the call is still
                    // under-applied, so the parameter stays symbolic for an
                    // enclosing scope to resolve. Keep its level: decrementing
                    // here is what re-attributes it to the wrong binder.
                    let _ = summary.parameter_uses.insert(use_);
                } else {
                    // An argument IS supplied but carries no callable
                    // provenance, so nothing proves what this slot will invoke.
                    // Re-attributing it to the caller's parameter at the same
                    // index — which `target_level.saturating_sub(1)` did, since
                    // a direct call has `target_level == 0` and 0 - 1 saturates
                    // back to 0 — hands the requirement to an unrelated binder,
                    // and any pure argument the caller passes in that slot then
                    // discharges an effect it never supplied. That is fail-OPEN
                    // and it breaks [EFFECTS-STATIC-DISCHARGE]: the program
                    // compiles and aborts at runtime with `unhandled effect`.
                    summary.unresolved_dynamic_call = true;
                }
            } else {
                if use_.level > target_level {
                    use_.level -= 1;
                }
                let _ = summary.parameter_uses.insert(use_);
            }
        }
        summary
    }

    fn substitute_value_at(
        &self,
        mut value: Value,
        target_level: usize,
        arguments: &[Option<Value>],
    ) -> Value {
        if let Some(callable) = value.callable.take() {
            match callable {
                Callable::Parameter {
                    mut level,
                    index,
                    projection,
                } if level == target_level => {
                    if let Some(mut replacement) = arguments
                        .get(index)
                        .and_then(Option::as_ref)
                        .cloned()
                        .and_then(|argument| project_path(argument, &projection))
                    {
                        shift_value_levels(&mut replacement, target_level, 0);
                        value.union(replacement);
                    } else {
                        level = target_level.saturating_sub(1);
                        value.callable = Some(Callable::Parameter {
                            level,
                            index,
                            projection,
                        });
                    }
                }
                Callable::Parameter {
                    mut level,
                    index,
                    projection,
                } => {
                    if level > target_level {
                        level -= 1;
                    }
                    value.callable = Some(Callable::Parameter {
                        level,
                        index,
                        projection,
                    });
                }
                Callable::Known(mut known) => {
                    known.summary =
                        self.substitute_summary_at(known.summary, target_level + 1, arguments);
                    known.returned = known.returned.map(|returned| {
                        Box::new(self.substitute_value_at(*returned, target_level + 1, arguments))
                    });
                    value.callable = Some(Callable::Known(known));
                }
                Callable::Unknown => value.callable = Some(Callable::Unknown),
            }
        }
        for nested in value.fields.values_mut() {
            *nested = self.substitute_value_at(nested.clone(), target_level, arguments);
        }
        if let Some(element) = value.element.take() {
            value.element = Some(Box::new(self.substitute_value_at(
                *element,
                target_level,
                arguments,
            )));
        }
        if let Some(success) = value.result_payload.take() {
            value.result_payload = Some(Box::new(self.substitute_value_at(
                *success,
                target_level,
                arguments,
            )));
        }
        if let Some(payload) = value.fiber_payload.take() {
            value.fiber_payload = Some(Box::new(self.substitute_value_at(
                *payload,
                target_level,
                arguments,
            )));
        }
        value.deferred = self.substitute_summary_at(value.deferred, target_level, arguments);
        value
    }

    fn handler_arm_env(
        &self,
        effect: &str,
        arm: &HandlerArm,
        body: &Expr,
        scope: &[String],
        env: &CallableEnv,
    ) -> CallableEnv {
        let mut arguments = vec![None; arm.params.len()];
        self.collect_operation_arguments(
            body,
            effect,
            &arm.operation,
            &arm.params,
            scope,
            env,
            &mut arguments,
        );
        let mut local = env.clone();
        for (index, parameter) in arm.params.iter().enumerate() {
            let _ = local.shadowed.insert(parameter.clone());
            if let Some(value) = arguments.get(index).cloned().flatten() {
                let _ = local.values.insert(parameter.clone(), value);
            } else {
                let _ = local.values.remove(parameter);
            }
        }
        local
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "operation payload provenance needs the lexical effect, arm, and value environment"
    )]
    fn collect_operation_arguments(
        &self,
        expression: &Expr,
        effect: &str,
        operation: &str,
        parameters: &[String],
        scope: &[String],
        env: &CallableEnv,
        out: &mut [Option<Value>],
    ) {
        if let Expr::Perform {
            effect: performed_effect,
            operation: performed_operation,
            arguments,
            named_arguments,
            ..
        } = expression
        {
            if performed_effect == effect && performed_operation == operation {
                for (index, argument) in arguments.iter().enumerate() {
                    if let (Some(slot), Some(value)) =
                        (out.get_mut(index), self.value(argument, scope, env))
                    {
                        merge_optional_value(slot, value);
                    }
                }
                for argument in named_arguments {
                    if let Some(index) = parameters.iter().position(|name| name == &argument.name) {
                        if let (Some(slot), Some(value)) =
                            (out.get_mut(index), self.value(&argument.value, scope, env))
                        {
                            merge_optional_value(slot, value);
                        }
                    }
                }
            }
        }
        walk_children(expression, |child| {
            self.collect_operation_arguments(child, effect, operation, parameters, scope, env, out);
        });
    }

    fn statements(&self, statements: &[Stmt], scope: &[String], env: &mut CallableEnv) -> Summary {
        let mut out = Summary::default();
        for statement in statements {
            match statement {
                Stmt::Let { name, value, .. } => {
                    out.union(self.expression(value, scope, env));
                    self.flow_callable_assignments(value, scope, env);
                    let _ = env.shadowed.insert(name.clone());
                    if let Some(provenance) = self.value(value, scope, env) {
                        let _ = env.values.insert(name.clone(), provenance);
                    } else {
                        let _ = env.values.remove(name);
                    }
                }
                Stmt::Assignment { name, value, .. } => {
                    out.union(self.expression(value, scope, env));
                    self.flow_callable_assignments(value, scope, env);
                    if let Some(provenance) = self.value(value, scope, env) {
                        let _ = env.values.insert(name.clone(), provenance);
                    } else {
                        let _ = env.values.remove(name);
                    }
                }
                Stmt::Expr { value, .. } => {
                    out.union(self.expression(value, scope, env));
                    self.flow_callable_assignments(value, scope, env);
                }
                _ => {}
            }
        }
        out
    }

    fn expressions(&self, expressions: &[Expr], scope: &[String], env: &CallableEnv) -> Summary {
        let mut out = Summary::default();
        for expression in expressions {
            out.union(self.expression(expression, scope, env));
        }
        out
    }

    fn named_expressions(
        &self,
        arguments: &[NamedArgument],
        scope: &[String],
        env: &CallableEnv,
    ) -> Summary {
        let mut out = Summary::default();
        for argument in arguments {
            out.union(self.expression(&argument.value, scope, env));
        }
        out
    }
}

fn expression_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Identifier(name) => Some(name),
        Expr::Path(path) => path.last(),
        _ => None,
    }
}

/// Whether a callee this pass could not resolve is nonetheless known to be
/// effect-free, so the call may be skipped instead of failing closed.
///
/// Only a built-in earns that. Reaching here with an identifier means it named
/// neither a tracked value nor a function in the index, and the remaining
/// candidates are a built-in or a binding this pass cannot see — notably a
/// top-level `let`, which `function_body` never seeds into the environment.
/// Trusting every unshadowed name let `let siren = ring` / `fn relay() =
/// siren()` type-check with `Alarm.ring` never handled, because the call
/// contributed nothing at all, not even a provenance failure.
fn statically_named_callee(expression: &Expr, env: &CallableEnv, index: &Index) -> bool {
    match expression {
        Expr::Identifier(name) => {
            !env.shadowed.contains(name)
                && (crate::builtins::builtin_signature(name).is_some()
                    || index.externs.contains(name))
        }
        Expr::Path(_) => true,
        _ => false,
    }
}

fn expression_site(expression: &Expr) -> usize {
    std::ptr::from_ref(expression).addr()
}

fn eager_callback_slots(name: &str) -> &'static [usize] {
    match name {
        "test" | "forEach" | "forEachList" | "httpListen" | "spawnProcess" | "mapList"
        | "filterList" => &[1],
        "fold" | "foldList" => &[2],
        _ => &[],
    }
}

fn iterator_consumer(name: &str) -> bool {
    matches!(name, "forEach" | "fold")
}

fn site_arguments(
    position: Option<Position>,
    sites: &HashMap<(u32, u32), Vec<String>>,
) -> Option<Vec<String>> {
    position
        .and_then(|position| sites.get(&(position.line, position.column)))
        .cloned()
}

fn requirement_name(requirement: &Requirement) -> String {
    if requirement.arguments.is_empty() {
        format!("{}.{}", requirement.effect, requirement.operation)
    } else {
        format!(
            "{}<{}>.{}",
            requirement.effect,
            requirement.arguments.join(", "),
            requirement.operation
        )
    }
}

fn project_field(mut value: Value, field: &str) -> Option<Value> {
    let mut projected = value.fields.remove(field);
    project_callable(
        value.callable,
        Projection::Field(field.to_string()),
        &mut projected,
    );
    projected
}

fn project_element(mut value: Value) -> Option<Value> {
    let mut projected = value.element.take().map(|value| *value);
    project_callable(value.callable, Projection::Element, &mut projected);
    projected
}

fn project_success_value(mut value: Value) -> Option<Value> {
    let mut projected = value.result_payload.take().map(|value| *value);
    if let Some(field) = value.fields.remove("value") {
        merge_optional_value(&mut projected, field);
    }
    project_callable(value.callable, Projection::SuccessValue, &mut projected);
    projected
}

fn project_fiber_value(mut value: Value) -> Option<Value> {
    let mut projected = value.fiber_payload.take().map(|value| *value);
    project_callable(value.callable, Projection::FiberValue, &mut projected);
    projected
}

fn project_callable(callable: Option<Callable>, next: Projection, projected: &mut Option<Value>) {
    let callable = match callable {
        Some(Callable::Parameter {
            level,
            index,
            mut projection,
        }) => {
            projection.push(next);
            Callable::Parameter {
                level,
                index,
                projection,
            }
        }
        Some(Callable::Unknown) => Callable::Unknown,
        Some(Callable::Known(_)) | None => return,
    };
    merge_optional_value(projected, Value::from_callable(callable));
}

fn project_path(mut value: Value, path: &[Projection]) -> Option<Value> {
    for projection in path {
        value = match projection {
            Projection::Field(field) => project_field(value, field),
            Projection::Element => project_element(value),
            Projection::SuccessValue => project_success_value(value),
            Projection::FiberValue => project_fiber_value(value),
        }?;
    }
    Some(value)
}

fn merge_value(slot: &mut Value, incoming: Value) {
    slot.union(incoming);
}

fn merge_optional_value(slot: &mut Option<Value>, incoming: Value) {
    if let Some(value) = slot {
        value.union(incoming);
    } else {
        *slot = Some(incoming);
    }
}

fn merge_boxed_value(slot: &mut Option<Box<Value>>, incoming: Value) {
    if let Some(value) = slot {
        value.union(incoming);
    } else {
        *slot = Some(Box::new(incoming));
    }
}

fn widen_value(mut value: Value, depth: usize) -> Value {
    if depth >= MAX_PROVENANCE_DEPTH {
        // Abandon the provenance TREE below this height, but keep what is
        // already known, because a value only reaches this depth through
        // widening rather than by being genuinely opaque.
        //
        // What grows without bound is the `returned` chain: an ML definition of
        // arity > 1 lowers to nested lambdas, so a recursive `f` returns a
        // closure naming `f` and each fixed-point iteration nests one more
        // layer. A callable's `summary` does not grow that way — `required` and
        // `parameter_uses` are flat sets that converge — so truncating only
        // `returned` terminates the iteration while keeping the callable
        // invokable with its real effects. Discarding the callable instead
        // would make every recursive curried ML function report as an
        // unresolvable dynamic call, and discarding `deferred.required` would
        // silently discharge an operation that still needs a handler.
        let mut deferred = value.deferred;
        deferred.widen();
        let callable = match value.callable {
            Some(Callable::Known(mut known)) => {
                known.returned = None;
                known.summary.widen();
                Some(Callable::Known(known))
            }
            // A value that was never callable must not BECOME one here.
            // Promoting `None` to `Unknown` would turn "this is an int" into
            // "this is a callable of unknown provenance", and every merge it
            // reaches would inherit that verdict.
            None => None,
            Some(other) => Some(other),
        };
        return Value {
            callable,
            deferred,
            ..Value::default()
        };
    }

    value.deferred.widen();
    if let Some(callable) = value.callable.take() {
        value.callable = Some(match callable {
            Callable::Known(mut known) => {
                known.summary.widen();
                known.returned = known
                    .returned
                    .map(|returned| Box::new(widen_value(*returned, depth + 1)));
                Callable::Known(known)
            }
            Callable::Parameter {
                level,
                index,
                projection,
            } if level < MAX_PROVENANCE_DEPTH && projection.len() < MAX_PROVENANCE_DEPTH => {
                Callable::Parameter {
                    level,
                    index,
                    projection,
                }
            }
            Callable::Parameter { .. } | Callable::Unknown => {
                value.deferred.unresolved_dynamic_call = true;
                Callable::Unknown
            }
        });
    }
    value.fields = value
        .fields
        .into_iter()
        .map(|(name, nested)| (name, widen_value(nested, depth + 1)))
        .collect();
    value.element = value
        .element
        .map(|nested| Box::new(widen_value(*nested, depth + 1)));
    value.result_payload = value
        .result_payload
        .map(|nested| Box::new(widen_value(*nested, depth + 1)));
    value.fiber_payload = value
        .fiber_payload
        .map(|nested| Box::new(widen_value(*nested, depth + 1)));
    value
}

fn shift_summary_from(summary: &mut Summary, amount: usize, minimum_level: usize) {
    summary.parameter_uses = summary
        .parameter_uses
        .iter()
        .cloned()
        .map(|mut use_| {
            if use_.level >= minimum_level {
                use_.level = use_.level.saturating_add(amount);
            }
            use_
        })
        .collect();
}

fn shift_summary_levels(summary: &mut Summary, amount: usize) {
    shift_summary_from(summary, amount, 0);
}

fn shift_value_levels(value: &mut Value, amount: usize, minimum_level: usize) {
    if let Some(callable) = &mut value.callable {
        match callable {
            Callable::Parameter { level, .. } => {
                if *level >= minimum_level {
                    *level = level.saturating_add(amount);
                }
            }
            Callable::Known(known) => {
                shift_summary_from(&mut known.summary, amount, minimum_level + 1);
                if let Some(returned) = &mut known.returned {
                    shift_value_levels(returned, amount, minimum_level + 1);
                }
            }
            Callable::Unknown => {}
        }
    }
    for nested in value.fields.values_mut() {
        shift_value_levels(nested, amount, minimum_level);
    }
    if let Some(element) = &mut value.element {
        shift_value_levels(element, amount, minimum_level);
    }
    if let Some(success) = &mut value.result_payload {
        shift_value_levels(success, amount, minimum_level);
    }
    if let Some(payload) = &mut value.fiber_payload {
        shift_value_levels(payload, amount, minimum_level);
    }
    shift_summary_from(&mut value.deferred, amount, minimum_level);
}

fn callable_summary(callable: &Callable) -> Summary {
    match callable {
        Callable::Known(known) => known.summary.clone(),
        Callable::Unknown => Summary {
            unresolved_dynamic_call: true,
            ..Summary::default()
        },
        Callable::Parameter {
            level,
            index,
            projection,
        } => Summary {
            required: Requirements::new(),
            parameter_uses: [ParameterUse {
                level: *level,
                index: *index,
                projection: projection.clone(),
                excluded: Requirements::new(),
            }]
            .into_iter()
            .collect(),
            unresolved_dynamic_call: false,
        },
    }
}

fn merge_callable(slot: &mut Option<Callable>, incoming: Callable) {
    let Some(existing) = slot.take() else {
        *slot = Some(incoming);
        return;
    };
    if existing == incoming {
        *slot = Some(existing);
        return;
    }
    let mut summary = callable_summary(&existing);
    summary.union(callable_summary(&incoming));
    let mut parameters = Vec::new();
    let mut returned = None;
    for callable in [&existing, &incoming] {
        if let Callable::Known(known) = callable {
            if parameters.is_empty() || parameters == known.parameters {
                parameters.clone_from(&known.parameters);
            }
            if let Some(value) = known.returned.as_deref().cloned() {
                merge_optional_value(&mut returned, value);
            }
        }
    }
    *slot = Some(Callable::Known(Box::new(KnownCallable {
        parameters,
        summary,
        returned: returned.map(Box::new),
    })));
}

fn bind_pattern(pattern: &Pattern, value: Option<&Value>, index: &Index, env: &mut CallableEnv) {
    match pattern {
        Pattern::Binding(name) | Pattern::TypeAnnotated { name, .. } => {
            let _ = env.shadowed.insert(name.clone());
            if let Some(value) = value {
                let _ = env.values.insert(name.clone(), value.clone());
            } else {
                let _ = env.values.remove(name);
            }
        }
        Pattern::Constructor {
            name,
            fields,
            sub_patterns,
        } => {
            for field in fields {
                let _ = env.shadowed.insert(field.clone());
                let projected = value.and_then(|value| {
                    if name == "Success" && field == "value" {
                        project_success_value(value.clone())
                    } else {
                        value.fields.get(field).cloned()
                    }
                });
                if let Some(projected) = projected {
                    let _ = env.values.insert(field.clone(), projected);
                } else {
                    let _ = env.values.remove(field);
                }
            }
            for (position, sub_pattern) in sub_patterns.iter().enumerate() {
                let projected = value.and_then(|value| {
                    index
                        .constructors
                        .get(name)
                        .and_then(|fields| fields.get(position))
                        .and_then(|field| value.fields.get(field))
                });
                bind_pattern(sub_pattern, projected, index, env);
            }
        }
        Pattern::Structural { fields } => {
            for field in fields {
                let _ = env.shadowed.insert(field.clone());
                if let Some(projected) = value.and_then(|value| value.fields.get(field)) {
                    let _ = env.values.insert(field.clone(), projected.clone());
                } else {
                    let _ = env.values.remove(field);
                }
            }
        }
        Pattern::List { elements, rest } => {
            let element = value.and_then(|value| value.element.as_deref());
            for pattern in elements {
                bind_pattern(pattern, element, index, env);
            }
            if let Some(rest) = rest {
                let _ = env.shadowed.insert(rest.clone());
                if let Some(value) = value {
                    let _ = env.values.insert(rest.clone(), value.clone());
                }
            }
        }
        Pattern::Wildcard | Pattern::Literal(_) => {}
    }
}

/// Check inferred rows and entry discharge. Implements
/// [EFFECTS-STATIC-DISCHARGE].
#[expect(
    clippy::too_many_lines,
    reason = "the fixed point, contract validation, and entry proof form one ordered checker pass"
)]
pub(crate) fn check(program: &Program, instances: &Instances) -> Vec<TypeError> {
    let index = Index::collect(program);
    let mut errors = Vec::new();
    for function in &index.functions {
        let unknown: BTreeSet<_> = function
            .declared_effects
            .iter()
            .filter(|effect| !index.effects.contains_key(&effect.name))
            .map(|effect| effect.name.clone())
            .collect();
        if !unknown.is_empty() {
            errors.push(
                TypeError::new(format!(
                    "function `{}` declares unknown effects: {}",
                    function.qualified,
                    unknown.into_iter().collect::<Vec<_>>().join(", ")
                ))
                .with_pos(function.position),
            );
        }
    }
    // Programs with no declared algebraic effects cannot have a valid latent
    // row. Returning here avoids a second deep recursive AST walk for huge pure
    // expression stress suites; unknown performs are already value-type errors.
    if index.effects.is_empty() {
        return errors;
    }
    let mut rows = vec![Summary::default(); index.functions.len()];
    let mut returns = vec![None; index.functions.len()];

    // Least fixed point: recursive and forward calls only add requirements.
    loop {
        let analyzer = Analyzer {
            index: &index,
            rows: &rows,
            returns: &returns,
            instances,
        };
        let mut changed = false;
        let mut next = rows.clone();
        let mut next_returns = returns.clone();
        for (id, function) in index.functions.iter().enumerate() {
            let mut actual = analyzer.function_body(function);
            actual.widen();
            if let Some(row) = next.get_mut(id) {
                let before = row.clone();
                // Requirements and parameter uses only grow toward the least
                // fixed point, so unioning them is what makes this converge.
                // Provenance is not of that kind: it is a property of the
                // CONVERGED row. A self-call reads `returns[id] == None` on the
                // first iteration and resolves to `Callable::Unknown`, so
                // folding that transient verdict in with `|=` would leave every
                // recursive function permanently unresolved. Take the freshly
                // computed verdict instead — the loop only exits once nothing
                // changed, so the surviving one was derived from converged
                // rows. `widen` still re-raises it when it drops a use.
                let resolved = actual.unresolved_dynamic_call;
                row.union(actual);
                row.unresolved_dynamic_call = resolved;
                row.widen();
                changed |= *row != before;
            }
            if let Some(returned) = analyzer.function_return(function) {
                if let Some(slot) = next_returns.get_mut(id) {
                    let before = slot.clone();
                    // Recompute rather than merge with the previous iteration.
                    // `function_return` already merges every return path of the
                    // body, so each pass yields a COMPLETE answer for the rows
                    // it was given; unioning across passes only preserves
                    // superseded ones. That distinction is load-bearing: on the
                    // first pass a recursive call reads `returns[id] == None`
                    // and evaluates to `Callable::Unknown`, and `merge_callable`
                    // folds `callable_summary(Unknown)` — which carries
                    // `unresolved_dynamic_call` — into the merged callable. Kept
                    // by union, that transient verdict outlives the fixed point
                    // and condemns every recursive curried ML function.
                    *slot = Some(returned.widened().widened());
                    changed |= *slot != before;
                }
            }
        }
        rows = next;
        returns = next_returns;
        if !changed {
            break;
        }
    }

    let analyzer = Analyzer {
        index: &index,
        rows: &rows,
        returns: &returns,
        instances,
    };
    // An annotation is a row contract/instantiation hint, never a handler.
    // Inferred operations outside its named effects are therefore an error.
    for (id, function) in index.functions.iter().enumerate() {
        if !function.declared_effects.is_empty() {
            let undeclared: BTreeSet<_> = rows
                .get(id)
                .into_iter()
                .flat_map(|row| &row.required)
                .filter(|requirement| {
                    !function.declared_effects.iter().any(|declared| {
                        declared.name == requirement.effect
                            && declared
                                .arguments
                                .as_ref()
                                .is_none_or(|arguments| arguments == &requirement.arguments)
                    })
                })
                .map(requirement_name)
                .collect();
            if !undeclared.is_empty() {
                errors.push(
                    TypeError::new(format!(
                        "function `{}` performs effects outside its declared row: {}",
                        function.qualified,
                        undeclared.into_iter().collect::<Vec<_>>().join(", ")
                    ))
                    .with_pos(function.position),
                );
            }
        }
        validate_handler_arms(
            &analyzer,
            &function.body,
            &function.scope,
            &CallableEnv::for_parameters(&function.parameters),
            &mut errors,
        );
    }
    let mut top_level_env = CallableEnv::default();
    validate_statement_handlers(
        &analyzer,
        &program.statements,
        &[],
        &mut top_level_env,
        &mut errors,
    );

    // Codegen executes a user `main` instead of top-level statements. Without
    // one, the top-level let/assignment/expression sequence is the entry.
    let entry = index
        .functions
        .iter()
        .position(|function| function.scope.is_empty() && function.name == "main")
        .map_or_else(
            || {
                let mut env = CallableEnv::default();
                analyzer.statements(&program.statements, &[], &mut env)
            },
            |main| rows.get(main).cloned().unwrap_or_default(),
        );
    if !entry.required.is_empty() {
        let operations = entry
            .required
            .iter()
            .map(requirement_name)
            .collect::<Vec<_>>()
            .join(", ");
        errors.push(TypeError::new(format!(
            "unhandled effect operations at program entry: {operations}; add a matching `handle`"
        )));
    }
    if !entry.parameter_uses.is_empty() {
        errors.push(TypeError::new(
            "program entry invokes an effect-polymorphic callback whose effects cannot be discharged",
        ));
    }
    if entry.unresolved_dynamic_call {
        errors.push(TypeError::new(
            "program entry invokes a dynamic callable whose effect provenance cannot be proven; preserve the callable through a statically tracked value path",
        ));
    }
    errors
}

fn validate_statement_handlers(
    analyzer: &Analyzer<'_>,
    statements: &[Stmt],
    scope: &[String],
    env: &mut CallableEnv,
    errors: &mut Vec<TypeError>,
) {
    for statement in statements {
        let value = match statement {
            Stmt::Let { value, .. } | Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
                Some(value)
            }
            _ => None,
        };
        if let Some(value) = value {
            validate_handler_arms(analyzer, value, scope, env, errors);
        }
        match statement {
            Stmt::Let { name, value, .. } | Stmt::Assignment { name, value, .. } => {
                let _ = env.shadowed.insert(name.clone());
                if let Some(provenance) = analyzer.value(value, scope, env) {
                    let _ = env.values.insert(name.clone(), provenance);
                } else {
                    let _ = env.values.remove(name);
                }
            }
            _ => {}
        }
    }
}

fn validate_handler_arms(
    analyzer: &Analyzer<'_>,
    expression: &Expr,
    scope: &[String],
    env: &CallableEnv,
    errors: &mut Vec<TypeError>,
) {
    if let Expr::Handler {
        effect,
        arms,
        body,
        position,
    } = expression
    {
        let handler_arguments =
            analyzer.instance_arguments(effect, *position, &analyzer.instances.handlers, "handler");
        for arm in arms {
            let local = analyzer.handler_arm_env(effect, arm, body, scope, env);
            let row = analyzer.expression(&arm.body, scope, &local);
            if row.required.iter().any(|requirement| {
                requirement.effect == *effect
                    && requirement.operation == arm.operation
                    && requirement.arguments == handler_arguments
            }) {
                errors.push(
                    TypeError::new(format!(
                        "handler arm `{effect}.{}` performs `{effect}` while that handler is active; this would recursively re-enter the same handler",
                        arm.operation
                    ))
                    .with_pos(arm.position),
                );
            }
            validate_handler_arms(analyzer, &arm.body, scope, &local, errors);
        }
        validate_handler_arms(analyzer, body, scope, env, errors);
        return;
    }
    walk_children(expression, |child| {
        validate_handler_arms(analyzer, child, scope, env, errors);
    });
}

#[expect(
    clippy::too_many_lines,
    reason = "the child visitor deliberately exhausts every AST expression variant"
)]
fn walk_children<'a>(expression: &'a Expr, mut visit: impl FnMut(&'a Expr)) {
    match expression {
        Expr::InterpolatedStr(parts) => {
            for part in parts {
                if let InterpolatedPart::Expr(expression) = part {
                    visit(expression);
                }
            }
        }
        Expr::List(items) => items.iter().for_each(&mut visit),
        Expr::Map(entries) => {
            for entry in entries {
                visit(&entry.key);
                visit(&entry.value);
            }
        }
        Expr::Object(fields)
        | Expr::TypeConstructor { fields, .. }
        | Expr::Update { fields, .. } => {
            for field in fields {
                visit(&field.value);
            }
        }
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            visit(left);
            visit(right);
        }
        Expr::Unary { operand, .. }
        | Expr::FieldAccess {
            target: operand, ..
        }
        | Expr::Spawn(operand)
        | Expr::Await(operand)
        | Expr::Recv(operand) => visit(operand),
        Expr::Yield(value) | Expr::Resume(value) => {
            if let Some(value) = value {
                visit(value);
            }
        }
        Expr::Send { channel, value }
        | Expr::Index {
            target: channel,
            index: value,
        } => {
            visit(channel);
            visit(value);
        }
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => {
            visit(function);
            arguments.iter().for_each(&mut visit);
            for argument in named_arguments {
                visit(&argument.value);
            }
        }
        Expr::MethodCall {
            target,
            arguments,
            named_arguments,
            ..
        } => {
            visit(target);
            arguments.iter().for_each(&mut visit);
            for argument in named_arguments {
                visit(&argument.value);
            }
        }
        Expr::Lambda { body, .. } => visit(body),
        Expr::Match { value, arms } => {
            visit(value);
            for arm in arms {
                visit(&arm.body);
            }
        }
        Expr::Block { statements, value } => {
            for statement in statements {
                match statement {
                    Stmt::Let { value, .. }
                    | Stmt::Assignment { value, .. }
                    | Stmt::Expr { value, .. } => visit(value),
                    _ => {}
                }
            }
            if let Some(value) = value {
                visit(value);
            }
        }
        Expr::Select { arms } => {
            for arm in arms {
                visit(&arm.body);
            }
        }
        Expr::Perform {
            arguments,
            named_arguments,
            ..
        } => {
            arguments.iter().for_each(&mut visit);
            for argument in named_arguments {
                visit(&argument.value);
            }
        }
        Expr::Handler { arms, body, .. } => {
            for arm in arms {
                visit(&arm.body);
            }
            visit(body);
        }
        Expr::Integer(_)
        | Expr::Float(_)
        | Expr::Str(_)
        | Expr::Bool(_)
        | Expr::Identifier(_)
        | Expr::Path(_) => {}
    }
}
