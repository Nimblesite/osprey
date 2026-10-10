//! The environment of a lambda that is applied inline instead of being called
//! through a closure cell. Implements [TYPE-FN-CLOSURE] and [TYPE-GENERICS-FN].
//!
//! A still-generic lambda has no single ABI, so it materialises no cell: each
//! call site beta-reduces its body. Lexical scoping does not change with the
//! lowering, so what the body closes over is fixed where the lambda is
//! DEFINED. Every captured local is aliased there under a name no source can
//! spell — an owner of its own under ARC — and the body is lowered behind a
//! lexical barrier holding those bindings and nothing of its caller's.
//!
//! Reading the caller's scope by name instead produced four defects: a
//! shadowing binding answered for the captured one, ARC released a captured
//! value at its source's last use and the body read freed memory, a nested
//! function failed with `unknown name`, and a handler value invoked from a
//! nested function silently dropped every write to its own state.

use crate::builder::{CellSlot, Codegen, LambdaDef};
use crate::error::{CodegenError, Result};
use crate::llty::Value;
use osprey_ast::Expr;
use osprey_types::Type;
use std::collections::{BTreeSet, HashSet};
use std::rc::Rc;

/// The separator of an alias. `$` cannot occur in a source identifier in
/// either flavor, so no binding the author writes can shadow an alias.
const ALIAS_MARK: &str = "$env";

/// One free name of an inline lambda, resolved where the lambda was defined.
#[derive(Clone)]
pub(crate) enum Captured {
    /// A value or handler-owned cell, reachable through `alias`.
    Slot { name: String, alias: String },
    /// Another inline lambda, with the environment it had at that point.
    Lambda {
        name: String,
        def: LambdaDef,
        env: Environment,
    },
    /// A name standing for a generic function.
    Alias { name: String, target: String },
}

/// Everything an inline lambda closes over, in stable (sorted) order.
pub(crate) type Environment = Rc<[Captured]>;

/// Where a captured slot lives in the scope applying the lambda.
enum Storage {
    Cell(CellSlot),
    Value(Value),
}

/// The bindings of an environment, read from the scope applying the lambda.
#[derive(Default)]
struct Frame {
    slots: Vec<(String, Storage, Option<Type>)>,
    lambdas: Vec<(String, LambdaDef, Environment)>,
    aliases: Vec<(String, String)>,
}

/// Whether `name` is an environment alias rather than a source binding.
pub(crate) fn is_alias(name: &str) -> bool {
    name.contains(ALIAS_MARK)
}

/// Fix the environment of a lambda to the bindings `free` resolves to here.
pub(crate) fn capture(cg: &mut Codegen, free: BTreeSet<String>) -> Environment {
    free.into_iter()
        .filter_map(|name| captured(cg, name))
        .collect()
}

/// An environment of values a generic function's returned lambda closes over:
/// its producing call's arguments, evaluated once at the binding.
pub(crate) fn of_values(cg: &mut Codegen, bound: Vec<(String, Value)>) -> Environment {
    bound
        .into_iter()
        .map(|(name, value)| {
            let alias = alias_value(cg, &name, value);
            Captured::Slot { name, alias }
        })
        .collect()
}

fn captured(cg: &mut Codegen, name: String) -> Option<Captured> {
    if let Some(alias) = alias_slot(cg, &name) {
        return Some(Captured::Slot { name, alias });
    }
    if let Some(target) = cg.call_aliases.get(&name).cloned() {
        return Some(Captured::Alias { name, target });
    }
    let def = cg.lambdas.get(&name).cloned()?;
    let env = cg.lambda_envs.get(&name).cloned().unwrap_or_default();
    Some(Captured::Lambda { name, def, env })
}

/// Alias the cell or value `name` is bound to here, or `None` when it has no
/// local storage (a file-scope binding, a function, a constructor).
fn alias_slot(cg: &mut Codegen, name: &str) -> Option<String> {
    let function = cg.fn_value_types.get(name).cloned();
    if let Some(cell) = cg.cell_slots.get(name).cloned() {
        let alias = fresh_alias(cg, name, function);
        let _ = cg.cell_slots.insert(alias.clone(), cell);
        return Some(alias);
    }
    let mut value = cg.lookup(name)?;
    value.inferred_type = value.inferred_type.or(function);
    Some(alias_value(cg, name, value))
}

/// Bind `value` under a fresh alias that owns it, so the capture survives the
/// last use of the binding it was read from [GC-ARC-PERCEUS].
fn alias_value(cg: &mut Codegen, name: &str, value: Value) -> String {
    let alias = fresh_alias(cg, name, function_type(&value));
    crate::arc::bind_owned(cg, &alias, &value);
    cg.bind(alias.clone(), value);
    alias
}

fn fresh_alias(cg: &mut Codegen, name: &str, function: Option<Type>) -> String {
    let alias = format!("{name}{ALIAS_MARK}{}", cg.next_env_id());
    if let Some(ty) = function {
        cg.bind_fn_local(&alias, ty);
    }
    alias
}

fn function_type(value: &Value) -> Option<Type> {
    value
        .inferred_type
        .clone()
        .filter(|ty| matches!(ty, Type::Fun { .. }))
}

/// Every alias `env` reaches, through the lambdas it captured as well.
pub(crate) fn aliases(env: &Environment) -> Vec<String> {
    env.iter()
        .flat_map(|captured| match captured {
            Captured::Slot { alias, .. } => vec![alias.clone()],
            Captured::Lambda { env, .. } => aliases(env),
            Captured::Alias { .. } => Vec::new(),
        })
        .collect()
}

/// The aliases `names` reach through the inline lambdas among them: what a
/// nested function must capture to apply those lambdas, and what stays alive
/// while they can still be applied.
pub(crate) fn reach(cg: &Codegen, names: &BTreeSet<String>) -> BTreeSet<String> {
    names
        .iter()
        .filter_map(|name| cg.lambda_envs.get(name))
        .flat_map(aliases)
        .collect()
}

/// Lower `emit` where the inline lambda `name` was defined: behind a lexical
/// barrier holding its environment and nothing of the caller's. A lambda with
/// no recorded environment keeps the scope it is applied in.
pub(crate) fn within<T>(
    cg: &mut Codegen,
    name: &str,
    emit: impl FnOnce(&mut Codegen) -> Result<T>,
) -> Result<T> {
    let Some(env) = cg.lambda_envs.get(name).cloned() else {
        return emit(cg);
    };
    let cells = cg
        .lambda_def(name)
        .map(|(_, body, _)| crate::effects::captured_mut_vars(body))
        .unwrap_or_default();
    behind_barrier(cg, &env, cells, emit)
}

/// [`within`] for a lambda that carries its environment with it.
pub(crate) fn within_env<T>(
    cg: &mut Codegen,
    env: &Environment,
    body: &Expr,
    emit: impl FnOnce(&mut Codegen) -> Result<T>,
) -> Result<T> {
    behind_barrier(cg, env, crate::effects::captured_mut_vars(body), emit)
}

/// Cell promotion belongs to the body being lowered, as it does for a closure.
fn behind_barrier<T>(
    cg: &mut Codegen,
    env: &Environment,
    cells: HashSet<String>,
    emit: impl FnOnce(&mut Codegen) -> Result<T>,
) -> Result<T> {
    let frame = resolve(cg, env)?;
    cg.with_file_scope(|cg| {
        cg.cell_vars = cells;
        install(cg, frame);
        emit(cg)
    })
}

fn resolve(cg: &Codegen, env: &Environment) -> Result<Frame> {
    let mut frame = Frame::default();
    for captured in env.iter() {
        match captured {
            Captured::Slot { name, alias } => frame.slots.push(slot(cg, name, alias)?),
            Captured::Alias { name, target } => frame.aliases.push((name.clone(), target.clone())),
            Captured::Lambda { name, def, env } => {
                for alias in aliases(env) {
                    frame.slots.push(slot(cg, &alias, &alias)?);
                }
                frame
                    .lambdas
                    .push((name.clone(), def.clone(), Rc::clone(env)));
            }
        }
    }
    Ok(frame)
}

/// The storage behind `alias`, to be bound as `name`. An alias a nested
/// function could not capture — a handler-owned cell inside a fiber — is
/// reported under the name the author wrote.
fn slot(cg: &Codegen, name: &str, alias: &str) -> Result<(String, Storage, Option<Type>)> {
    let storage = match (cg.cell_slots.get(alias), cg.lookup(alias)) {
        (Some(cell), _) => Storage::Cell(cell.clone()),
        (None, Some(value)) => Storage::Value(value),
        (None, None) => return Err(CodegenError::unknown(source_name(alias))),
    };
    let function = cg.fn_value_types.get(alias).cloned().or(match &storage {
        Storage::Cell(cell) => cell.inferred_type.clone(),
        Storage::Value(value) => function_type(value),
    });
    Ok((name.to_string(), storage, function))
}

fn source_name(alias: &str) -> &str {
    alias.split(ALIAS_MARK).next().unwrap_or(alias)
}

fn install(cg: &mut Codegen, frame: Frame) {
    for (name, storage, function) in frame.slots {
        if let Some(ty) = function.filter(|ty| matches!(ty, Type::Fun { .. })) {
            cg.bind_fn_local(&name, ty);
        }
        match storage {
            Storage::Cell(cell) => {
                let _ = cg.cell_slots.insert(name, cell);
            }
            Storage::Value(value) => cg.bind(name, value),
        }
    }
    for (name, def, env) in frame.lambdas {
        let _ = cg.lambdas.insert(name.clone(), def);
        let _ = cg.lambda_envs.insert(name, env);
    }
    cg.call_aliases.extend(frame.aliases);
}
