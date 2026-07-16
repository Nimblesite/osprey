//! Algebraic effects: `effect` declarations, `handle … in …` and `perform`.
//! Each `handle` arm becomes a top-level handler function; entering the
//! `handle` pushes those functions onto the C runtime's handler stack
//! (`__osprey_handler_push`, keyed by effect+operation name) and leaving pops
//! them, so a `perform` in any (even forward-referenced) function resolves the
//! innermost active handler dynamically via `__osprey_handler_lookup` and an
//! indirect call. The example handlers never `resume`, so an arm is an ordinary
//! function returning the operation's result.

use crate::builder::{CellSlot, Codegen, ResumeCodegenContext};
use crate::cast::coerce_to;
use crate::conv::unbox_from_i64;
use crate::error::Result;
use crate::expr::gen_expr;
use crate::freevars::free_idents;
use crate::llty::{LType, Value};
use crate::types::{ltype_of, result_inner};
use osprey_ast::{contains_resume, Expr, HandlerArm, MatchArm, Stmt};
use std::collections::{BTreeSet, HashSet};

/// A parsed effect-operation signature: parameter types, the result LLVM type,
/// and (when the result is `Result<T, _>`) the success inner type. A generic
/// effect's type-parameter slots are ERASED — they travel as boxed `i64` and
/// the `*_erased` flags mark which slots must box/unbox at the boundaries.
/// Implements [EFFECTS-GENERIC-RUNTIME].
#[derive(Clone)]
pub(crate) struct OpSig {
    pub params: Vec<LType>,
    pub ret: LType,
    pub ret_result_inner: Option<LType>,
    /// Per-parameter: whether the declared type is an effect type parameter.
    pub param_erased: Vec<bool>,
    /// Whether the declared result is an effect type parameter.
    pub ret_erased: bool,
}

impl OpSig {
    /// A default all-`i64` signature for `arity` parameters — the fallback when
    /// inference recorded no resolved signature for an effect operation.
    fn default_for_arity(arity: usize) -> Self {
        OpSig {
            params: vec![LType::I64; arity],
            ret: LType::I64,
            ret_result_inner: None,
            param_erased: vec![false; arity],
            ret_erased: false,
        }
    }

    /// The handler function's LLVM return-type spelling (the Result block
    /// pointer for a Result result, else the plain type).
    fn ret_ty(&self) -> String {
        crate::llty::ret_spelling(self.ret, self.ret_result_inner)
    }

    /// The handler function-pointer type. Every arm takes a hidden leading
    /// `i8* env` (its captured cells + values), e.g. `i64 (i8*, i64)*`.
    fn fn_ptr_ty(&self) -> String {
        let mut parts = vec!["i8*".to_string()];
        parts.extend(self.params.iter().map(LType::to_string));
        format!("{} ({})*", self.ret_ty(), parts.join(", "))
    }
}

/// Emit `ret <ty> <operand>` for `ret` and close out the nested function whose
/// LLVM return type matches `sig`.
fn ret_and_exit(
    cg: &mut Codegen,
    saved: crate::builder::SavedFn,
    sig: &OpSig,
    name: &str,
    params: &[(LType, String)],
    ret: &Value,
) {
    // Nested-function epilogue: the return transfers +1, owned locals drop
    // [GC-ARC-PERCEUS].
    crate::arc::epilogue(cg, Some(ret));
    cg.emit(format!("ret {} {}", ret.llvm_ty(), ret.operand));
    cg.exit_nested_fn(saved, &sig.ret_ty(), name, params);
}

/// Bind each of an arm's operation parameters as an SSA value (`%name`, typed
/// from `sig`, defaulting to `i64`) and append it to the emitted `params`
/// list. An erased (generic) slot arrives as a boxed `i64` and is unboxed to
/// the type inference resolved for this handle site. Implements
/// [EFFECTS-GENERIC-RUNTIME].
fn bind_arm_params(
    cg: &mut Codegen,
    arm: &HandlerArm,
    sig: &OpSig,
    resolved: Option<&osprey_types::OpType>,
    params: &mut Vec<(LType, String)>,
) {
    for (i, pname) in arm.params.iter().enumerate() {
        let pty = sig.params.get(i).copied().unwrap_or(LType::I64);
        let erased = sig.param_erased.get(i).copied().unwrap_or(false);
        let bound = match resolved.and_then(|r| r.params.get(i)).filter(|_| erased) {
            Some(rt) => crate::effect_generics::unbox_erased(cg, &format!("%{pname}"), rt),
            None => Value::new(format!("%{pname}"), pty),
        };
        cg.bind(pname.clone(), bound);
        params.push((pty, pname.clone()));
    }
}

/// The free identifiers an arm's body closes over, minus the arm's own
/// parameters — the names that must be captured from the enclosing scope.
fn arm_free_idents(arm: &HandlerArm) -> impl Iterator<Item = String> + '_ {
    let mut free = BTreeSet::new();
    free_idents(&arm.body, &mut free);
    free.into_iter().filter(|n| !arm.params.contains(n))
}

/// One binding shared by every arm of a single `handle` region, captured into
/// the region's environment.
enum ArmCap {
    /// A handler-captured mutable: the env carries the heap cell's `i8*` pointer
    /// so arms `load`/`store` the same slot — handler-owned state. `ptr` is the
    /// cell pointer in the enclosing scope (a `{pointee}*` operand).
    Cell {
        name: String,
        ptr: String,
        pointee: LType,
        osp_ty: Option<String>,
    },
    /// Any other free variable: captured by value, closure-style.
    Val { name: String, val: Value },
}

impl ArmCap {
    /// The env-slot LLVM type: a cell travels as its `i8*` pointer, a value as
    /// its own travelling type.
    fn slot_ty(&self) -> String {
        match self {
            ArmCap::Cell { .. } => "i8*".to_string(),
            ArmCap::Val { val, .. } => val.llvm_ty(),
        }
    }
}

/// Build an [`OpSig`] from inference's resolved operation signature — the one
/// source of truth for effect types (no string re-parsing in the backend).
pub(crate) fn op_sig_of(op: &osprey_types::OpType) -> OpSig {
    // A slot mentioning a type parameter ANYWHERE (a bare `T` or a nested
    // `Result<T, string>`) is erased: it travels as one boxed `i64`, boxed
    // and unboxed against each site's resolved instantiation — a nested
    // parameter changes the slot's concrete shape per instantiation just as
    // a top-level one does. Implements [EFFECTS-GENERIC-RUNTIME].
    let ret_erased = osprey_types::has_type_var(&op.ret);
    let inner = if ret_erased {
        None
    } else {
        result_inner(&op.ret)
    };
    let ret = if ret_erased {
        LType::I64
    } else if inner.is_some() {
        LType::Ptr
    } else {
        ltype_of(&op.ret)
    };
    OpSig {
        params: op
            .params
            .iter()
            .map(|t| {
                if osprey_types::has_type_var(t) {
                    LType::I64
                } else {
                    ltype_of(t)
                }
            })
            .collect(),
        ret,
        ret_result_inner: inner,
        param_erased: op.params.iter().map(osprey_types::has_type_var).collect(),
        ret_erased,
    }
}

/// The resolved [`OpSig`] for `effect.operation`, falling back to an all-`i64`
/// signature of the arm's arity when inference recorded none.
fn op_sig_for(cg: &Codegen, effect: &str, arm: &HandlerArm) -> OpSig {
    let key = format!("{effect}.{}", arm.operation);
    cg.effect_op(&key)
        .unwrap_or_else(|| OpSig::default_for_arity(arm.params.len()))
}

fn declare_stack(cg: &mut Codegen) {
    cg.add_extern("declare i32 @__osprey_handler_push(i8*, i8*, i8*, i8*)");
    cg.add_extern("declare i32 @__osprey_handler_pop()");
    cg.add_extern("declare i8* @__osprey_handler_lookup(i8*, i8*)");
    cg.add_extern("declare i8* @__osprey_handler_lookup_env(i8*, i8*)");
}

/// Branch on a null handler pointer: print `unhandled effect: <key>.<op>` and
/// exit, so a missed lookup fails loudly instead of calling null. Implements
/// [EFFECTS-GENERIC-RUNTIME].
fn emit_unhandled_guard(cg: &mut Codegen, raw: &str, lookup_key: &str, operation: &str) {
    cg.add_extern("declare i32 @puts(i8*)");
    cg.add_extern("declare void @exit(i32)");
    let msg = cg.string_constant(&format!("unhandled effect: {lookup_key}.{operation}"));
    let is_null = cg.emit_reg(format!("icmp eq i8* {raw}, null"));
    let abort_lbl = cg.fresh_label();
    let ok_lbl = cg.fresh_label();
    cg.emit(format!(
        "br i1 {is_null}, label %{abort_lbl}, label %{ok_lbl}"
    ));
    cg.start_block(&abort_lbl);
    let p = cg.fresh_reg();
    cg.emit(format!("{p} = call i32 @puts(i8* {})", msg.operand));
    cg.emit("call void @exit(i32 1)");
    cg.emit("unreachable");
    cg.start_block(&ok_lbl);
}

fn declare_coro(cg: &mut Codegen) {
    cg.add_extern("declare i8* @__osprey_coro_new(i8*)");
    cg.add_extern("declare void @__osprey_coro_start(i8*, i64 (i8*)*, i8*, i8*)");
    cg.add_extern("declare i64 @__osprey_coro_suspend(i8*, i64, i64*, i64)");
    cg.add_extern("declare i64 @__osprey_coro_resume(i8*, i64)");
    cg.add_extern("declare i64 @__osprey_coro_done(i8*)");
    cg.add_extern("declare i64 @__osprey_coro_op(i8*)");
    cg.add_extern("declare i64 @__osprey_coro_arg(i8*, i64)");
    cg.add_extern("declare i64 @__osprey_coro_result(i8*)");
    cg.add_extern("declare void @__osprey_coro_abort(i8*)");
    cg.add_extern("declare void @__osprey_coro_free(i8*)");
    cg.add_extern("declare i8* @__osprey_handler_snapshot()");
}

/// Mutable locals that an effect handler arm captures from an enclosing scope —
/// the set promoted to shared heap cells so a plain `mut` becomes a reference
/// cell the handler owns (`get`/`set` arms and the outer scope share one slot).
/// `cell_vars = {mutable bindings} ∩ {names a handler arm references freely}`.
/// Implements [EFFECTS-HANDLER-STATE].
pub(crate) fn captured_mut_vars(body: &Expr) -> HashSet<String> {
    let (mut muts, mut captured) = (BTreeSet::new(), BTreeSet::new());
    scan_expr(body, &mut muts, &mut captured);
    muts.intersection(&captured).cloned().collect()
}

/// As [`captured_mut_vars`] but over the trailing top-level statements that
/// synthesize `main` when there is no user `main`.
pub(crate) fn captured_mut_vars_in_stmts(stmts: &[&Stmt]) -> HashSet<String> {
    let (mut muts, mut captured) = (BTreeSet::new(), BTreeSet::new());
    for s in stmts {
        scan_stmt(s, &mut muts, &mut captured);
    }
    muts.intersection(&captured).cloned().collect()
}

// A purpose-built AST walk (parallel in *shape* to `freevars::walk`, but
// collecting two different sets: every mutable binding and the names handler
// arms reference freely). The handler-arm free idents themselves come from
// `free_idents` so that one definition of "what does this close over" stays in
// `freevars`; only the find-the-handlers/muts traversal lives here.
fn scan_expr(e: &Expr, muts: &mut BTreeSet<String>, captured: &mut BTreeSet<String>) {
    match e {
        Expr::Handler { arms, body, .. } => {
            for arm in arms {
                captured.extend(arm_free_idents(arm));
                scan_expr(&arm.body, muts, captured);
            }
            scan_expr(body, muts, captured);
        }
        Expr::Block { statements, value } => {
            for s in statements {
                scan_stmt(s, muts, captured);
            }
            if let Some(v) = value {
                scan_expr(v, muts, captured);
            }
        }
        Expr::Match { value, arms } => {
            scan_expr(value, muts, captured);
            scan_arms(arms, muts, captured);
        }
        Expr::Select { arms } => scan_arms(arms, muts, captured),
        _ => scan_children(e, muts, captured),
    }
}

fn scan_arms(arms: &[MatchArm], muts: &mut BTreeSet<String>, captured: &mut BTreeSet<String>) {
    scan_slice(arms, muts, captured, |arm| &arm.body);
}

/// Recurse into every child expression of `e` (the variants that are not
/// special-cased in [`scan_expr`]), so a handler/mut nested anywhere is found.
fn scan_children(e: &Expr, muts: &mut BTreeSet<String>, captured: &mut BTreeSet<String>) {
    match e {
        Expr::InterpolatedStr(parts) => {
            for p in parts {
                if let osprey_ast::InterpolatedPart::Expr(x) = p {
                    scan_expr(x, muts, captured);
                }
            }
        }
        Expr::List(xs) => scan_slice(xs, muts, captured, |x| x),
        Expr::Map(es) => {
            for en in es {
                scan_expr(&en.key, muts, captured);
                scan_expr(&en.value, muts, captured);
            }
        }
        Expr::Object(fs)
        | Expr::TypeConstructor { fields: fs, .. }
        | Expr::Update { fields: fs, .. } => {
            scan_slice(fs, muts, captured, |f| &f.value);
        }
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            scan_expr(left, muts, captured);
            scan_expr(right, muts, captured);
        }
        Expr::Unary { operand, .. } => scan_expr(operand, muts, captured),
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => {
            scan_expr(function, muts, captured);
            scan_slice(arguments, muts, captured, |x| x);
            scan_slice(named_arguments, muts, captured, |n| &n.value);
        }
        Expr::MethodCall {
            target,
            arguments,
            named_arguments,
            ..
        } => {
            scan_expr(target, muts, captured);
            scan_slice(arguments, muts, captured, |x| x);
            scan_slice(named_arguments, muts, captured, |n| &n.value);
        }
        Expr::FieldAccess { target, .. } => scan_expr(target, muts, captured),
        Expr::Index { target, index } => {
            scan_expr(target, muts, captured);
            scan_expr(index, muts, captured);
        }
        Expr::Lambda { body, .. } | Expr::Spawn(body) | Expr::Await(body) | Expr::Recv(body) => {
            scan_expr(body, muts, captured);
        }
        Expr::Yield(Some(x)) => scan_expr(x, muts, captured),
        Expr::Send { channel, value } => {
            scan_expr(channel, muts, captured);
            scan_expr(value, muts, captured);
        }
        Expr::Perform {
            arguments,
            named_arguments,
            ..
        } => {
            scan_slice(arguments, muts, captured, |x| x);
            scan_slice(named_arguments, muts, captured, |n| &n.value);
        }
        Expr::Resume(Some(value)) => scan_expr(value, muts, captured),
        _ => {}
    }
}

/// Recurse into each element of `items`, projecting it to its sub-expression
/// with `pick`. The one place the effect scanner fans out over a collection
/// node, threading `muts`/`captured` through [`osprey_ast::walk_each`].
fn scan_slice<T>(
    items: &[T],
    muts: &mut BTreeSet<String>,
    captured: &mut BTreeSet<String>,
    pick: impl Fn(&T) -> &Expr,
) {
    osprey_ast::walk_each(items, &mut (muts, captured), pick, |e, (m, c)| {
        scan_expr(e, m, c);
    });
}

fn scan_stmt(s: &Stmt, muts: &mut BTreeSet<String>, captured: &mut BTreeSet<String>) {
    match s {
        Stmt::Let {
            name,
            value,
            mutable,
            ..
        } => {
            if *mutable {
                let _ = muts.insert(name.clone());
            }
            scan_expr(value, muts, captured);
        }
        Stmt::Assignment { name, value, .. } => {
            let _ = muts.insert(name.clone());
            scan_expr(value, muts, captured);
        }
        Stmt::Expr { value, .. } => scan_expr(value, muts, captured),
        _ => {}
    }
}

/// `handle Effect arm… in body` — capture the region's environment (the cells
/// and values its arms reference), emit a handler function per arm bound to that
/// env, push them on the runtime stack for the duration of `body`, then pop.
pub(crate) fn gen_handler(
    cg: &mut Codegen,
    effect: &str,
    arms: &[HandlerArm],
    body: &Expr,
    position: Option<osprey_ast::Position>,
) -> Result<Value> {
    declare_stack(cg);
    let site_ops = crate::effect_generics::site_handler_ops(cg, position);
    if arms.iter().any(|arm| contains_resume(&arm.body)) {
        return gen_resuming_handler(cg, effect, arms, body, site_ops.as_ref());
    }
    // A generic effect's handler registers under its instantiation-mangled
    // key, so only same-instantiation performs resolve to it. Implements
    // [EFFECTS-GENERIC-RUNTIME].
    let key = site_ops.as_ref().map_or_else(
        || effect.to_string(),
        |s| crate::effect_generics::runtime_effect_key(effect, &s.effect_args),
    );
    let caps = capture_list(cg, arms);
    let (env, env_ty) = build_env(cg, &caps);
    for arm in arms {
        let sig = op_sig_for(cg, effect, arm);
        let resolved = site_ops.as_ref().and_then(|m| m.ops.get(&arm.operation));
        let id = cg.next_handler_id();
        let fn_name = format!("__handler_{effect}_{}_{id}", arm.operation);
        emit_handler_fn(cg, &fn_name, arm, &sig, resolved, &caps, &env_ty)?;
        let eff_s = cg.string_constant(&key);
        let op_s = cg.string_constant(&arm.operation);
        let fp = cg.fresh_reg();
        cg.emit(format!(
            "{fp} = bitcast {} @{fn_name} to i8*",
            sig.fn_ptr_ty()
        ));
        let r = cg.fresh_reg();
        cg.emit(format!(
            "{r} = call i32 @__osprey_handler_push(i8* {}, i8* {}, i8* {fp}, i8* {env})",
            eff_s.operand, op_s.operand
        ));
    }

    let result = gen_expr(cg, body)?;

    for _ in arms {
        let r = cg.fresh_reg();
        cg.emit(format!("{r} = call i32 @__osprey_handler_pop()"));
    }
    // The popped region's env reached its structural end: drop it (its mask
    // releases the captured values) [GC-ARC-PERCEUS], plan 0011 M5.
    if env != "null" {
        crate::arc::release_operand(cg, &env);
    }
    Ok(result)
}

/// The bindings every arm of this region captures, in stable (sorted) order: a
/// handler-captured mutable becomes a shared [`ArmCap::Cell`]; any other bound
/// free variable is captured by value. Names that resolve to nothing in scope
/// (top-level functions, constructors) need no capture — the arm resolves them
/// directly.
fn arms_free_idents(arms: &[HandlerArm]) -> BTreeSet<String> {
    arms.iter().flat_map(arm_free_idents).collect()
}

fn capture_list(cg: &Codegen, arms: &[HandlerArm]) -> Vec<ArmCap> {
    caps_from_names(cg, arms_free_idents(arms))
}

fn capture_list_resuming(cg: &Codegen, arms: &[HandlerArm], body: &Expr) -> Vec<ArmCap> {
    let mut names = arms_free_idents(arms);
    free_idents(body, &mut names);
    caps_from_names(cg, names)
}

fn caps_from_names(cg: &Codegen, names: BTreeSet<String>) -> Vec<ArmCap> {
    names
        .into_iter()
        .filter_map(|name| {
            if let Some(slot) = cg.cell_slots.get(&name) {
                Some(ArmCap::Cell {
                    name,
                    ptr: slot.ptr.clone(),
                    pointee: slot.pointee,
                    osp_ty: slot.osp_ty.clone(),
                })
            } else {
                cg.lookup(&name).map(|val| ArmCap::Val { name, val })
            }
        })
        .collect()
}

/// Allocate the region's environment cell and store each capture into it,
/// returning its `i8*` handle and the struct type. A capture-free region uses a
/// `null` env (the arms ignore it).
fn build_env(cg: &mut Codegen, caps: &[ArmCap]) -> (String, String) {
    if caps.is_empty() {
        return ("null".to_string(), String::new());
    }
    let env_ty = format!(
        "{{ {} }}",
        caps.iter()
            .map(ArmCap::slot_ty)
            .collect::<Vec<_>>()
            .join(", ")
    );
    // Layout word: cell captures are `i8*` pointers to heap mut-cells, value
    // captures mark themselves by slot type ([`crate::meta`]).
    let mf: Vec<_> = caps
        .iter()
        .map(|c| crate::meta::MetaField::of_slot_ty(&c.slot_ty()))
        .collect();
    let cell = cg.malloc_struct(&env_ty, crate::meta::struct_meta(&mf));
    for (i, c) in caps.iter().enumerate() {
        let slot_ty = c.slot_ty();
        let p = cg.emit_reg(format!(
            "getelementptr {env_ty}, {env_ty}* {cell}, i32 0, i32 {i}"
        ));
        let operand = store_operand(cg, c);
        // The env's drop mask releases each captured pointer [GC-ARC-PERCEUS].
        crate::arc::dup_store(cg, &slot_ty, &operand);
        cg.emit(format!("store {slot_ty} {operand}, {slot_ty}* {p}"));
    }
    let env = cg.emit_reg(format!("bitcast {env_ty}* {cell} to i8*"));
    (env, env_ty)
}

/// The operand stored into the env slot for a capture: a cell's `i8*` pointer
/// (the heap slot, shared so arms mutate the same location), or a value's
/// operand.
fn store_operand(cg: &mut Codegen, c: &ArmCap) -> String {
    match c {
        ArmCap::Cell { ptr, pointee, .. } => {
            let ty = pointee.as_str();
            cg.emit_reg(format!("bitcast {ty}* {ptr} to i8*"))
        }
        ArmCap::Val { val, .. } => val.operand.clone(),
    }
}

/// Emit a top-level handler function for one arm: a hidden leading `i8* %__env`
/// it reloads its captures from, then the operation's own parameters; its body
/// is the arm body coerced to the operation's result.
fn emit_handler_fn(
    cg: &mut Codegen,
    name: &str,
    arm: &HandlerArm,
    sig: &OpSig,
    resolved: Option<&osprey_types::OpType>,
    caps: &[ArmCap],
    env_ty: &str,
) -> Result<()> {
    let saved = cg.enter_nested_fn();
    let mut params = vec![(LType::Ptr, String::from("__env"))];
    reload_env(cg, caps, env_ty);
    bind_arm_params(cg, arm, sig, resolved, &mut params);
    let body = gen_expr(cg, &arm.body)?;
    let ret = if sig.ret_erased {
        // An erased (generic) result returns boxed — pointer-ness leaves the
        // arm's frame, so dup before the epilogue drops it [GC-ARC-PERCEUS].
        crate::arc::escape_retain(cg, &body);
        // The perform site unboxes it to its resolved type. Implements
        // [EFFECTS-GENERIC-RUNTIME].
        crate::effect_generics::box_erased(cg, body, resolved.map(|r| &r.ret))
    } else if let Some(inner) = sig.ret_result_inner {
        if body.result_inner.is_some() {
            body
        } else {
            crate::result::make_ok(cg, body, inner)?
        }
    } else {
        coerce_to(cg, body, sig.ret)?
    };
    ret_and_exit(cg, saved, sig, name, &params, &ret);
    Ok(())
}

/// Inside an arm function: cast `%__env` back to the region's struct and rebuild
/// each capture — a [`ArmCap::Cell`] as a live cell slot (so reads `load` and
/// reassignments `store` the shared heap location), a value by binding its
/// reloaded register.
fn reload_env(cg: &mut Codegen, caps: &[ArmCap], env_ty: &str) {
    if caps.is_empty() {
        return;
    }
    let env = cg.emit_reg(format!("bitcast i8* %__env to {env_ty}*"));
    for (i, c) in caps.iter().enumerate() {
        let slot_ty = c.slot_ty();
        let p = cg.emit_reg(format!(
            "getelementptr {env_ty}, {env_ty}* {env}, i32 0, i32 {i}"
        ));
        let loaded = cg.emit_reg(format!("load {slot_ty}, {slot_ty}* {p}"));
        match c {
            ArmCap::Cell {
                name,
                pointee,
                osp_ty,
                ..
            } => {
                let ptr = cg.emit_reg(format!("bitcast i8* {loaded} to {}*", pointee.as_str()));
                let _ = cg.cell_slots.insert(
                    name.clone(),
                    CellSlot {
                        ptr,
                        pointee: *pointee,
                        osp_ty: osp_ty.clone(),
                    },
                );
            }
            ArmCap::Val { name, val } => {
                let mut v = val.clone();
                v.operand = loaded;
                cg.bind(name.clone(), v);
            }
        }
    }
}

#[derive(Clone)]
struct DriveArm {
    op_id: usize,
    operation: String,
    sig: OpSig,
    arm_fn: String,
}

/// `handle` region whose arms contain explicit `resume`: the handled body runs
/// on a body thread and each `perform` suspends into this host-side dispatcher.
fn gen_resuming_handler(
    cg: &mut Codegen,
    effect: &str,
    arms: &[HandlerArm],
    body: &Expr,
    site_ops: Option<&osprey_types::HandlerSite>,
) -> Result<Value> {
    declare_stack(cg);
    declare_coro(cg);
    // Same instantiation-mangled runtime key as the non-resuming path.
    // Implements [EFFECTS-GENERIC-RUNTIME].
    let key = site_ops.map_or_else(
        || effect.to_string(),
        |s| crate::effect_generics::runtime_effect_key(effect, &s.effect_args),
    );

    let caps = capture_list_resuming(cg, arms, body);
    let (env, env_ty) = build_env(cg, &caps);
    let id = cg.next_handler_id();
    let body_fn = format!("__resume_body_{effect}_{id}");
    let drive_fn = format!("__resume_drive_{effect}_{id}");

    let answer_ty = emit_resuming_body_fn(cg, &body_fn, body, &caps, &env_ty)?;
    let mut drive_arms = Vec::new();
    for (op_id, arm) in arms.iter().enumerate() {
        let sig = op_sig_for(cg, effect, arm);
        let resolved = site_ops.and_then(|m| m.ops.get(&arm.operation));
        let suspend_fn = format!("__resume_suspend_{effect}_{}_{id}_{op_id}", arm.operation);
        let arm_fn = format!("__resume_arm_{effect}_{}_{id}_{op_id}", arm.operation);
        emit_suspend_fn(cg, &suspend_fn, op_id, &sig);
        emit_resuming_arm_fn(
            cg,
            arm,
            &ArmFnSpec {
                name: &arm_fn,
                drive_fn: &drive_fn,
                answer_ty,
                sig: &sig,
                resolved,
                caps: &caps,
                env_ty: &env_ty,
            },
        )?;
        drive_arms.push(DriveArm {
            op_id,
            operation: arm.operation.clone(),
            sig,
            arm_fn,
        });
    }
    emit_drive_fn(cg, &drive_fn, &drive_arms);

    let coro = cg.call("i8*", "__osprey_coro_new", "i8*", &[&env]);
    for arm in &drive_arms {
        let suspend_fn = format!(
            "__resume_suspend_{effect}_{}_{id}_{}",
            arm.operation, arm.op_id
        );
        let eff_s = cg.string_constant(&key);
        let op_s = cg.string_constant(&arm.operation);
        let fp = cg.emit_reg(format!(
            "bitcast {} @{suspend_fn} to i8*",
            arm.sig.fn_ptr_ty()
        ));
        let _ = cg.call(
            "i32",
            "__osprey_handler_push",
            "i8*, i8*, i8*, i8*",
            &[&eff_s.operand, &op_s.operand, &fp, &coro],
        );
    }

    let snap = cg.call("i8*", "__osprey_handler_snapshot", "", &[]);
    cg.call_void(
        "__osprey_coro_start",
        "i8*, i64 (i8*)*, i8*, i8*",
        &[&coro, &format!("@{body_fn}"), &env, &snap],
    );
    let boxed = cg.emit_reg(format!("call i64 @{drive_fn}(i8* {env}, i8* {coro})"));

    for _ in arms {
        let _ = cg.call("i32", "__osprey_handler_pop", "", &[]);
    }
    cg.call_void("__osprey_coro_free", "i8*", &[&coro]);
    // The coro region ended: drop its env (mask releases the captures)
    // [GC-ARC-PERCEUS], plan 0011 M5.
    if env != "null" {
        crate::arc::release_operand(cg, &env);
    }
    let out = unbox_from_i64(cg, &boxed, answer_ty);
    // The body fn escape-retained its answer at the boxing site: own it here.
    crate::arc::own(cg, &out);
    Ok(out)
}

fn emit_resuming_body_fn(
    cg: &mut Codegen,
    name: &str,
    body: &Expr,
    caps: &[ArmCap],
    env_ty: &str,
) -> Result<LType> {
    let saved = cg.enter_nested_fn();
    reload_env(cg, caps, env_ty);
    let body_raw = gen_expr(cg, body)?;
    let body = crate::result::unwrap(cg, body_raw);
    let answer_ty = body.ty;
    let boxed = box_codegen_value(cg, body);
    crate::arc::epilogue(cg, None);
    cg.emit(format!("ret i64 {}", boxed.operand));
    cg.exit_nested_fn(saved, "i64", name, &[(LType::Ptr, String::from("__env"))]);
    Ok(answer_ty)
}

fn emit_suspend_fn(cg: &mut Codegen, name: &str, op_id: usize, sig: &OpSig) {
    let saved = cg.enter_nested_fn();
    let mut params = vec![(LType::Ptr, String::from("__coro"))];
    for (i, pty) in sig.params.iter().copied().enumerate() {
        params.push((pty, format!("__arg{i}")));
    }

    let args_ptr = if sig.params.is_empty() {
        String::from("null")
    } else {
        let arr_ty = format!("[{} x i64]", sig.params.len());
        let arr = cg.emit_reg(format!("alloca {arr_ty}"));
        for (i, pty) in sig.params.iter().copied().enumerate() {
            let value = Value::new(format!("%__arg{i}"), pty);
            let boxed = box_codegen_value(cg, value);
            let slot = cg.emit_reg(format!(
                "getelementptr {arr_ty}, {arr_ty}* {arr}, i64 0, i64 {i}"
            ));
            cg.emit(format!("store i64 {}, i64* {slot}", boxed.operand));
        }
        cg.emit_reg(format!(
            "getelementptr {arr_ty}, {arr_ty}* {arr}, i64 0, i64 0"
        ))
    };
    let raw = cg.call(
        "i64",
        "__osprey_coro_suspend",
        "i8*, i64, i64*, i64",
        &[
            "%__coro",
            &op_id.to_string(),
            &args_ptr,
            &sig.params.len().to_string(),
        ],
    );
    let ret = unbox_coro_value(cg, &raw, sig.ret, sig.ret_result_inner);
    ret_and_exit(cg, saved, sig, name, &params, &ret);
}

struct ArmFnSpec<'a> {
    name: &'a str,
    drive_fn: &'a str,
    answer_ty: LType,
    sig: &'a OpSig,
    resolved: Option<&'a osprey_types::OpType>,
    caps: &'a [ArmCap],
    env_ty: &'a str,
}

fn emit_resuming_arm_fn(cg: &mut Codegen, arm: &HandlerArm, spec: &ArmFnSpec<'_>) -> Result<()> {
    let saved = cg.enter_nested_fn();
    reload_env(cg, spec.caps, spec.env_ty);
    let mut params = vec![
        (LType::Ptr, String::from("__env")),
        (LType::Ptr, String::from("__coro")),
    ];
    bind_arm_params(cg, arm, spec.sig, spec.resolved, &mut params);
    let op_ret_is_result = spec.sig.ret_result_inner.is_some()
        || spec
            .resolved
            .is_some_and(|r| result_inner(&r.ret).is_some());
    cg.resume_ctx = Some(ResumeCodegenContext {
        env: String::from("%__env"),
        coro: String::from("%__coro"),
        drive_fn: spec.drive_fn.to_string(),
        answer_ty: spec.answer_ty,
        op_ret_is_result,
    });
    let body_raw = gen_expr(cg, &arm.body)?;
    let body = coerce_to(cg, body_raw, spec.answer_ty)?;
    let boxed = box_codegen_value(cg, body);
    crate::arc::epilogue(cg, None);
    cg.emit(format!("ret i64 {}", boxed.operand));
    cg.exit_nested_fn(saved, "i64", spec.name, &params);
    Ok(())
}

fn emit_drive_fn(cg: &mut Codegen, name: &str, arms: &[DriveArm]) {
    let saved = cg.enter_nested_fn();
    let params = vec![
        (LType::Ptr, String::from("__env")),
        (LType::Ptr, String::from("__coro")),
    ];
    let done = cg.call("i64", "__osprey_coro_done", "i8*", &["%__coro"]);
    let done_cond = cg.emit_reg(format!("icmp ne i64 {done}, 0"));
    let done_lbl = cg.fresh_label();
    let dispatch_lbl = cg.fresh_label();
    cg.emit(format!(
        "br i1 {done_cond}, label %{done_lbl}, label %{dispatch_lbl}"
    ));

    cg.start_block(&done_lbl);
    let result = cg.call("i64", "__osprey_coro_result", "i8*", &["%__coro"]);
    cg.emit(format!("ret i64 {result}"));

    cg.start_block(&dispatch_lbl);
    let op = cg.call("i64", "__osprey_coro_op", "i8*", &["%__coro"]);
    let miss_lbl = cg.fresh_label();
    let check_labels: Vec<String> = arms.iter().map(|_| cg.fresh_label()).collect();
    let arm_labels: Vec<String> = arms.iter().map(|_| cg.fresh_label()).collect();
    if let Some(first) = check_labels.first() {
        cg.emit(format!("br label %{first}"));
    } else {
        cg.emit(format!("br label %{miss_lbl}"));
    }

    for (i, ((arm, check_label), arm_label)) in
        arms.iter().zip(&check_labels).zip(&arm_labels).enumerate()
    {
        cg.start_block(check_label);
        let cmp = cg.emit_reg(format!("icmp eq i64 {op}, {}", arm.op_id));
        let next = check_labels.get(i + 1).unwrap_or(&miss_lbl);
        cg.emit(format!("br i1 {cmp}, label %{arm_label}, label %{next}"));
    }

    for (arm, arm_label) in arms.iter().zip(&arm_labels) {
        cg.start_block(arm_label);
        let mut args = vec![String::from("i8* %__env"), String::from("i8* %__coro")];
        for (idx, pty) in arm.sig.params.iter().copied().enumerate() {
            let raw = cg.call(
                "i64",
                "__osprey_coro_arg",
                "i8*, i64",
                &["%__coro", &idx.to_string()],
            );
            let value = unbox_from_i64(cg, &raw, pty);
            args.push(value.typed());
        }
        let arm_result = cg.emit_reg(format!("call i64 @{}({})", arm.arm_fn, args.join(", ")));
        let done_after = cg.call("i64", "__osprey_coro_done", "i8*", &["%__coro"]);
        let done_after_cond = cg.emit_reg(format!("icmp ne i64 {done_after}, 0"));
        let abort_lbl = cg.fresh_label();
        let return_lbl = cg.fresh_label();
        cg.emit(format!(
            "br i1 {done_after_cond}, label %{return_lbl}, label %{abort_lbl}"
        ));
        cg.start_block(&abort_lbl);
        cg.call_void("__osprey_coro_abort", "i8*", &["%__coro"]);
        cg.emit(format!("br label %{return_lbl}"));
        cg.start_block(&return_lbl);
        cg.emit(format!("ret i64 {arm_result}"));
    }

    cg.start_block(&miss_lbl);
    cg.call_void("__osprey_coro_abort", "i8*", &["%__coro"]);
    cg.emit("ret i64 0");
    cg.exit_nested_fn(saved, "i64", name, &params);
}

pub(crate) fn gen_resume(cg: &mut Codegen, value: Option<&Expr>) -> Result<Value> {
    declare_coro(cg);
    let Some(ctx) = cg.resume_ctx.clone() else {
        return Err(crate::error::CodegenError::invalid(
            "`resume` outside a handler arm",
        ));
    };
    let raw_value = match value {
        Some(expr) => gen_expr(cg, expr)?,
        None => Value::unit(),
    };
    // The resume value lands in the operation-result slot: unwrap a Result
    // only when the operation does NOT expect one (the usual value-site
    // rule); a Result-instantiated slot receives the whole block.
    let raw_value = if ctx.op_ret_is_result {
        raw_value
    } else {
        crate::result::unwrap(cg, raw_value)
    };
    let boxed_value = box_codegen_value(cg, raw_value);
    let resumed = cg.call(
        "i64",
        "__osprey_coro_resume",
        "i8*, i64",
        &[&ctx.coro, &boxed_value.operand],
    );
    let done = cg.call("i64", "__osprey_coro_done", "i8*", &[&ctx.coro]);
    let done_cond = cg.emit_reg(format!("icmp ne i64 {done}, 0"));
    let done_lbl = cg.fresh_label();
    let more_lbl = cg.fresh_label();
    let end_lbl = cg.fresh_label();
    cg.emit(format!(
        "br i1 {done_cond}, label %{done_lbl}, label %{more_lbl}"
    ));

    cg.start_block(&done_lbl);
    let done_pred = cg.snapshot_to(&end_lbl);

    cg.start_block(&more_lbl);
    let nested = cg.emit_reg(format!(
        "call i64 @{}(i8* {}, i8* {})",
        ctx.drive_fn, ctx.env, ctx.coro
    ));
    let more_pred = cg.snapshot_to(&end_lbl);

    cg.start_block(&end_lbl);
    let phi = cg.emit_reg(format!(
        "phi i64 [ {resumed}, %{done_pred} ], [ {nested}, %{more_pred} ]"
    ));
    Ok(unbox_from_i64(cg, &phi, ctx.answer_ty))
}

fn box_codegen_value(cg: &mut Codegen, value: Value) -> Value {
    // Every effect-boundary boxing erases pointer-ness from the ARC drop
    // walk: dup so the unboxing side owns +1 [GC-ARC-PERCEUS].
    crate::arc::escape_retain(cg, &value);
    crate::effect_generics::box_raw_value(cg, value)
}

pub(crate) fn unbox_coro_value(
    cg: &mut Codegen,
    raw: &str,
    ty: LType,
    result_inner: Option<LType>,
) -> Value {
    if let Some(inner) = result_inner {
        let ptr = cg.emit_reg(format!("inttoptr i64 {raw} to i8*"));
        let struct_ty = crate::llty::result_struct_ty(inner);
        let typed = cg.emit_reg(format!("bitcast i8* {ptr} to {struct_ty}*"));
        return Value::result(typed, inner);
    }
    unbox_from_i64(cg, raw, ty)
}

/// `perform Effect.op(args)` — look up the active handler and call it. An
/// erased (generic) slot boxes its argument and unboxes its result against
/// the signature inference resolved for this site. Implements
/// [EFFECTS-GENERIC-RUNTIME].
pub(crate) fn gen_perform(
    cg: &mut Codegen,
    effect: &str,
    operation: &str,
    args: &[Expr],
    position: Option<osprey_ast::Position>,
) -> Result<Value> {
    declare_stack(cg);
    let sig_key = format!("{effect}.{operation}");
    let sig = cg
        .effect_op(&sig_key)
        .unwrap_or_else(|| OpSig::default_for_arity(args.len()));
    let site = crate::effect_generics::site_perform_op(cg, position);
    // Look up the handler under the instantiation-mangled key, so a
    // mismatched instantiation misses (a loud unhandled-effect abort) rather
    // than reaching a handler of the wrong type. Implements
    // [EFFECTS-GENERIC-RUNTIME].
    let lookup_key = site.as_ref().map_or_else(
        || effect.to_string(),
        |s| crate::effect_generics::runtime_effect_key(effect, &s.effect_args),
    );

    // Evaluate + coerce arguments to the operation's parameter types.
    let mut typed = Vec::new();
    for (i, a) in args.iter().enumerate() {
        let v = gen_expr(cg, a)?;
        let v = if sig.param_erased.get(i).copied().unwrap_or(false) {
            crate::effect_generics::box_erased(
                cg,
                v,
                site.as_ref().and_then(|s| s.op.params.get(i)),
            )
        } else {
            let want = sig.params.get(i).copied().unwrap_or(LType::I64);
            coerce_to(cg, v, want)?
        };
        typed.push(v.typed());
    }

    let eff_s = cg.string_constant(&lookup_key);
    let op_s = cg.string_constant(operation);
    let raw = cg.fresh_reg();
    cg.emit(format!(
        "{raw} = call i8* @__osprey_handler_lookup(i8* {}, i8* {})",
        eff_s.operand, op_s.operand
    ));
    // A missed lookup returns null — abort with a message instead of calling
    // a null pointer (an instantiation mismatch on a generic effect misses by
    // design, [EFFECTS-GENERIC-RUNTIME]).
    emit_unhandled_guard(cg, &raw, &lookup_key, operation);
    let env = cg.fresh_reg();
    cg.emit(format!(
        "{env} = call i8* @__osprey_handler_lookup_env(i8* {}, i8* {})",
        eff_s.operand, op_s.operand
    ));
    let fp = cg.fresh_reg();
    cg.emit(format!("{fp} = bitcast i8* {raw} to {}", sig.fn_ptr_ty()));
    let ret_ty = sig.ret_ty();
    let r = cg.fresh_reg();
    let mut call_args = vec![format!("i8* {env}")];
    call_args.extend(typed);
    cg.emit(format!(
        "{r} = call {ret_ty} {fp}({})",
        call_args.join(", ")
    ));
    if sig.ret_erased {
        return Ok(match site {
            Some(s) => {
                let v = crate::effect_generics::unbox_erased(cg, &r, &s.op.ret);
                // The arm escape-retained before boxing: own the +1 here.
                crate::arc::own(cg, &v);
                v
            }
            None => Value::new(r, LType::I64),
        });
    }
    let out = match sig.ret_result_inner {
        Some(inner) => Value::result(r, inner),
        None => Value::new(r, sig.ret),
    };
    // The handler fn's epilogue transferred +1 [GC-ARC-PERCEUS].
    crate::arc::own(cg, &out);
    Ok(out)
}
