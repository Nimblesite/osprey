//! The fewest annotations that give every generated definition its Lean type
//! ([MATH-CODE-EMIT-ELEGANT]).
//!
//! Osprey's checker decides what an unannotated parameter becomes, and it is
//! the only authority on that: arithmetic between two open values defaults to
//! `int`, and `floatDiv` is open over `int` too. So nothing here predicts the
//! checker. The module is emitted, the checker infers it, and the first
//! parameter of each definition whose inferred type does not keep its Lean
//! type ([`holds`]) is annotated; that repeats until every parameter matches.
//! A result must be its Lean type outright: while it is open the next parameter
//! is annotated, and a definition with every parameter annotated and an open
//! result keeps a `name : type` header.

use crate::model::{Def, Module, Ty};
use std::collections::BTreeSet;

/// What one definition writes beyond its bare parameter names.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Written {
    /// Parameters annotated inline, by position.
    pub(crate) annotated: BTreeSet<usize>,
    /// Whether the definition keeps its whole signature as a header.
    pub(crate) header: bool,
}

/// The annotations of every definition of `module`, in order.
pub(crate) fn settled(file: &str, module: &Module) -> Vec<Written> {
    let mut written = vec![Written::default(); module.defs.len()];
    // Each round annotates at least one more parameter or adds a header, so
    // the rounds are bounded by the parameters and definitions there are.
    let bound = module
        .defs
        .iter()
        .map(|def| def.params.len().saturating_add(1))
        .sum::<usize>();
    for _ in 0..=bound {
        let text = crate::emit::emit(file, module, &written);
        let Some(inferred) = inferred(&text, module) else {
            return all_headers(module);
        };
        if !refine(module, &inferred, &mut written) {
            return written;
        }
    }
    all_headers(module)
}

/// Every definition with its whole signature: what the module wrote before
/// it could be tighter, and what it falls back to when inference cannot run.
fn all_headers(module: &Module) -> Vec<Written> {
    module
        .defs
        .iter()
        .map(|_| Written {
            annotated: BTreeSet::new(),
            header: true,
        })
        .collect()
}

/// Each definition's inferred parameter and result types, as Osprey spells them.
type Inferred = Vec<Option<(Vec<String>, String)>>;

fn inferred(text: &str, module: &Module) -> Option<Inferred> {
    let parsed = osprey_syntax::parse_program_with_flavor(text, osprey_syntax::Flavor::Ml);
    if !parsed.errors.is_empty() {
        return None;
    }
    // The published types say `number` where a value stays generic over numbers;
    // a module the full check refuses is read from inference alone.
    let types = osprey_types::check_and_publish(&parsed.program, &[]).map_or_else(
        |_| osprey_types::infer_program(&parsed.program),
        |checked| checked.types().clone(),
    );
    Some(
        module
            .defs
            .iter()
            .map(|def| {
                let key = keys(module, &def.name)
                    .into_iter()
                    .find(|key| types.param_types(key).is_some())?;
                let params = types.param_types(&key)?;
                let result = types.return_type(&key)?;
                Some((
                    params.iter().map(ToString::to_string).collect(),
                    result.to_string(),
                ))
            })
            .collect(),
    )
}

/// Every name the checker may file a module's definition under: assembled
/// projects mangle module members, a lone source keeps their written path.
fn keys(module: &Module, name: &str) -> Vec<String> {
    let mut path: Vec<&str> = module.namespace.iter().map(String::as_str).collect();
    path.extend([module.name.as_str(), name]);
    vec![
        osprey_ast::symbol::mangle(path.iter().copied()),
        osprey_ast::symbol::mangle([module.name.as_str(), name]),
        path.join("::"),
        format!("{}::{name}", module.name),
        name.to_owned(),
    ]
}

/// Add the next annotation each definition needs; whether anything was added.
fn refine(module: &Module, inferred: &Inferred, written: &mut [Written]) -> bool {
    let mut changed = false;
    for ((def, found), write) in module.defs.iter().zip(inferred).zip(written.iter_mut()) {
        if write.header {
            continue;
        }
        let Some((params, result)) = found else {
            // A constant is not a function the checker files; its value is
            // written as a float literal or from floats, so it is already its type.
            if def.params.is_empty() {
                continue;
            }
            write.header = true;
            changed = true;
            continue;
        };
        let next = first_open(def, params, write).or_else(|| {
            // An open result needs a fixed parameter; the header is the last resort.
            (!concrete(def.result, result))
                .then(|| unannotated(def, write))
                .flatten()
        });
        match next {
            Some(index) => {
                let _ = write.annotated.insert(index);
                changed = true;
            }
            None if !concrete(def.result, result) => {
                write.header = true;
                changed = true;
            }
            None => {}
        }
    }
    changed
}

/// The first parameter whose inferred type is not its Lean type and is not yet annotated.
fn first_open(def: &Def, params: &[String], write: &Written) -> Option<usize> {
    def.params
        .iter()
        .zip(params)
        .enumerate()
        .find(|(index, ((_, ty), found))| !write.annotated.contains(index) && !holds(*ty, found))
        .map(|(index, _)| index)
}

/// The first parameter not yet annotated.
fn unannotated(def: &Def, write: &Written) -> Option<usize> {
    (0..def.params.len()).find(|index| !write.annotated.contains(index))
}

/// Whether an inferred result is the Lean type: a result open over every type
/// would let a definition that Lean types as `Bool` return anything.
fn concrete(lean: Ty, inferred: &str) -> bool {
    inferred == lean.osprey() || (lean == Ty::Float && inferred == "number")
}

/// Whether an inferred parameter type keeps the Lean type's meaning: the Lean type itself, or
/// `number` for a Float, which keeps every float a caller passes. A parameter left open (a type
/// variable) is not proven: the checker defaults such a value to `int` at a call that passes an
/// open value, so it is annotated.
fn holds(lean: Ty, inferred: &str) -> bool {
    concrete(lean, inferred)
}
