//! File-scope bindings as LLVM module globals. Implements
//! [MODULES-FILE-SCOPE-BINDING].
//!
//! A top-level `let`/`mut` lowers to an SSA register inside `main`, which no
//! other function can name. A binding a function body *does* read therefore
//! needs storage with module lifetime: one `internal global` per binding,
//! stored at the initializer and loaded at every read from a function.
//!
//! Only the bindings functions actually read get one — the same shape as
//! [`crate::effects::captured_mut_vars`], which promotes only the `mut`s a
//! handler arm captures. Every other program emits byte-identical IR.
//!
//! A `mut` an effect handler owns is already a heap cell ([EFFECTS-HANDLER-STATE]);
//! its global holds the cell's ADDRESS, so a function reads the same live
//! location the arms write, not a stale copy of the value.

use crate::builder::{CellSlot, Codegen, ParamSig};
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use osprey_ast::{Program, Stmt};
use osprey_types::Type;
use std::collections::{BTreeSet, HashSet};

/// LLVM spelling of the pointer a cell-backed global stores.
const CELL_STORAGE: LType = LType::Ptr;

/// Module storage for one file-scope binding.
#[derive(Clone)]
pub(crate) struct GlobalSlot {
    /// The `@`-qualified global symbol.
    symbol: String,
    /// The binding's ABI shape, so a load reconstructs the same [`Value`] an
    /// incoming parameter of that type would carry.
    sig: ParamSig,
    /// Aggregate owner tag for the loaded value.
    owner: Option<String>,
    /// The global holds a heap cell's address rather than the value itself.
    cell: bool,
    /// The binding's inferred type, so a call through a function-valued global
    /// recovers the closure ABI its cell was built with.
    ty: Type,
}

impl GlobalSlot {
    fn storage(&self) -> LType {
        if self.cell {
            CELL_STORAGE
        } else {
            self.sig.ty
        }
    }
}

/// Declare a global for every file-scope binding some function body reads.
/// Runs before any function is emitted, so a forward call that inlines a
/// generic body still finds the slot it reads.
pub(crate) fn seed(
    cg: &mut Codegen,
    program: &Program,
    cells: &HashSet<String>,
    read: &BTreeSet<String>,
) -> Result<()> {
    for statement in &program.statements {
        let Stmt::Let {
            name,
            value,
            position,
            mutable,
            ..
        } = statement
        else {
            continue;
        };
        if !read.contains(name) || cg.module_globals.contains_key(name) {
            continue;
        }
        // A binding that materialises no value has nothing to publish here,
        // and its readers resolve it by NAME instead ([`crate::stmt`]).
        if crate::stmt::binds_no_value(cg, value) {
            continue;
        }
        let slot = describe(cg, name, *position, *mutable && cells.contains(name))?;
        cg.add_global(format!(
            "@{} = internal global {} zeroinitializer",
            slot.symbol,
            slot.storage().as_str()
        ));
        let _ = cg.module_globals.insert(name.clone(), slot);
    }
    Ok(())
}

/// Build the slot from inference. A binding the backend cannot type has no
/// storage it could be given, and guessing one would read a wrong-width value
/// back out — so it fails loudly instead.
fn describe(
    cg: &Codegen,
    name: &str,
    position: Option<osprey_ast::Position>,
    cell: bool,
) -> Result<GlobalSlot> {
    let ty = cg.prog.let_type(position).ok_or_else(|| {
        CodegenError::unsupported(format!(
            "file-scope binding `{name}` is read by a function but inference recorded no type for it"
        ))
    })?;
    Ok(GlobalSlot {
        symbol: format!("osp.g.{name}"),
        sig: ParamSig::of(ty),
        owner: crate::types::owner_name(&cg.prog, ty),
        cell,
        ty: ty.clone(),
    })
}

/// The file-scope `mut`s that must live in a heap cell: the ones a handler arm
/// captures, plus every one a function body reads.
///
/// The second half is what keeps a mutable binding at ONE location. A `mut`
/// left as an SSA value in `main` while functions read a global would be two
/// places at once — a handler arm inside a function would write the global and
/// `main` would go on reading its stale register. Implements
/// [EFFECTS-HANDLER-STATE] [MODULES-FILE-SCOPE-BINDING].
pub(crate) fn cell_names(top_level: &[&Stmt], read: &BTreeSet<String>) -> HashSet<String> {
    let mut cells = crate::effects::captured_mut_vars_in_stmts(top_level);
    cells.extend(top_level.iter().filter_map(|statement| match statement {
        Stmt::Let {
            name,
            mutable: true,
            ..
        } if read.contains(name) => Some(name.clone()),
        _ => None,
    }));
    cells
}

/// Names read from inside some top-level function body. A function's own
/// parameters shadow the file scope, so they are subtracted first.
pub(crate) fn read_by_functions(program: &Program) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for statement in &program.statements {
        let Stmt::Function {
            parameters, body, ..
        } = statement
        else {
            continue;
        };
        let mut free = BTreeSet::new();
        osprey_ast::freevars::free_idents(body, &mut free);
        for parameter in parameters {
            let _ = free.remove(&parameter.name);
        }
        out.extend(free);
    }
    out
}

/// The function type of a function-valued global, so a call through the name
/// dispatches on the closure ABI rather than emitting a direct call to a symbol
/// no `define` ever produced.
pub(crate) fn fn_type(cg: &Codegen, name: &str) -> Option<Type> {
    match cg.module_globals.get(name).map(|slot| &slot.ty) {
        Some(ty @ Type::Fun { .. }) => Some(ty.clone()),
        _ => None,
    }
}

/// Load `name`'s current value, or `None` when it has no module storage. A
/// cell-backed global is two loads: the address, then the live value at it.
pub(crate) fn read(cg: &mut Codegen, name: &str) -> Option<Value> {
    let slot = cg.module_globals.get(name).cloned()?;
    let storage = slot.storage().as_str();
    let raw = cg.emit_reg(format!(
        "load {storage}, {storage}* @{symbol}",
        symbol = slot.symbol
    ));
    let raw = if slot.cell {
        let pointee = slot.sig.ty.as_str();
        let typed = cg.emit_reg(format!("bitcast i8* {raw} to {pointee}*"));
        cg.emit_reg(format!("load {pointee}, {pointee}* {typed}"))
    } else {
        raw
    };
    Some(crate::cast::incoming_param(cg, raw, slot.sig, slot.owner))
}

/// Publish the value a file-scope `let` just bound. The global is an owner, so
/// it takes its own reference and the initializing statement stays free to drop
/// its own [GC-ARC-PERCEUS].
pub(crate) fn publish(cg: &mut Codegen, name: &str, value: Value) -> Result<()> {
    let Some(slot) = cg.module_globals.get(name).cloned() else {
        return Ok(());
    };
    if slot.cell {
        return Err(CodegenError::invalid(format!(
            "file-scope `{name}` was promoted to a handler cell, so its global holds an \
             address; storing the value here would leave every reader dereferencing it"
        )));
    }
    let stored = crate::cast::coerce_param(cg, value, slot.sig)?;
    store(cg, &slot, &stored.operand);
    Ok(())
}

/// Publish the ADDRESS of a handler-owned cell, so every later read of the
/// global sees the arms' writes rather than the value the cell started with.
/// The cell's own pointee must be the type [`read`] will load back through it;
/// a disagreement would read the slot at the wrong width, so it fails loudly.
pub(crate) fn publish_cell(cg: &mut Codegen, name: &str, cell: &CellSlot) -> Result<()> {
    let Some(slot) = cg.module_globals.get(name).cloned() else {
        return Ok(());
    };
    if !slot.cell {
        return Err(CodegenError::invalid(format!(
            "file-scope `{name}` has a value global, so publishing a cell address would \
             make every reader load the pointer as the value"
        )));
    }
    if cell.pointee != slot.sig.ty {
        return Err(CodegenError::invalid(format!(
            "file-scope cell `{name}` holds {} but inference typed it {}",
            cell.pointee.as_str(),
            slot.sig.ty.as_str()
        )));
    }
    let pointee = cell.pointee.as_str();
    let address = if pointee == CELL_STORAGE.as_str() {
        cell.ptr.clone()
    } else {
        cg.emit_reg(format!("bitcast {pointee}* {} to i8*", cell.ptr))
    };
    store(cg, &slot, &address);
    Ok(())
}

/// Initialize the global. Managed storage always takes a reference, which is
/// what makes the invariant provable: the global holds exactly one reference
/// whenever it holds a managed value, so [`assign`] releases the value it
/// replaces and [`release_all`] releases the survivor once [GC-ARC-PERCEUS].
fn store(cg: &mut Codegen, slot: &GlobalSlot, operand: &str) {
    let storage = slot.storage().as_str();
    if slot.storage().is_managed_ptr() {
        crate::arc::retain_operand(cg, operand);
    }
    cg.emit(format!(
        "store {storage} {operand}, {storage}* @{symbol}",
        symbol = slot.symbol
    ));
}

/// Reassign a file-scope binding from a handler arm, which runs as a lifted
/// function and so cannot see `main`'s frame. The global — or the cell it
/// addresses — IS the shared location, so the write lands exactly where every
/// reader looks [EFFECTS-HANDLER-STATE] [MODULES-FILE-SCOPE-BINDING].
///
/// Rebind order matches [`crate::stmt::gen_cell_store`]: retain the incoming
/// value BEFORE releasing the old one, so `x = x` never frees what it stores.
pub(crate) fn assign(cg: &mut Codegen, name: &str, value: Value) -> Result<()> {
    let Some(slot) = cg.module_globals.get(name).cloned() else {
        return Err(CodegenError::unknown(name));
    };
    let (target, held) = destination(cg, &slot);
    let ty = held.as_str();
    let incoming = if slot.cell {
        crate::cast::coerce_to(cg, value, held)?
    } else {
        crate::cast::coerce_param(cg, value, slot.sig)?
    };
    if held.is_managed_ptr() {
        crate::arc::retain_operand(cg, &incoming.operand);
        let old = cg.emit_reg(format!("load {ty}, {ty}* {target}"));
        crate::arc::release_operand(cg, &old);
    }
    cg.emit(format!("store {ty} {}, {ty}* {target}", incoming.operand));
    Ok(())
}

/// The pointer a write goes through and the type it stores: the global itself
/// for a value binding, the cell it currently addresses for a promoted `mut`.
fn destination(cg: &mut Codegen, slot: &GlobalSlot) -> (String, LType) {
    if !slot.cell {
        return (format!("@{}", slot.symbol), slot.storage());
    }
    let storage = CELL_STORAGE.as_str();
    let address = cg.emit_reg(format!(
        "load {storage}, {storage}* @{symbol}",
        symbol = slot.symbol
    ));
    let pointee = slot.sig.ty.as_str();
    let typed = cg.emit_reg(format!("bitcast i8* {address} to {pointee}*"));
    (typed, slot.sig.ty)
}

/// Drop every global's reference at the end of `main`, so the leak sentinel
/// sees a balanced program [MEM-BACKENDS]. A cell global releases the cell it
/// points at; the value the cell holds is released by the cell's own drop mask.
pub(crate) fn release_all(cg: &mut Codegen) {
    for slot in cg.module_globals.values().cloned().collect::<Vec<_>>() {
        if !slot.storage().is_managed_ptr() {
            continue;
        }
        let storage = slot.storage().as_str();
        let live = cg.emit_reg(format!(
            "load {storage}, {storage}* @{symbol}",
            symbol = slot.symbol
        ));
        crate::arc::release_operand(cg, &live);
        cg.emit(format!(
            "store {storage} null, {storage}* @{symbol}",
            symbol = slot.symbol
        ));
    }
}
