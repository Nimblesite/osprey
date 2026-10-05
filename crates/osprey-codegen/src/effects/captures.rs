//! Handler environments and shared cell capture.
use super::{
    free_idents, AstNode, BTreeSet, CellSlot, Codegen, Expr, HandlerArm, HashSet, Stmt, Value,
};

/// The free identifiers an arm's body closes over, minus the arm's own
/// parameters — the names that must be captured from the enclosing scope.
pub(super) fn arm_free_idents(arm: &HandlerArm) -> impl Iterator<Item = String> + '_ {
    let mut free = BTreeSet::new();
    free_idents(&arm.body, &mut free);
    free.into_iter().filter(|n| !arm.params.contains(n))
}

/// One binding shared by every arm of a single `handle` region, captured into
/// the region's environment.
pub(super) enum ArmCap {
    /// A handler-captured mutable: the env carries the heap cell's `i8*` pointer
    /// so arms `load`/`store` the same slot — handler-owned state. `ptr` is the
    /// cell pointer in the enclosing scope (a `{pointee}*` operand).
    Cell { name: String, cell: CellSlot },
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

// Capture rules stay local; structural child enumeration is shared with the
// other AST passes so new expression variants have one traversal table.
pub(super) fn scan_expr(e: &Expr, muts: &mut BTreeSet<String>, captured: &mut BTreeSet<String>) {
    match e {
        Expr::Handler { arms, .. } => {
            for arm in arms {
                captured.extend(arm_free_idents(arm));
            }
        }
        Expr::Lambda {
            parameters, body, ..
        } => {
            captured.extend(crate::closure::free_names(parameters, body));
        }
        _ => {}
    }
    // Preserve this collector's existing exclusion of specialized operands.
    if matches!(e, Expr::TypeApply { .. }) {
        return;
    }
    AstNode::Expression(e).for_each_child(|child| match child {
        AstNode::Statement(statement) => scan_stmt(statement, muts, captured),
        AstNode::Expression(expression) => scan_expr(expression, muts, captured),
    });
}

pub(super) fn scan_stmt(s: &Stmt, muts: &mut BTreeSet<String>, captured: &mut BTreeSet<String>) {
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

/// The bindings every arm of this region captures, in stable (sorted) order: a
/// handler-captured mutable becomes a shared [`ArmCap::Cell`]; any other bound
/// free variable is captured by value. Names that resolve to nothing in scope
/// (top-level functions, constructors) need no capture — the arm resolves them
/// directly.
pub(super) fn arms_free_idents(arms: &[HandlerArm]) -> BTreeSet<String> {
    arms.iter().flat_map(arm_free_idents).collect()
}

pub(super) fn capture_list(cg: &Codegen, arms: &[HandlerArm]) -> Vec<ArmCap> {
    caps_from_names(cg, arms_free_idents(arms))
}

pub(super) fn capture_list_resuming(
    cg: &Codegen,
    arms: &[HandlerArm],
    body: &Expr,
    return_clause: Option<&Expr>,
) -> Vec<ArmCap> {
    let mut names = arms_free_idents(arms);
    free_idents(body, &mut names);
    if let Some(clause) = return_clause {
        free_idents(clause, &mut names);
    }
    caps_from_names(cg, names)
}

pub(super) fn caps_from_names(cg: &Codegen, names: BTreeSet<String>) -> Vec<ArmCap> {
    names
        .into_iter()
        .filter_map(|name| {
            if let Some(slot) = cg.cell_slots.get(&name) {
                Some(ArmCap::Cell {
                    name,
                    cell: slot.clone(),
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
pub(super) fn build_env(cg: &mut Codegen, caps: &[ArmCap]) -> (String, String) {
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
pub(super) fn store_operand(cg: &mut Codegen, c: &ArmCap) -> String {
    match c {
        ArmCap::Cell { cell, .. } => {
            let ty = cell.pointee.as_str();
            let ptr = &cell.ptr;
            cg.emit_reg(format!("bitcast {ty}* {ptr} to i8*"))
        }
        ArmCap::Val { val, .. } => val.operand.clone(),
    }
}

/// Inside an arm function: cast `%__env` back to the region's struct and rebuild
/// each capture — a [`ArmCap::Cell`] as a live cell slot (so reads `load` and
/// reassignments `store` the shared heap location), a value by binding its
/// reloaded register.
pub(super) fn reload_env(cg: &mut Codegen, caps: &[ArmCap], env_ty: &str) {
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
        bind_capture(cg, c, loaded);
    }
}

fn bind_capture(cg: &mut Codegen, c: &ArmCap, loaded: String) {
    match c {
        ArmCap::Cell { name, cell } => {
            let ptr = cg.emit_reg(format!("bitcast i8* {loaded} to {}*", cell.pointee));
            cg.bind_cell(
                name,
                CellSlot {
                    ptr,
                    ..cell.clone()
                },
                false,
            );
        }
        ArmCap::Val { name, val } => {
            let mut v = val.clone();
            v.operand = loaded;
            cg.emit_debug_local(name, &v);
            cg.bind(name.clone(), v);
        }
    }
}
