//! Static algebraic-effect discharge.
//!
//! Value inference predates effect rows and deliberately keeps their runtime
//! instantiation machinery separate. This pass computes the latent operation
//! requirements of function bodies, propagates them through calls, discharges
//! only the operations supplied by a handler, and proves the selected program
//! entry is pure. Operation-level requirements are essential: handlers are
//! partial, so an inner arm for `Policy.score` must not swallow a
//! `Policy.label` that belongs to an outer handler.

mod analyze;
mod call_values;
mod calls;
mod environment;
mod handlers;
#[cfg(test)]
mod identity_tests;
mod index;
mod methods;
mod model_values;
mod names;
mod projections;
mod statements;
mod substitute;
mod summary;
mod value_flow;
mod values;
use names::{
    eager_callback_slots, expression_name, expression_site, iterator_consumer, requirement_name,
    site_arguments, source_named_callee, statically_named_callee, type_can_call,
};
mod value_shapes;
use value_shapes::{
    builtin_callable_value, builtin_return_shape, method_thunk, project_callable, project_element,
    project_fiber_value, project_field, project_success_value,
};
mod provenance;
use provenance::{
    map_parameter_use_values, map_projection_values, merge_boxed_value, merge_callable,
    merge_optional_value, merge_value, ordered_values, shift_summary_levels, shift_value_levels,
    widen_value, widen_value_budget,
};
mod patterns;
use patterns::bind_pattern;
mod solver;
use solver::{clear_verdicts, converge, file_scope_env, full_application};
mod specialize;
use specialize::{specialize_argument, specialize_summary, specialize_value};
mod check;
pub(crate) use check::check;
mod validate;
use validate::{entry_errors, validate_arithmetic_initializers, validate_statement_handlers};
mod gpu;
use gpu::{gpu_kernel_slot, validate_gpu_kernel};
mod validate_handlers;
use validate_handlers::validate_handler_arms;
mod reachability;
use reachability::{fibered_operation, operation_pairs, walk_children};

use crate::error::TypeError;
use osprey_ast::{
    Expr, FieldAssignment, HandlerArm, InterpolatedPart, ModuleItem, NamedArgument, Pattern,
    Position, Program, Stmt,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

type HandlerCandidates = HashMap<(u32, u32), BTreeSet<Vec<String>>>;

#[derive(Default)]
pub(crate) struct Instances {
    pub(crate) expression_types: RefCell<HashMap<usize, crate::ty::Type>>,
    pub(crate) methods: crate::methods::Targets,
    /// A type error already proved a call target is not callable. It cannot
    /// also be an effectful dynamic callable in the accepted-program proof.
    pub(crate) non_callable_call_error: bool,
    /// A source position can identify more than one interpolation fragment.
    /// Preserve every independently inferred candidate instead of letting the
    /// final fragment overwrite the others.
    pub(crate) performs: HashMap<(u32, u32), Vec<Vec<String>>>,
    pub(crate) handlers: HashMap<(u32, u32), Vec<String>>,
    /// Explicit function-row arguments in source order, canonicalized by the
    /// type checker so a nominal record and its inferred row share an identity.
    pub(crate) declared_rows: HashMap<(u32, u32), Vec<Option<Vec<String>>>>,
    /// Substituted operation arguments available to refine a handler.
    pub(crate) argument_types: HashMap<Vec<String>, Vec<crate::ty::Type>>,
    /// Candidate instantiations required by each handler's body after calls
    /// and callbacks have propagated through the closed-program summary.
    pub(crate) handler_inference: RefCell<HandlerCandidates>,
    pub(crate) instantiations: HashMap<usize, HashMap<String, String>>,
    pub(crate) binders: HashMap<String, HashMap<String, String>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Requirement {
    effect: String,
    operation: String,
    arguments: Vec<String>,
}

type Requirements = BTreeSet<Requirement>;

/// Provenance is an abstract value, not a runtime value tree. Recursive
/// closures and aggregates grow across fixed-point iterations. Bound both
/// path depth and branching size; lost nested projections stay unknown.
/// Implements [EFFECTS-PROVENANCE].
const MAX_PROVENANCE_DEPTH: usize = 32;
const MAX_PROVENANCE_NODES: usize = 256;

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
    arguments: CallArguments,
    excluded: Requirements,
}

/// A callable can be stored below a record field or collection element. Keep
/// that access path symbolic while the aggregate itself is a function
/// parameter, then resolve it against the concrete argument at the call site.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Projection {
    Field(String),
    Method(Box<MethodProjection>),
    Returned(Box<CallArguments>),
    /// Resolve a callback's operation result after that callback is invoked
    /// inside a handler. The callback was constructed outside this scope, so
    /// its result cannot be resolved until its actual call is projected.
    Handled(BTreeMap<Requirement, Value>),
    Element,
    SuccessValue,
    FiberValue,
}

/// A method on a symbolic receiver cannot select its UFCS fallback until the
/// receiver is substituted. The fallback is a zero-argument closure so its
/// effects and returned value keep the same lexical scope as the field call.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct MethodProjection {
    field: String,
    arguments: Vec<Option<Value>>,
    named: Vec<(String, Option<Value>)>,
    fallback: Value,
}

/// Arguments belong to the caller until a symbolic callee is substituted.
/// Keep their provenance so a callback returning an argument or captured value
/// resolves to that value, rather than to the callback itself.
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct CallArguments {
    positional: Vec<Option<Value>>,
    named: Vec<(String, Option<Value>)>,
}

#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct Summary {
    required: Requirements,
    runtime_builtins: BTreeSet<String>,
    parameter_uses: BTreeSet<ParameterUse>,
    unresolved_dynamic_call: bool,
}

#[derive(Clone)]
struct DeclaredEffect {
    name: String,
    /// `None` is an uninstantiated/wildcard row entry; `Some` is an exact
    /// generic instance contract.
    arguments: Option<Vec<String>>,
}

#[derive(Clone)]
struct Function<'a> {
    name: String,
    qualified: String,
    scope: Vec<String>,
    parameters: Vec<String>,
    declared_effects: Vec<DeclaredEffect>,
    effect_tail: Option<String>,
    effect_row_present: bool,
    body: &'a Expr,
    position: Option<Position>,
}

#[derive(Default)]
struct Index<'a> {
    operations: osprey_ast::OperationTable,
    functions: Vec<Function<'a>>,
    qualified: HashMap<String, usize>,
    bare: HashMap<String, Vec<usize>>,
    effects: HashMap<String, usize>,
    constructors: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct KnownCallable {
    parameters: Vec<String>,
    summary: Summary,
    returned: Option<Box<Value>>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
#[derive(Clone, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
struct Value {
    callable: Option<Callable>,
    /// The result of a perform whose provider is selected only when its
    /// enclosing callback runs. This is value provenance, not a requirement
    /// discharge: the ordinary effect summary still tracks the operation.
    performed: Requirements,
    /// `Some` proves which fields exist, even when their values have no
    /// callable provenance. `None` must never justify choosing a free function.
    field_names: Option<BTreeSet<String>>,
    fields: BTreeMap<String, Value>,
    element: Option<Box<Value>>,
    result_payload: Option<Box<Value>>,
    fiber_payload: Option<Box<Value>>,
    channel_sites: BTreeSet<usize>,
    deferred: Summary,
}

#[derive(Clone, Default)]
struct CallableEnv {
    values: HashMap<String, Value>,
    shadowed: HashSet<String>,
    channel_payloads: BTreeMap<usize, Value>,
    /// Results supplied by handlers active while this expression evaluates.
    /// These are dynamic bindings, never captures of a newly created closure.
    handler_returns: BTreeMap<Requirement, Value>,
    /// `(effect, operation)` pairs an enclosing compile-time handler answers.
    /// Discharge rewrites those requests away before the residual program
    /// exists, so they are never dynamic requirements of the code they sit in.
    static_answers: HashSet<(String, String)>,
}

struct Analyzer<'a> {
    index: &'a Index<'a>,
    rows: &'a [Summary],
    returns: &'a [Option<Value>],
    instances: &'a Instances,
    /// Provenance the file-scope statements establish, seeded underneath every
    /// function body. Without it a callee named by a top-level `let` resolves
    /// to nothing and the provenance verdict fails closed — see
    /// [`statically_named_callee`].
    file_scope: CallableEnv,
}
