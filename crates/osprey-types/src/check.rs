//! The type checker driver: a two-pass walk over a [`Program`]. Pass one
//! collects every top-level declaration (types + their constructors, effects,
//! externs, function signatures) so forward references and recursion resolve.
//! Pass two infers each function body and top-level statement, unifying against
//! the declared signatures, then resolves everything against the final
//! substitution.

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
    /// Operation name → written signature (`fn(T) -> Unit`), in declaration
    /// order.
    pub ops: Vec<(String, String)>,
}

/// All cross-cutting declaration tables, plus the inference context.
pub struct Checker {
    pub(crate) ctx: InferCtx,
    pub(crate) errors: Vec<TypeError>,
    pub(crate) ctors: HashMap<String, CtorInfo>,
    /// Effect name -> its generic declaration (type params + raw op sigs).
    effects: HashMap<String, EffectInfo>,
    /// Union/Result type name -> its variant constructor names (exhaustiveness).
    pub(crate) union_variants: HashMap<String, Vec<String>>,
    /// Function/extern name -> declared parameter names (for named arguments).
    pub(crate) fn_params: HashMap<String, Vec<String>>,
    /// Function name -> the exact (params, ret) types created in pass one, so
    /// body inference reuses the very same variables the signature exported.
    fn_sigs: HashMap<String, (Vec<Type>, Type)>,
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
    /// Concrete arguments passed to representation-sensitive built-ins. These
    /// are validated after inference so a variable constrained later in the
    /// same body is checked at its final type.
    pub(crate) builtin_uses: Vec<(String, Type)>,
    /// The built-in function names — user code may not redefine these.
    builtins: HashSet<String>,
    /// Stack of `(operation result type, handler answer type)` for the handler
    /// arms currently being inferred, so a `resume` inside an arm types its
    /// argument against the operation result and itself as the answer.
    /// Implements [EFFECTS-RESUME].
    pub(crate) resume_ctx: Vec<(Type, Type)>,
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

impl Checker {
    fn new() -> Checker {
        let mut c = Checker {
            ctx: InferCtx::new(),
            errors: Vec::new(),
            ctors: HashMap::new(),
            effects: HashMap::new(),
            union_variants: HashMap::new(),
            fn_params: HashMap::new(),
            fn_sigs: HashMap::new(),
            lambda_tys: Vec::new(),
            let_tys: Vec::new(),
            list_tys: Vec::new(),
            builtin_uses: Vec::new(),
            builtins: HashSet::new(),
            resume_ctx: Vec::new(),
            handler_scopes: Vec::new(),
            perform_tys: Vec::new(),
            perform_actual_tys: Vec::new(),
            handler_tys: Vec::new(),
            fn_typarams: HashMap::new(),
            current_fn_typarams: HashMap::new(),
            defer_arith: true,
        };
        c.register_result_ctors();
        c.register_builtin_variances();
        // Burn the ids the builtin schemes hand-write as quantified binders so
        // no live inference variable can collide with them (they stay
        // permanently unbound). See `builtins::RESERVED_SCHEME_VARS`.
        for _ in 0..crate::builtins::RESERVED_SCHEME_VARS {
            let _ = c.ctx.fresh();
        }
        c
    }

    /// Built-in constructors' declared variance: producers are covariant in
    /// what they produce; `Map` keys are looked up (invariant) while values
    /// only flow out. Implements [TYPE-VARIANCE-ASSIGN].
    fn register_builtin_variances(&mut self) {
        self.ctx.set_variance(
            names::RESULT,
            vec![Variance::Covariant, Variance::Covariant],
        );
        self.ctx
            .set_variance(names::LIST, vec![Variance::Covariant]);
        self.ctx
            .set_variance(names::FIBER, vec![Variance::Covariant]);
        self.ctx
            .set_variance(names::MAP, vec![Variance::Invariant, Variance::Covariant]);
    }

    /// Built-in `Result` constructors `Success { value: T }` / `Error { message: E }`.
    fn register_result_ctors(&mut self) {
        let _ = self.ctors.insert(
            names::SUCCESS.into(),
            CtorInfo {
                owner: names::RESULT.into(),
                owner_is_record: false,
                type_params: vec!["T".into(), "E".into()],
                fields: vec![("value".into(), "T".into())],
            },
        );
        // `Error { message: <string> }` builds the E side of a `Result<T, E>`;
        // the message is a concrete string, leaving E free to unify with the
        // declared error type (e.g. the nominal `Error`), not pinned to string.
        let _ = self.ctors.insert(
            names::ERROR.into(),
            CtorInfo {
                owner: names::RESULT.into(),
                owner_is_record: false,
                type_params: vec!["T".into(), "E".into()],
                fields: vec![("message".into(), "string".into())],
            },
        );
        let _ = self.union_variants.insert(
            names::RESULT.into(),
            vec![names::SUCCESS.into(), names::ERROR.into()],
        );
        // Built-in HttpResponse record returned by HTTP request handlers.
        let _ = self.ctors.insert(
            "HttpResponse".into(),
            CtorInfo {
                owner: "HttpResponse".into(),
                owner_is_record: true,
                type_params: Vec::new(),
                // Field set + order match `struct HttpResponse` in
                // `runtime/http_shared.h` exactly — the C HTTP runtime reads the
                // handler's returned struct by this layout.
                fields: vec![
                    ("status".into(), "int".into()),
                    ("headers".into(), "string".into()),
                    ("contentType".into(), "string".into()),
                    ("streamFd".into(), "int".into()),
                    ("isComplete".into(), "bool".into()),
                    ("partialBody".into(), "string".into()),
                ],
            },
        );
    }

    fn record_err(&mut self, e: TypeError, pos: Option<Position>) {
        self.errors.push(e.with_pos(pos));
    }

    /// Unify and record any failure. Shared by the expr/pattern modules.
    pub(crate) fn push_unify(&mut self, a: &Type, b: &Type) {
        if let Err(e) = unify(&mut self.ctx, a, b) {
            self.errors.push(e);
        }
    }

    /// Directional assignment-site unification, recording failures. This may
    /// wrap a bare value in Success but never erase a Result error channel.
    pub(crate) fn push_assign(&mut self, expected: &Type, actual: &Type) {
        if let Err(e) = unify_assignable(&mut self.ctx, expected, actual) {
            self.errors.push(e);
        }
    }

    /// Check a statement appearing inside a block expression, threading new
    /// bindings into the block's local scope.
    pub(crate) fn infer_block_stmt(&mut self, s: &Stmt, env: &mut TypeEnv) {
        match s {
            Stmt::Let {
                name,
                mutable,
                ty,
                value,
                position,
                ..
            } => self.check_let(name, *mutable, ty.as_ref(), value, env, *position),
            Stmt::Assignment {
                name,
                value,
                position,
            } => self.check_assignment(name, value, env, *position),
            Stmt::Expr {
                value, position, ..
            } => {
                let inferred = self.infer_expr(value, env);
                if self.ctx.prune(&inferred).is_named(names::RESULT) {
                    self.record_err(
                        TypeError::new(
                            "an unhandled `Result` cannot be discarded; use `match` or `?:`",
                        ),
                        *position,
                    );
                }
            }
            _ => {}
        }
    }

    /// Pass one: fill the declaration tables and the base environment.
    fn collect(&mut self, program: &Program, env: &mut TypeEnv) {
        // `env` is exactly the builtin table on entry — snapshot it so
        // `collect_function` can reject redefinition of a built-in.
        if self.builtins.is_empty() {
            self.builtins = env.bound_names();
        }
        // Register every declared type's variance first, so position
        // validation sees nested constructors' variance regardless of
        // declaration order. Implements [TYPE-VARIANCE-DECL].
        for stmt in &program.statements {
            if let Stmt::Type {
                name, type_params, ..
            } = stmt
            {
                self.ctx.set_variance(
                    name.clone(),
                    type_params.iter().map(|p| p.variance).collect(),
                );
            }
        }
        for stmt in &program.statements {
            match stmt {
                Stmt::Type {
                    name,
                    type_params,
                    variants,
                    validation_func,
                    position,
                    ..
                } => {
                    if validation_func.is_some() {
                        self.record_err(
                            TypeError::new("validated record `where` is not supported".to_string()),
                            *position,
                        );
                    }
                    self.collect_type(name, type_params, variants, *position);
                }
                Stmt::Effect {
                    name,
                    type_params,
                    operations,
                    position,
                    ..
                } => self.collect_effect(name, type_params, operations, *position),
                Stmt::Extern {
                    name,
                    parameters,
                    return_type,
                    ..
                } => self.collect_extern(name, parameters, return_type.as_ref(), env),
                Stmt::Function {
                    name,
                    type_params,
                    parameters,
                    return_type,
                    position,
                    ..
                } => {
                    self.collect_function(name, type_params, parameters, return_type.as_ref(), env);
                    for e in crate::variance::reject_fn_variance(name, type_params) {
                        self.record_err(e, *position);
                    }
                }
                _ => {}
            }
        }
    }

    /// Registers constructor fields before body inference, including fields
    /// that refer to their own or another declared union [TYPE-UNION-REC].
    fn collect_type(
        &mut self,
        name: &str,
        type_params: &[TypeParam],
        variants: &[TypeVariant],
        position: Option<Position>,
    ) {
        let is_record = match variants.first() {
            Some(first) => variants.len() == 1 && first.name == name,
            None => false,
        };
        if !is_record {
            let _ = self.union_variants.insert(
                name.to_string(),
                variants.iter().map(|v| v.name.clone()).collect(),
            );
        }
        let param_names: Vec<String> = type_params.iter().map(|p| p.name.clone()).collect();
        for v in variants {
            let fields = v
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.ty.clone()))
                .collect();
            let _ = self.ctors.insert(
                v.name.clone(),
                CtorInfo {
                    owner: name.to_string(),
                    owner_is_record: is_record,
                    type_params: param_names.clone(),
                    fields,
                },
            );
        }
        for e in crate::variance::validate_type_decl(&self.ctx, name, type_params, variants) {
            self.record_err(e, position);
        }
    }

    fn collect_effect(
        &mut self,
        name: &str,
        type_params: &[TypeParam],
        operations: &[EffectOperation],
        position: Option<Position>,
    ) {
        let _ = self.effects.insert(
            name.to_string(),
            EffectInfo {
                type_params: type_params.iter().map(|p| p.name.clone()).collect(),
                ops: operations
                    .iter()
                    .map(|op| (op.name.clone(), op.ty.clone()))
                    .collect(),
            },
        );
        for e in crate::variance::validate_effect_decl(&self.ctx, name, type_params, operations) {
            self.record_err(e, position);
        }
    }

    /// Instantiate an effect's operations at fresh type arguments — one
    /// instance per handle site / unresolved perform. Returns `None` for an
    /// undeclared effect. Implements [EFFECTS-GENERIC-INSTANTIATION].
    pub(crate) fn effect_instance_ops(
        &mut self,
        effect: &str,
    ) -> Option<(Vec<Type>, HashMap<String, crate::info::OpType>)> {
        let info = self.effects.get(effect)?.clone();
        let mut pmap = HashMap::new();
        let mut args = Vec::new();
        for p in &info.type_params {
            let v = self.ctx.fresh();
            args.push(v.clone());
            let _ = pmap.insert(p.clone(), v);
        }
        Some((args, Self::instantiate_ops(&info, &pmap)))
    }

    /// Instantiate an effect at an effect-row entry's declared type arguments
    /// (`!State<int>`), resolving each argument against the enclosing
    /// function's type parameters; missing arguments become fresh variables.
    /// Implements [EFFECTS-GENERIC-ROWS].
    fn effect_row_scope(
        &mut self,
        row: &EffectRef,
        fn_typarams: &HashMap<String, Type>,
    ) -> Option<EffectScope> {
        let info = self.effects.get(&row.name)?.clone();
        let mut pmap = HashMap::new();
        let mut args = Vec::new();
        for (i, p) in info.type_params.iter().enumerate() {
            let t = match row.type_args.get(i) {
                Some(te) => type_expr_to_type(te, fn_typarams),
                None => self.ctx.fresh(),
            };
            args.push(t.clone());
            let _ = pmap.insert(p.clone(), t);
        }
        Some(EffectScope {
            name: row.name.clone(),
            args,
            ops: Self::instantiate_ops(&info, &pmap),
        })
    }

    /// Parse an effect's raw operation signatures against an instantiation of
    /// its type parameters.
    fn instantiate_ops(
        info: &EffectInfo,
        pmap: &HashMap<String, Type>,
    ) -> HashMap<String, crate::info::OpType> {
        info.ops
            .iter()
            .map(|(op_name, sig)| {
                let (params, ret) = parse_fn_sig(sig, pmap);
                (op_name.clone(), crate::info::OpType { params, ret })
            })
            .collect()
    }

    fn collect_extern(
        &mut self,
        name: &str,
        parameters: &[ExternParameter],
        return_type: Option<&TypeExpr>,
        env: &mut TypeEnv,
    ) {
        let empty = HashMap::new();
        let params: Vec<Type> = parameters
            .iter()
            .map(|p| type_expr_to_type(&p.ty, &empty))
            .collect();
        let ret = return_type.map_or_else(Type::unit, |r| type_expr_to_type(r, &empty));
        // Publishing the resolved signature is what lets the backend type FFI
        // calls with the declared parameter/return types (a `Ptr` as `i8*`, not
        // the `i64` default) — same as `collect_function`.
        self.publish_signature(name, parameters, params, ret, env);
    }

    /// Record a declaration's parameter names, publish its resolved signature
    /// for the backend, and bind it monomorphically in `env`. Both collectors
    /// (extern and function) finish here, so a signature can never reach one
    /// table and miss another.
    fn publish_signature<P: ParamName>(
        &mut self,
        name: &str,
        parameters: &[P],
        params: Vec<Type>,
        ret: Type,
        env: &mut TypeEnv,
    ) {
        self.record_fn_params(name, parameters);
        let _ = self
            .fn_sigs
            .insert(name.to_string(), (params.clone(), ret.clone()));
        env.insert(name, Scheme::mono(Type::fun(params, ret)));
    }

    /// Record a function/extern's positional parameter names (for named-argument
    /// reordering at call sites). Generic over the two parameter node types,
    /// which both carry a `name`.
    fn record_fn_params<P: ParamName>(&mut self, name: &str, parameters: &[P]) {
        let _ = self.fn_params.insert(
            name.to_string(),
            parameters
                .iter()
                .map(|p| p.param_name().to_string())
                .collect(),
        );
    }

    fn collect_function(
        &mut self,
        name: &str,
        type_params: &[TypeParam],
        parameters: &[Parameter],
        return_type: Option<&TypeExpr>,
        env: &mut TypeEnv,
    ) {
        // The testing built-ins are shadowable by design [TESTING-SHADOWING]:
        // a user `fn test/expect/check` replaces the built-in scheme below.
        if self.builtins.contains(name) && !crate::builtins::SHADOWABLE_BUILTINS.contains(&name) {
            self.errors.push(TypeError::new(format!(
                "cannot redefine built-in function `{name}`"
            )));
            return;
        }
        if self.fn_sigs.contains_key(name) {
            self.errors
                .push(TypeError::new(format!("duplicate definition `{name}`")));
            return;
        }
        // Declared type parameters (`fn map<T, U>`) bind to fresh inference
        // variables so every `T` in the signature is the SAME variable —
        // without a binder, `T` would be a nominal type named "T".
        // Implements [TYPE-GENERICS-FN].
        let mut typarams = HashMap::new();
        for tp in type_params {
            let v = self.ctx.fresh();
            let _ = typarams.insert(tp.name.clone(), v);
        }
        let params: Vec<Type> = parameters
            .iter()
            .map(|p| match &p.ty {
                Some(te) => type_expr_to_type(te, &typarams),
                None => self.ctx.fresh(),
            })
            .collect();
        let ret = match return_type {
            Some(te) => type_expr_to_type(te, &typarams),
            None => self.ctx.fresh(),
        };
        let _ = self.fn_typarams.insert(name.to_string(), typarams);
        self.publish_signature(name, parameters, params, ret, env);
    }

    /// Pass two: infer bodies and run top-level statements.
    fn check(&mut self, program: &Program, env: &mut TypeEnv) {
        for stmt in &program.statements {
            match stmt {
                Stmt::Function {
                    name,
                    parameters,
                    effects,
                    body,
                    position,
                    ..
                } => self.check_function(name, parameters, effects, body, env, *position),
                Stmt::Module { body, .. } => {
                    let mut inner = env.child();
                    let prog = Program {
                        statements: body
                            .iter()
                            .map(|item| item.declaration.as_ref().clone())
                            .collect(),
                    };
                    // Module declarations live in their own lexical scope. Run
                    // both checker passes there: the old implementation only
                    // ran pass two, so module functions were never registered
                    // and their bodies were silently skipped.
                    self.collect(&prog, &mut inner);
                    self.check(&prog, &mut inner);
                }
                Stmt::Namespace { body, .. } => {
                    let mut inner = env.child();
                    let prog = Program {
                        statements: body.clone(),
                    };
                    self.collect(&prog, &mut inner);
                    self.check(&prog, &mut inner);
                }
                // `let` / assignment / bare-expr statements infer the same way at
                // top level and inside a block.
                other => self.infer_block_stmt(other, env),
            }
        }
    }

    fn check_function(
        &mut self,
        name: &str,
        parameters: &[Parameter],
        effects: &[EffectRef],
        body: &Expr,
        env: &mut TypeEnv,
        pos: Option<Position>,
    ) {
        let (params, ret) = match self.fn_sigs.get(name) {
            Some(sig) => sig.clone(),
            None => return,
        };
        let mut local = env.child();
        for (p, ty) in parameters.iter().zip(&params) {
            local.insert(p.name.clone(), Scheme::mono(ty.clone()));
        }
        // The declared effect row instantiates each referenced effect for the
        // body's `perform` sites (`!State<int>` pins `T` to `int`).
        // Implements [EFFECTS-GENERIC-ROWS].
        let typarams = self.fn_typarams.get(name).cloned().unwrap_or_default();
        let scopes: Vec<_> = effects
            .iter()
            .filter_map(|r| self.effect_row_scope(r, &typarams))
            .collect();
        let pushed = scopes.len();
        self.handler_scopes.extend(scopes);
        self.current_fn_typarams = typarams;
        let body_ty = self.infer_expr(body, &local);
        self.current_fn_typarams = HashMap::new();
        self.handler_scopes
            .truncate(self.handler_scopes.len().saturating_sub(pushed));
        self.unify_or_err(&ret, &body_ty, &format!("function `{name}` body"), pos);
        // Generalize the now-constrained signature so later call sites can use
        // the function polymorphically (HM let-generalization for top-level fns).
        // Remove the function's own monomorphic entry first, else its signature
        // variables would count as "free in the environment" and nothing would
        // generalize.
        let fun_ty = Type::fun(params, ret);
        env.remove(name);
        let scheme = self.generalize_with_obligations(env, &fun_ty);
        env.insert(name, scheme);
    }

    /// Generalize `ty`, carrying every built-in obligation recorded on a
    /// variable this scheme quantifies ([`Scheme::obligations`]).
    ///
    /// An obligation on a variable that generalizes cannot be discharged here —
    /// the body never says what the type is — but it is not vacuous either: it
    /// binds at every call site. Keeping it in the flat list as well would
    /// report the wrapper itself, which is not an error, so the obligation
    /// MOVES into the scheme.
    fn generalize_with_obligations(&mut self, env: &TypeEnv, ty: &Type) -> Scheme {
        let mut scheme = generalize(&mut self.ctx, env, ty);
        if scheme.vars.is_empty() {
            return scheme;
        }
        let (deferred, kept) = self.split_obligations(&scheme.vars);
        // Quantifying a variable that still carries a pending arithmetic
        // overload would hand every call site its own fresh copy, so no use
        // could ever inform the choice and the definition would default to
        // integer with nothing having looked at it. Keeping it monomorphic is
        // what lets `gpuFold(0.0, plus)` type `plus` at float.
        let pinned = self.arith_operand_vars(&kept);
        scheme.vars.retain(|v| !pinned.contains(v));
        self.builtin_uses = kept;
        scheme.obligations = deferred;
        scheme
    }

    /// Every type variable a pending arithmetic overload rests on.
    fn arith_operand_vars(&mut self, obligations: &[(String, Type)]) -> BTreeSet<VarId> {
        let mut vars = BTreeSet::new();
        for (name, ty) in obligations {
            if crate::expr::is_deferred_arith(name) {
                self.ctx.free_vars(ty, &mut vars);
            }
        }
        vars
    }

    /// Split the recorded built-in uses into those resting on `vars` (deferred
    /// to each instantiation) and those that stay for [`Self::validate_builtin_uses`].
    fn split_obligations(&mut self, vars: &[VarId]) -> SplitObligations {
        let uses = std::mem::take(&mut self.builtin_uses);
        let resolved: Vec<(String, Type)> = uses
            .into_iter()
            .map(|(name, ty)| {
                let ty = self.ctx.apply(&ty);
                (name, ty)
            })
            .collect();
        let mut deferred = Vec::new();
        let mut kept = Vec::new();
        for (name, ty) in resolved {
            // A pending arithmetic overload never travels into a scheme: it is
            // settled once for the whole program
            // ([`Checker::deferred_arith`]), and the variables it rests on stay
            // monomorphic so every use of the definition constrains the SAME
            // choice.
            if crate::expr::is_deferred_arith(&name) || !self.mentions_any(&ty, vars) {
                kept.push((name, ty));
            } else {
                deferred.push((name, ty));
            }
        }
        (deferred, kept)
    }

    /// Whether `ty` still rests on one of the quantified variables.
    fn mentions_any(&mut self, ty: &Type, vars: &[VarId]) -> bool {
        let mut free = BTreeSet::new();
        self.ctx.free_vars(ty, &mut free);
        vars.iter().any(|v| free.contains(v))
    }

    fn check_let(
        &mut self,
        name: &str,
        mutable: bool,
        ty: Option<&TypeExpr>,
        value: &Expr,
        env: &mut TypeEnv,
        pos: Option<Position>,
    ) {
        let value_ty = self.infer_expr(value, env);
        let binding_ty = if let Some(te) = ty {
            let annotated = type_expr_to_type(te, &HashMap::new());
            self.unify_or_err(&annotated, &value_ty, &format!("let `{name}`"), pos);
            annotated
        } else {
            value_ty.clone()
        };
        // Publish the binding's inferred type for editor hover, keyed by source
        // position (resolved against the final substitution in `infer_program`).
        // Implements [LSP-HOVER-VARIABLES]
        if let Some(p) = pos {
            self.let_tys.push((p, binding_ty.clone()));
        }
        // A let-bound value carries obligations exactly as a named function
        // does: quantifying a variable that still holds a pending arithmetic
        // overload would hand every use its own fresh copy, so nothing could
        // inform the choice and `let join = |a, b| => a + b` used only on
        // strings defaulted to the CHECKED INTEGER overload — `join("a", "b")`
        // printed `Success(<pointer>)` instead of `ab`
        // ([`Checker::deferred_arith`]).
        let mut scheme = self.generalize_with_obligations(env, &binding_ty);
        let pinned = self.stateful_handle_vars(&binding_ty);
        scheme.vars.retain(|v| !pinned.contains(v));
        if mutable {
            env.insert_mutable(name, scheme);
        } else {
            env.insert(name, scheme);
        }
    }

    /// Every type variable sitting inside a STATEFUL HANDLE VALUE in `ty` — the
    /// ML value restriction, narrowed to the two constructors that need it.
    ///
    /// A `let` generalizes, so quantifying such a variable hands every USE of
    /// the one handle its own copy, and nothing ties them together:
    ///
    /// ```text
    /// let ch = Channel(1)
    /// send(ch, "text")
    /// print("${recv(ch) + 1}")   // accepted; printed a raw pointer as an int
    /// ```
    ///
    /// A channel's element type is fixed at run time by what was sent, and a
    /// fiber's by the thunk that produced it; neither is a fresh value per use
    /// the way an immutable `List<t>` is. The same hole is why a
    /// `Channel<List<List<int>>>` was received back as `List<int>` and every
    /// nested read answered its fallback.
    /// Implements [CONCURRENCY-CHANNEL] and [CONCURRENCY-SPAWN-AWAIT].
    fn stateful_handle_vars(&mut self, ty: &Type) -> BTreeSet<VarId> {
        let mut pinned = BTreeSet::new();
        let applied = self.ctx.apply(ty);
        self.collect_handle_vars(&applied, &mut pinned);
        pinned
    }

    fn collect_handle_vars(&mut self, ty: &Type, out: &mut BTreeSet<VarId>) {
        match ty {
            Type::Con { name, args } if name == names::CHANNEL || name == names::FIBER => {
                for arg in args {
                    self.ctx.free_vars(arg, out);
                }
            }
            Type::Con { args, .. } => {
                for arg in args {
                    self.collect_handle_vars(arg, out);
                }
            }
            // NOT into a function. A function's type variables are instantiated
            // fresh at every call, so a factory or helper that merely mentions a
            // handle — `let make = |n| => Channel(n)`, or a generic drain
            // helper — hands each call its own and stays soundly polymorphic.
            // Pinning those would reject safe code to fix an unsafe case that
            // only arises when one handle VALUE is shared between uses.
            Type::Record { fields, .. } => {
                for field in fields.values() {
                    self.collect_handle_vars(field, out);
                }
            }
            _ => {}
        }
    }

    /// Unify `expected` against `actual`, recording a positioned error prefixed
    /// with `label` (e.g. `` "let `x`" ``) when they don't match.
    fn unify_or_err(&mut self, expected: &Type, actual: &Type, label: &str, pos: Option<Position>) {
        if let Err(e) = unify_assignable(&mut self.ctx, expected, actual) {
            self.record_err(TypeError::new(format!("{label}: {}", e.message)), pos);
        }
    }

    fn check_assignment(
        &mut self,
        name: &str,
        value: &Expr,
        env: &mut TypeEnv,
        pos: Option<Position>,
    ) {
        let value_ty = self.infer_expr(value, env);
        match env.get(name).cloned() {
            Some(scheme) => {
                if !env.is_mutable(name) {
                    self.record_err(
                        TypeError::new(format!("cannot assign to immutable variable `{name}`")),
                        pos,
                    );
                } else if self.resume_ctx.is_empty() {
                    // Handler arms are the language's mutation boundary. The
                    // resume context is present only while an arm body is
                    // being checked and is deliberately cleared across lambda
                    // boundaries, matching where handler-owned state may be
                    // changed at runtime.
                    self.record_err(
                        TypeError::new(
                            "state mutation is only allowed inside an effect handler arm",
                        ),
                        pos,
                    );
                }
                let existing = crate::env::instantiate(&mut self.ctx, &scheme);
                self.unify_or_err(
                    &existing,
                    &value_ty,
                    &format!("assignment to `{name}`"),
                    pos,
                );
            }
            None => self.record_err(
                TypeError::new(format!("assignment to undeclared `{name}`")),
                pos,
            ),
        }
    }

    /// Reject concrete receiver/value types that their built-in runtime cannot
    /// interpret. Unresolved generalized variables remain the one inference
    /// limitation: Osprey has no type-class constraint to preserve the
    /// obligation in a polymorphic scheme.
    fn validate_builtin_uses(&mut self) {
        let uses = std::mem::take(&mut self.builtin_uses);
        for (name, ty) in uses {
            let resolved = self.ctx.apply(&ty);
            if let Some(message) = crate::builtin_constraints::invalid_use(&name, &resolved) {
                self.errors.push(TypeError::new(message));
            }
        }
    }

    /// Settle every arithmetic overload left open by
    /// [`Checker::deferred_arith`], now that nothing further can constrain an
    /// operand. Resolving one site can constrain another's operand — `fn twice(x)
    /// = x + x` used by `fn quad(y) = twice(y) + twice(y)` — so this repeats
    /// until a pass changes nothing, which it must: each pass either resolves a
    /// site or leaves the substitution untouched.
    fn resolve_deferred_arithmetic(&mut self) {
        self.defer_arith = false;
        for _ in 0..self.builtin_uses.len().saturating_add(1) {
            let before = self.ctx.bound_count();
            let uses = self.builtin_uses.clone();
            for (name, ty) in &uses {
                self.resolve_deferred_arith(name, ty);
            }
            if self.ctx.bound_count() == before {
                return;
            }
        }
    }

    /// Build the instantiated field types of a constructor against fresh type
    /// arguments. Returns (per-type-param fresh var, declared field map).
    pub(crate) fn ctor_instance(&mut self, name: &str) -> Option<CtorInstance> {
        let info = self.ctors.get(name)?;
        let owner = info.owner.clone();
        let is_record = info.owner_is_record;
        let params = info.type_params.clone();
        let raw_fields = info.fields.clone();
        let mut pmap = HashMap::new();
        let mut args = Vec::new();
        for p in &params {
            let v = self.ctx.fresh();
            let _ = pmap.insert(p.clone(), v.clone());
            args.push(v);
        }
        let fields = raw_fields
            .iter()
            .map(|(fname, fty)| (fname.clone(), type_name_to_type(fty, &pmap)))
            .collect();
        Some((args, fields, owner, is_record))
    }
}

/// Type-check a program. Returns every type error found (empty ⇒ well-typed).
#[must_use]
pub fn check_program(program: &Program) -> Vec<TypeError> {
    checked_program(program).errors
}

/// Run inference and publish the resolved signatures, constructor layouts and
/// union tags for the code generator. Type errors are intentionally dropped
/// here — codegen runs after `check_program` has gated correctness — so the
/// backend always receives the best-effort resolved shape of every declaration.
#[must_use]
pub fn infer_program(program: &Program) -> crate::info::ProgramTypes {
    use crate::info::{CtorLayout, ProgramTypes};
    let mut checker = checked_program(program);

    let functions = checker
        .fn_sigs
        .iter()
        .map(|(name, (params, ret))| {
            let rp = params.iter().map(|t| checker.ctx.apply(t)).collect();
            let rr = checker.ctx.apply(ret);
            (name.clone(), (rp, rr))
        })
        .collect();
    let ctors = checker
        .ctors
        .iter()
        .map(|(name, info)| {
            // A declared type parameter resolves to a type variable — the
            // backend lowers a variable to its uniform boxed representation,
            // which is exactly the generic-payload rule.
            let pmap = erased_type_params(&info.type_params);
            let fields = info
                .fields
                .iter()
                .map(|(f, written)| (f.clone(), type_name_to_type(written, &pmap)))
                .collect();
            (
                name.clone(),
                CtorLayout {
                    owner: info.owner.clone(),
                    owner_is_record: info.owner_is_record,
                    type_params: info.type_params.clone(),
                    fields,
                },
            )
        })
        .collect();
    let unions = checker.union_variants.clone();
    // The erased view of every effect: each declared type parameter resolves
    // to a type variable, which the backend lowers to its uniform boxed
    // representation — one operation ABI per program regardless of how many
    // instantiations exist. Implements [EFFECTS-GENERIC-RUNTIME].
    let effects = checker
        .effects
        .iter()
        .map(|(name, info)| {
            let pmap = erased_type_params(&info.type_params);
            let ops = info
                .ops
                .iter()
                .map(|(op_name, sig)| {
                    let (params, ret) = parse_fn_sig(sig, &pmap);
                    (op_name.clone(), crate::info::OpType { params, ret })
                })
                .collect();
            (name.clone(), ops)
        })
        .collect();
    let lambda_tys = checker.lambda_tys.clone();
    let let_tys = checker.let_tys.clone();
    let list_tys = checker.list_tys.clone();
    let lambdas = resolve_positioned(&mut checker.ctx, &lambda_tys);
    let lets = resolve_positioned(&mut checker.ctx, &let_tys);
    let lists = resolve_positioned(&mut checker.ctx, &list_tys);
    let perform_tys = checker.perform_tys.clone();
    let performs = dedupe_sites(perform_tys.iter().map(|(pos, op, args)| {
        let site = crate::info::PerformSite {
            op: resolve_op(&mut checker.ctx, op),
            effect_args: args.iter().map(|t| checker.ctx.apply(t)).collect(),
        };
        ((pos.line, pos.column), site)
    }));
    let handler_tys = checker.handler_tys.clone();
    let handler_ops = dedupe_sites(handler_tys.iter().map(|(pos, args, ops)| {
        let site = crate::info::HandlerSite {
            effect_args: args.iter().map(|t| checker.ctx.apply(t)).collect(),
            ops: ops
                .iter()
                .map(|(n, op)| (n.clone(), resolve_op(&mut checker.ctx, op)))
                .collect(),
        };
        ((pos.line, pos.column), site)
    }));
    ProgramTypes {
        functions,
        ctors,
        unions,
        effects,
        lambdas,
        lets,
        lists,
        performs,
        handler_ops,
    }
}

/// Collect declarations and type-check a program before either caller consumes
/// diagnostics or publishes inferred backend metadata.
fn checked_program(program: &Program) -> Checker {
    let mut checker = Checker::new();
    let mut env = base_env();
    checker.collect(program, &mut env);
    checker.check(program, &mut env);
    checker.resolve_deferred_arithmetic();
    checker.validate_builtin_uses();
    let perform_tys = checker.perform_tys.clone();
    let perform_actual_tys = checker.perform_actual_tys.clone();
    let handler_tys = checker.handler_tys.clone();
    let scoped_performs =
        collect_site_candidates(perform_tys.into_iter().map(|(position, _, arguments)| {
            (
                (position.line, position.column),
                arguments
                    .iter()
                    .map(|argument| checker.ctx.apply(argument).to_string())
                    .collect(),
            )
        }));
    let mut actual_performs = HashMap::new();
    let mut unresolved_actual_sites = HashSet::new();
    for (position, arguments) in perform_actual_tys {
        let key = (position.line, position.column);
        let arguments: Vec<_> = arguments
            .iter()
            .map(|argument| checker.ctx.apply(argument))
            .collect();
        if arguments.iter().all(type_is_resolved) {
            push_site_candidate(
                &mut actual_performs,
                key,
                arguments.iter().map(ToString::to_string).collect(),
            );
        } else {
            let _ = unresolved_actual_sites.insert(key);
        }
    }
    let mut performs = scoped_performs;
    for (key, candidates) in actual_performs {
        if unresolved_actual_sites.contains(&key) {
            for candidate in candidates {
                push_site_candidate(&mut performs, key, candidate);
            }
        } else {
            let _ = performs.insert(key, candidates);
        }
    }
    let instances = crate::effect_rows::Instances {
        performs,
        handlers: dedupe_sites(handler_tys.into_iter().map(|(position, arguments, _)| {
            (
                (position.line, position.column),
                arguments
                    .iter()
                    .map(|argument| checker.ctx.apply(argument).to_string())
                    .collect(),
            )
        })),
    };
    checker
        .errors
        .extend(crate::effect_rows::check(program, &instances));
    checker.errors.extend(crate::init_order::check(program));
    checker
}

/// The code generator's erased view assigns every declared generic parameter
/// the uniform boxed type-variable representation.
fn erased_type_params(type_params: &[String]) -> HashMap<String, Type> {
    type_params
        .iter()
        .enumerate()
        .map(|(index, parameter)| (parameter.clone(), Type::Var(erased_var(index))))
        .collect()
}

/// The variable id standing for the type parameter at `index`. Numbering the
/// erasure by POSITION is what lets the backend read a generic record's real
/// field types back out of an instantiation: `Envelope<Map<…>, int>.metadata`
/// is declared `U`, and only the parameter's position says that `U` is the
/// second argument. Implements [TYPE-GENERICS-DECL].
#[must_use]
pub fn erased_var(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

/// Resolve one operation signature against the final substitution.
fn resolve_op(ctx: &mut InferCtx, op: &crate::info::OpType) -> crate::info::OpType {
    crate::info::OpType {
        params: op.params.iter().map(|t| ctx.apply(t)).collect(),
        ret: ctx.apply(&op.ret),
    }
}

/// A type is resolved once it mentions no inference variable anywhere — the
/// exact complement of [`crate::ty::has_type_var`], which owns the traversal.
fn type_is_resolved(ty: &Type) -> bool {
    !crate::ty::has_type_var(ty)
}

fn collect_site_candidates<S: PartialEq>(
    entries: impl Iterator<Item = ((u32, u32), S)>,
) -> HashMap<(u32, u32), Vec<S>> {
    let mut out = HashMap::new();
    for (key, candidate) in entries {
        push_site_candidate(&mut out, key, candidate);
    }
    out
}

fn push_site_candidate<S: PartialEq>(
    sites: &mut HashMap<(u32, u32), Vec<S>>,
    key: (u32, u32),
    candidate: S,
) {
    let candidates = sites.entry(key).or_default();
    if !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

/// Collect position-keyed effect sites, DROPPING any key that appears with
/// two different resolutions. String-interpolation fragments are re-parsed
/// with fragment-relative positions, so two performs in different fragments
/// can share a `(line, column)` key — publishing either one would hand
/// codegen the wrong signature. Without an entry the backend degrades to the
/// unmangled, fully-boxed path, which fails loudly (unhandled effect) rather
/// than confusing types. Implements [EFFECTS-GENERIC-INSTANTIATION].
fn dedupe_sites<S: PartialEq>(
    entries: impl Iterator<Item = ((u32, u32), S)>,
) -> HashMap<(u32, u32), S> {
    let mut out: HashMap<(u32, u32), S> = HashMap::new();
    let mut conflicted: HashSet<(u32, u32)> = HashSet::new();
    for (key, site) in entries {
        if conflicted.contains(&key) {
            continue;
        }
        match out.get(&key) {
            Some(existing) if *existing != site => {
                let _ = out.remove(&key);
                let _ = conflicted.insert(key);
            }
            _ => {
                let _ = out.insert(key, site);
            }
        }
    }
    out
}

/// Resolve a list of source-position-keyed types against the final
/// substitution, keying the published map by `(line, column)`.
fn resolve_positioned(ctx: &mut InferCtx, tys: &[(Position, Type)]) -> HashMap<(u32, u32), Type> {
    tys.iter()
        .map(|(pos, ty)| ((pos.line, pos.column), ctx.apply(ty)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::infer_program;
    use crate::testutil::{check, ok};
    use osprey_syntax::parse_program;

    /// One generic effect over a generic RECORD, differing only in the
    /// instantiation the handler answers at.
    fn stash(handler_payload: &str) -> String {
        format!(
            "type Box<T> = {{ value: T }}\n\
             effect Stash<T> {{\n\
               put: fn(T) -> Unit\n\
               take: fn() -> T\n\
             }}\n\
             fn stash() = perform Stash.put(Box {{ value: 42 }})\n\
             fn main() -> Unit = {{\n\
               let cached = handle Stash\n\
                 put v => print(\"put\")\n\
                 take => Box {{ value: {handler_payload} }}\n\
               in stash()\n\
               print(\"${{cached}}\")\n\
             }}\n"
        )
    }

    #[test]
    fn a_handler_discharges_a_generic_record_instance_only_at_its_own_instantiation() {
        // The effect-row site keys that match a perform to a handler are built
        // by RENDERING the argument type, so a rendering that collapsed a
        // declared record to its bare name made `Box<int>` and `Box<string>`
        // ONE key — and the mismatch below type-checked with zero errors.
        //
        // Both directions are asserted here because the negative alone is
        // one-sided: a checker that rejected every generic record instance
        // would satisfy it while breaking the language. The end-to-end
        // rejection lives in
        // `examples/failscompilation/generic_effect_record_arg_mismatch.ospo`.
        let matching = check(&stash("0"));
        assert!(
            matching.is_empty(),
            "a handler at the SAME instance must discharge: {matching:?}"
        );

        let mismatched = check(&stash("\"cached text\""));
        assert!(
            !mismatched.is_empty(),
            "a handler at `Stash<Box<string>>` must not discharge a \
             `Stash<Box<int>>` operation"
        );
        // Exactly ONE error, naming the whole operation at its instantiation.
        // `!is_empty()` plus a field-name substring would also be satisfied by
        // an unrelated failure — a parse slip, a second spurious diagnostic —
        // so the mismatch would stop being what the test proves.
        let [only] = mismatched.as_slice() else {
            panic!("the instantiation mismatch is the ONLY thing wrong here: {mismatched:?}")
        };
        assert_eq!(
            only.message,
            "unhandled effect operations at program entry: \
             Stash<{ value: int }>.put; add a matching `handle`",
            "the diagnostic must name the operation at its instantiation \
             structurally, not collapse it to `Box`"
        );
    }

    #[test]
    fn module_bodies_are_checked_in_a_child_scope() {
        let errs = check(
            "module Math {\n\
               fn square(x: int) -> int = (x * x) ?: 0\n\
             }\n",
        );
        assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
        // A type error inside a module body is still reported.
        let errs = check(
            "module Bad {\n\
               let y: int = \"not an int\"\n\
             }\n",
        );
        assert!(errs.iter().any(|e| e.message.contains("type mismatch")));
        // Module functions must run both declaration collection and body
        // checking; historically only module lets reached inference.
        let errs = check(
            "module BadFn {\n\
               fn broken() -> int = \"not an int\"\n\
             }\n",
        );
        assert!(errs.iter().any(|e| e.message.contains("type mismatch")));
    }

    #[test]
    fn a_discarded_result_statement_is_rejected() {
        // A bare statement whose value is a `Result` throws the failure away —
        // the one place the wrapper can vanish without anyone naming it.
        let errs = check(
            "fn risky(n: int) -> Result<int, MathError> = n + 1\n\
             fn go() -> int = {\n\
               risky(1)\n\
               0\n\
             }\n",
        );
        assert!(errs
            .iter()
            .any(|e| e.message.contains("cannot be discarded")));
        ok("fn risky(n: int) -> Result<int, MathError> = n + 1\n\
            fn go() -> int = {\n\
              let handled = risky(1) ?: 0\n\
              handled\n\
            }\n");
    }

    #[test]
    fn testing_builtins_typecheck_and_reject_bad_arity() {
        // [TESTING-BUILTINS] all assertion schemes accept the documented shapes.
        let errs = check(
            "test(\"adds\", fn() => expect(1 + 1, 2))\n\
             test(\"labeled\", fn() => check(\"sum\", 4, 2 + 2))\n\
             test(\"predicates\", fn() => {\n\
               expectTrue(2 < 3)\n\
               expectFalse(3 < 2)\n\
               checkTrue(\"ordered\", 4 <= 4)\n\
               checkFalse(\"different\", 4 == 5)\n\
               expectAll([true, 2 < 3, 4 == 4])\n\
               checkAll(\"batch\", [true, 5 != 6])\n\
             })\n",
        );
        assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
        let errs = check("expect(1)\n");
        assert!(errs.iter().any(|e| e.message.contains("arity")));
        let errs = check("expectTrue(1)\n");
        assert!(errs.iter().any(|e| e.message.contains("type mismatch")));
        let errs = check("checkFalse(true)\n");
        assert!(errs.iter().any(|e| e.message.contains("arity")));
        let errs = check("expectAll([true, 1])\n");
        assert!(errs.iter().any(|e| e.message.contains("type mismatch")));
        let errs = check("checkAll(\"batch\", [true])\n");
        assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
        let errs = check("test(42, fn() => expect(1, 1))\n");
        assert!(
            !errs.is_empty(),
            "a non-string test name must not typecheck"
        );
    }

    #[test]
    fn testing_builtins_are_shadowable_but_others_stay_reserved() {
        // [TESTING-SHADOWING] a user test/expect/check replaces the built-in;
        // every other built-in still rejects redefinition.
        let errs = check(
            "fn check(t: int) -> int = (t + 1) ?: 0\n\
             fn expect(a: int) -> int = a\n\
             fn test(x: int) -> int = x\n\
             let r = check(expect(test(1)))\n",
        );
        assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
        let errs = check("fn range(t: int) -> int = t\n");
        assert!(errs
            .iter()
            .any(|e| e.message.contains("cannot redefine built-in")));
    }

    #[test]
    fn receiver_directed_size_builtins_reject_unsupported_types() {
        // [BUILTIN-COLLECTION-LENGTH] [BUILTIN-COLLECTION-ISEMPTY]
        ok("let xs = [1, 2]\n\
            let scores = { \"alice\": 1 }\n\
            let a = length(\"abc\")\n\
            let b = xs.length()\n\
            let c = scores |> isEmpty\n");
        ok("fn has_bytes(source) = length(source) > 0 && byteLength(source) > 0\n");
        for source in [
            "let n = length(42)\n",
            "let n = isEmpty(false)\n",
            "type R = { value: int }\nlet n = length(R { value: 1 })\n",
            "let n = isEmpty(fn(x) => x)\n",
        ] {
            let errs = check(source);
            assert!(
                errs.iter().any(|e| e
                    .message
                    .contains("supports only string, List<T>, or Map<string, V>")),
                "expected receiver rejection for {source:?}, got {errs:?}"
            );
        }
    }

    #[test]
    fn string_conversion_builtins_reject_runtime_handles() {
        // [BUILTIN-PRINT] [BUILTIN-TOSTRING]
        ok("fn noop() -> Unit = sleep(0)\n\
            let a = print(1)\n\
            let b = print(1.5)\n\
            let c = print(true)\n\
            let d = print(\"text\")\n\
            let e = print(noop())\n\
            let f = toString(intDiv(4, 2))\n\
            extern fn textResult() -> Result<string, string>\n\
            let g = print(textResult())\n");
        for source in [
            "type R = { value: int }\nlet s = toString(R { value: 1 })\n",
            "let s = print([1, 2])\n",
            "let m = { \"one\": 1 }\nlet s = toString(m)\n",
            "let s = print(fn(x) => x)\n",
            "let s = toString(range(0, 2))\n",
            "let s = print(spawn 1)\n",
            "let c = Channel(1)\nlet s = toString(c)\n",
            "extern fn raw() -> Ptr\nlet s = print(raw())\n",
            "extern fn bad() -> Result<List<int>, Error>\nlet s = toString(bad())\n",
            "type R = { value: int }\nextern fn bad() -> Result<int, R>\nlet s = print(bad())\n",
        ] {
            let errs = check(source);
            assert!(
                errs.iter()
                    .any(|e| e.message.contains("cannot convert value")),
                "expected string-conversion rejection for {source:?}, got {errs:?}"
            );
        }
    }

    #[test]
    fn runtime_callbacks_use_their_exact_function_types() {
        ok("fn event(pid: int, kind: int, data: string) -> Unit = print(data)\n\
            let process = spawnProcess(\"echo ok\", event)\n\
            fn request(method: string, path: string, headers: string, body: string) -> HttpResponse = HttpResponse { status: 200, headers: headers, contentType: \"text/plain\", streamFd: -1, isComplete: true, partialBody: body }\n\
            let listening = httpListen(1, request)\n");
        let process_errs = check(
            "fn wrong(pid: int) -> Unit = print(pid)\n\
             let process = spawnProcess(\"echo no\", wrong)\n",
        );
        assert!(process_errs
            .iter()
            .any(|e| e.message.contains("function arity mismatch")));
        let http_errs = check(
            "fn wrong(method: string, path: string, headers: string, body: string) = body\n\
             let listening = httpListen(1, wrong)\n",
        );
        assert!(http_errs
            .iter()
            .any(|e| e.message.contains("type mismatch")));
    }

    #[test]
    fn declared_typaram_functions_generalize_regardless_of_binding_direction() {
        // Regression lock for the builtin binder-id collision: builtin schemes
        // quantify hand-written Var(0)/Var(1); when the fresh supply also
        // handed out those ids, a var-var unification routed through them made
        // `TypeEnv::free_vars` resolve THROUGH a builtin's binder, and
        // `fn identity<T>(x) -> T = x` silently lost its polymorphism (the
        // failure depended on which side of the unification held the typaram
        // var). All three annotation spellings must stay polymorphic across
        // two instantiations. See `builtins::RESERVED_SCHEME_VARS`.
        ok("fn id1<T>(x: T) -> T = x\n\
            print(\"${id1(5)} ${id1(\"hi\")}\")\n");
        ok("fn id2<T>(x: T) = x\n\
            print(\"${id2(5)} ${id2(\"hi\")}\")\n");
        ok("fn id3<T>(x) -> T = x\n\
            print(\"${id3(5)} ${id3(\"hi\")}\")\n");
        // The HOF shape that first exposed it: a generic fn passed by name to
        // an inferred HOF, applied at two different instantiations.
        ok("fn identity<T>(x: T) -> T = x\n\
            fn apply(f, x) = f(x)\n\
            print(\"${apply(identity, 5)} ${apply(identity, \"hi\")}\")\n");
    }

    #[test]
    fn resume_inside_an_arm_lambda_is_a_type_error() {
        // A lambda body runs when called, not where it is written, so the
        // arm's continuation is not live inside it ([EFFECTS-RESUME]).
        let errs = check(
            "effect E { op: fn() -> int }\n\
             fn go() -> int !E = perform E.op()\n\
             let r = handle E\n\
                 op => {\n\
                     let f = |x| => resume(x)\n\
                     f(9)\n\
                 }\n\
             in go()\n",
        );
        assert!(
            errs.iter().any(|e| e
                .message
                .contains("`resume` is only valid inside a handler arm")),
            "expected the lambda-resume rejection, got: {errs:?}"
        );
    }

    #[test]
    fn assignment_to_undeclared_name_is_an_error() {
        let errs = check("fn main() -> Unit = {\n  neverDeclared = 100\n}\n");
        assert!(errs.iter().any(|e| e
            .message
            .contains("assignment to undeclared `neverDeclared`")));
    }

    #[test]
    fn extern_declarations_register_signatures() {
        // An `extern fn` exercises `collect_extern`, `record_fn_params` over
        // `ExternParameter`, and the published signature used at the call site.
        ok("extern fn c_add(a: int, b: int) -> int\n\
            fn use_it() -> int = c_add(1, 2)\n");
        // An extern with no declared return type defaults to Unit.
        ok("extern fn c_log(msg: string)\n\
            fn use_log() -> Unit = c_log(\"x\")\n");
    }

    #[test]
    fn unimplemented_validated_record_syntax_is_rejected() {
        let errs = check(
            "fn validate(value) = true\n\
             type Item = { value: int } where validate\n",
        );
        assert!(errs.iter().any(|e| e
            .message
            .contains("validated record `where` is not supported")));
    }

    #[test]
    fn infer_program_publishes_functions_and_unions() {
        let parsed = parse_program(
            "type Color = Red | Green\n\
             fn dbl(x: int) -> int = x * 2\n\
             let g = fn(n) => n + 1\n\
             let n = dbl(21)\n",
        );
        let info = infer_program(&parsed.program);
        // Resolved function signatures and union tags are published.
        assert!(info.functions.contains_key("dbl"));
        assert_eq!(
            info.unions.get("Color").map(Vec::len),
            Some(2),
            "Color variants published"
        );
        // The lambda's resolved type is published keyed by source position.
        assert!(!info.lambdas.is_empty(), "lambda type published");
        // The unannotated `let n = dbl(21)` resolves to `int`, published by
        // position. Implements [LSP-HOVER-VARIABLES]
        assert!(
            info.lets.values().any(|t| t.to_string() == "int"),
            "let type published: {:?}",
            info.lets
        );
    }

    #[test]
    fn conflicting_effect_site_keys_are_dropped() {
        // Two sites sharing one (line, column) key with DIFFERENT resolutions
        // (string-interpolation fragments re-parse at fragment-relative
        // positions) must both be dropped; agreeing duplicates are kept.
        // Implements [EFFECTS-GENERIC-INSTANTIATION].
        use crate::info::PerformSite;
        use crate::ty::Type;
        let site = |t: Type| PerformSite {
            op: crate::info::OpType {
                params: Vec::new(),
                ret: t.clone(),
            },
            effect_args: vec![t],
        };
        let entries = vec![
            ((1, 0), site(Type::int())),
            ((1, 0), site(Type::string())),
            ((1, 0), site(Type::int())),
            ((2, 4), site(Type::bool())),
            ((3, 1), site(Type::unit())),
            ((3, 1), site(Type::unit())),
        ];
        let out = super::dedupe_sites(entries.into_iter());
        assert!(!out.contains_key(&(1, 0)), "conflicted key must be dropped");
        assert!(out.contains_key(&(2, 4)));
        assert!(out.contains_key(&(3, 1)), "agreeing duplicates are kept");
    }

    #[test]
    fn single_variant_same_name_type_is_a_record() {
        // `type Foo = Foo` is the single-variant record form (`is_record`); using
        // the bare name is a record value, and `infer_program` publishes it.
        let parsed = parse_program("type Foo = Foo\nlet x = Foo\n");
        let info = infer_program(&parsed.program);
        let foo = info.ctors.get("Foo").expect("Foo ctor published");
        assert!(foo.owner_is_record);
    }
}
