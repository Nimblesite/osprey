//! The type checker driver: a two-pass walk over a [`Program`]. Pass one
//! collects every top-level declaration (types + their constructors, effects,
//! externs, function signatures) so forward references and recursion resolve.
//! Pass two infers each function body and top-level statement, unifying against
//! the declared signatures, then resolves everything against the final
//! substitution.

mod bindings;
mod collect;
mod declarations;
mod functions;
mod initialize;
mod obligations;
mod signatures;
mod statements;
use bindings::returns_expansive_handler_value;
mod driver;
mod validation;
pub use driver::check_program;
pub use driver::check_program_exports;
pub(crate) use driver::infer_checked;
pub use driver::infer_program;
mod publish;
use publish::publish_program;
mod instances;
use instances::effect_instances;
mod effect_keys;
use effect_keys::display_effect_arguments;
use effect_keys::effect_keys;
use effect_keys::effect_type_key;
use effect_keys::resolved_binders;
use effect_keys::resolved_declared_rows;
use effect_keys::resolved_effect_keys;
mod refine;
use refine::refine_handler_arguments;
mod sites;
use sites::collect_site_candidates;
use sites::dedupe_sites;
use sites::erased_type_params;
pub use sites::erased_var;
use sites::push_site_candidate;
use sites::resolve_op;
use sites::resolve_positioned;
use sites::type_is_resolved;
#[cfg(test)]
mod tests;

use crate::builtins::base_env;
use crate::convert::{parse_fn_sig, type_expr_to_type, type_name_to_type};
use crate::ctx::InferCtx;
use crate::env::{generalize, TypeEnv};
use crate::error::TypeError;
use crate::ty::{names, Scheme, Type};
use crate::unify::{unify, unify_assignable};
use crate::VarId;
use osprey_ast::{
    EffectOperation, EffectRef, Expr, ExternParameter, Parameter, Position, Program, Stmt,
    TypeExpr, TypeParam, TypeVariant, Variance,
};
use std::collections::BTreeSet;
use std::collections::{HashMap, HashSet};

/// The shared `name` accessor of the two AST parameter node types, so
/// [`Checker::record_fn_params`] handles both without duplication.
trait ParamName {
    fn param_name(&self) -> &str;
}

impl ParamName for Parameter {
    fn param_name(&self) -> &str {
        &self.name
    }
}

impl ParamName for ExternParameter {
    fn param_name(&self) -> &str {
        &self.name
    }
}

/// A constructor (record builder, union variant, or built-in `Success`/`Error`).
pub(crate) struct CtorInfo {
    pub owner: String,
    pub owner_is_record: bool,
    pub type_params: Vec<String>,
    /// (field name, field type as written).
    pub fields: Vec<(String, String)>,
}

/// Built-in obligations split by whether they rest on a scheme's quantified
/// variables: `(deferred to each instantiation, checked here)`.
type SplitObligations = (Vec<(String, Type)>, Vec<(String, Type)>);

/// A constructor instantiated against fresh type arguments:
/// (owner type arguments, instantiated `(field, type)` pairs, owner name,
/// whether the owner is a record).
pub(crate) type CtorInstance = (Vec<Type>, Vec<(String, Type)>, String, bool);

/// One in-scope effect instantiation: the effect's name, its resolved type
/// arguments, and the operations instantiated at them.
pub(crate) struct EffectScope {
    pub name: String,
    pub args: Vec<Type>,
    pub ops: HashMap<String, crate::info::OpType>,
}

/// A declared effect, stored generically: its type parameters plus each
/// operation's written signature, instantiated per handle site / effect-row
/// entry. Implements [EFFECTS-GENERIC-DECL].
#[derive(Clone)]
pub(crate) struct EffectInfo {
    /// The effect's declared type parameter names, in order.
    pub type_params: Vec<String>,
    /// Operation name → written signature (`fn(T) -> Unit`) and declared mode,
    /// in declaration order.
    pub ops: Vec<(String, String, osprey_ast::OperationMode)>,
}

/// One value thrown away, banked until the substitution is final.
/// Implements [BLOCK-DISCARD].
struct Discard {
    ty: Type,
    position: Option<Position>,
    /// Written as `let _ = e` rather than left bare in statement position.
    explicit: bool,
}

impl Discard {
    fn implicit(ty: Type, position: Option<Position>) -> Self {
        Self {
            ty,
            position,
            explicit: false,
        }
    }

    fn explicit(ty: Type, position: Option<Position>) -> Self {
        Self {
            ty,
            position,
            explicit: true,
        }
    }
}

/// Generated wildcard binders still name the source `_` in diagnostics.
pub(crate) fn annotation_name(name: &str) -> String {
    if name.starts_with("$wild") {
        "_".to_owned()
    } else {
        name.to_owned()
    }
}

/// All cross-cutting declaration tables, plus the inference context.
pub(crate) struct Checker {
    pub(crate) ctx: InferCtx,
    pub(crate) errors: Vec<TypeError>,
    function_effects: HashMap<String, crate::info::EffectRequirements>,
    pub(crate) ctors: HashMap<String, CtorInfo>,
    /// Effect name -> its generic declaration (type params + raw op sigs).
    effects: HashMap<String, EffectInfo>,
    pub(crate) expression_types: HashMap<usize, Type>,
    /// Union/Result type name -> its variant constructor names (exhaustiveness).
    pub(crate) union_variants: HashMap<String, Vec<String>>,
    /// Function/extern name -> declared parameter names (for named arguments).
    pub(crate) fn_params: HashMap<String, Vec<String>>,
    /// Function name -> the exact (params, ret) types created in pass one, so
    /// body inference reuses the very same variables the signature exported.
    fn_sigs: HashMap<String, (Vec<Type>, Type)>,
    /// Expansive calls returning a handler closure allocate one shared state.
    /// Their result cannot quantify a fresh callable ABI at each use.
    handler_factories: HashSet<String>,
    /// Every lambda's inferred function type, keyed by its source position —
    /// resolved and published to the backend by [`infer_program`].
    pub(crate) lambda_tys: Vec<(Position, Type)>,
    /// Every `let` binding's inferred type, keyed by its source position, so
    /// editor hover can show the type of an unannotated binding. Resolved and
    /// published by [`infer_program`]. Implements [LSP-HOVER-VARIABLES]
    let_tys: Vec<(Position, Type)>,
    /// Every list literal's inferred `List<T>`, keyed by the literal's source
    /// position. An empty literal carries no element the backend can read a
    /// representation from, so this table is its only source of one. Resolved
    /// and published by [`infer_program`].
    pub(crate) list_tys: Vec<(Position, Type)>,
    /// Fresh substitutions at value uses, retained for the effect proof.
    pub(crate) instantiations: HashMap<usize, HashMap<VarId, Type>>,
    /// Dotted call identity: a field name, or receiver-first function dispatch.
    pub(crate) methods: crate::methods::Targets,
    /// Explicit applications have stable source positions for backend cloning.
    pub(crate) application_tys: Vec<(Position, HashMap<VarId, Type>)>,
    /// Concrete arguments passed to representation-sensitive built-ins. These
    /// are validated after inference so a variable constrained later in the
    /// same body is checked at its final type.
    pub(crate) builtin_uses: Vec<(String, Type)>,
    /// Declared types whose fields only their own module may read; keyed by
    /// linkage name, so [`osprey_ast::symbol::encloses`] decides membership.
    /// Implements [MODULES-OPAQUE-TYPES].
    pub(crate) opaque_types: HashSet<String>,
    /// Linkage name of the top-level function being checked — the lexical home
    /// a field obligation records, so a generic accessor exported by a module
    /// keeps its access rights when the obligation travels to a client's call.
    /// Empty outside any function (the entry prologue).
    pub(crate) site: String,
    /// The function each located arithmetic obligation was written in. An
    /// obligation is settled long after its function, and whether its operand
    /// may be read through an opaque alias depends on where the operator is.
    pub(crate) use_sites: HashMap<String, String>,
    /// Generalized constraints remain part of each source binding's contract.
    scheme_obligations: HashMap<String, Vec<(String, Type)>>,
    /// Every discarded value, where it was written, and whether the author said
    /// so with `let _ =`. Validated after inference for the same reason
    /// `builtin_uses` is. Implements [BLOCK-DISCARD].
    discards: Vec<Discard>,
    /// The built-in function names — user code may not redefine these.
    builtins: HashSet<String>,
    /// Source mutation boundaries were already checked before static discharge.
    /// Rewritten static arms may now be closure bodies; their assignments still
    /// require mutable bindings and matching value types.
    source_contracts_validated: bool,
    /// The enclosing handler arms, innermost last. A control arm binds the
    /// continuation `resume` invokes; a value arm binds none and hides any
    /// outer one. Presence also marks the arm's mutation scope.
    /// Implements [EFFECTS-RESUME].
    pub(crate) resume_ctx: Vec<ResumeSite>,
    /// Stack of in-scope effect instantiations — one entry per enclosing
    /// `handle` body or declared effect-row entry — resolved innermost-first
    /// by `perform` sites, matching the runtime's innermost-wins handler
    /// stack. Implements [EFFECTS-GENERIC-INSTANTIATION].
    pub(crate) handler_scopes: Vec<EffectScope>,
    /// Every `perform` site's instantiated operation signature and effect
    /// type arguments, keyed by its source position — resolved and published
    /// for the code generator.
    pub(crate) perform_tys: Vec<(Position, crate::info::OpType, Vec<Type>)>,
    /// A perform site's effect arguments inferred independently from its
    /// declared function row. This keeps a wrong row contract from laundering
    /// the operation's actual instantiation in the static effect proof.
    pub(crate) perform_actual_tys: Vec<(Position, Vec<Type>)>,
    /// Every `handle` site's instantiated effect type arguments and operation
    /// signatures, keyed by its source position — resolved and published for
    /// the code generator.
    pub(crate) handler_tys: Vec<(Position, Vec<Type>, HashMap<String, crate::info::OpType>)>,
    /// Function name → its declared type parameters bound to fresh inference
    /// variables (empty for undeclared). Implements [TYPE-GENERICS-FN].
    fn_typarams: HashMap<String, HashMap<String, Type>>,
    /// Explicit function-row arguments after record/alias resolution. The
    /// effect checker compares these with inferred request identities.
    declared_effect_rows: HashMap<(u32, u32), Vec<Option<Vec<Type>>>>,
    /// The type parameters of the function whose body is currently being
    /// inferred, so annotations inside the body (explicit construction-site
    /// type arguments) resolve the binder's variables, not nominal names.
    pub(crate) current_fn_typarams: HashMap<String, Type>,
    /// Whether an arithmetic site with two unconstrained operands may still
    /// leave its overload open ([`Checker::deferred_arith`]). Cleared for the
    /// resolution pass, which re-enters the same selection to settle every
    /// pending site against its operands' final types.
    pub(crate) defer_arith: bool,
}

/// The binding name that means "throw this away" — the one spelling that opts
/// out of [BLOCK-DISCARD] for an ordinary value.
const DISCARD_BINDING: &str = "_";

/// What `resume` finds in the innermost enclosing handler arm.
#[derive(Clone)]
pub(crate) enum ResumeSite {
    /// A control arm: `resume` delivers the operation's result and the
    /// expression evaluates to the handler's answer.
    Control { op_ret: Type, answer: Type },
    /// A value arm supplies the operation's result by returning it, so it
    /// owns no continuation to invoke.
    Value { effect: String, operation: String },
}

impl ResumeSite {
    /// Why `resume` is refused at this site (`None` when the site is not
    /// an arm at all). Implements [EFFECTS-RESUME].
    pub(crate) fn refusal(site: Option<&Self>) -> String {
        match site {
            Some(Self::Value { effect, operation }) if effect == osprey_ast::ARITH_EFFECT => format!(
                "handler arm `Arith.{operation}` cannot `resume`: Arith operations use value mode; return the recovery value from the arm"
            ),
            Some(Self::Value { effect, operation }) => format!(
                "handler arm `{effect}.{operation}` cannot `resume`: `{operation}` is a value operation, so its arm supplies the operation's result and owns no continuation; `resume` requires the continuation of a control operation arm, so declare `{operation}` `control` to take it"
            ),
            _ => "`resume` requires the continuation of a control operation arm, and none is live here: `resume` is only meaningful directly inside such an arm, not at top level or in a lambda body, which runs when called rather than where it is written"
                .to_owned(),
        }
    }
}
