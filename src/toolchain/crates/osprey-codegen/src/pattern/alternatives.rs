//! Or-patterns. Each alternative runs its own tests, left to right; a failed
//! alternative falls to the next one, and the last falls to the arm's
//! failure. Every alternative that matches jumps to one join block, where
//! each binder is a phi of the alternatives' values, so the arm body is
//! emitted once. Implements [PATTERN-OR-ORDER] and [PATTERN-OR-BODY].

use super::nested::{capture, Capture};
use super::{list, structural};
use crate::{builder::Codegen, error::Result, llty::Value};
use osprey_ast::{Expr, MatchArm, Pattern};

/// A match with an or-pattern arm: every arm tests and captures its whole
/// pattern in order and falls to the next on any failed test, so an
/// or-pattern sits beside any other arm shape. The checker has proved the
/// match exhaustive, so falling past the last arm is unreachable.
pub(super) fn gen_alternatives_match(
    cg: &mut Codegen,
    disc: &Value,
    arms: &[MatchArm],
) -> Result<Value> {
    let end = cg.fresh_label();
    let mark = super::match_mark(cg);
    let mut phi_in: Vec<(Value, String)> = Vec::new();
    for arm in arms {
        super::nested::refutable_arm(cg, arm, disc, &mut phi_in)?;
    }
    cg.emit("unreachable");
    super::finish_phi(cg, &phi_in, &end, mark)
}

/// One binder's value on one alternative's path.
struct Joined {
    name: String,
    value: Value,
    owned: bool,
}

/// Capture `alternatives` against `value`, jumping to `failed` when none
/// matches; the captures returned are the joined binders.
pub(super) fn capture_alternatives(
    cg: &mut Codegen,
    value: &Value,
    alternatives: &[Pattern],
    failed: &str,
) -> Result<Vec<Capture>> {
    let join = cg.fresh_label();
    let owned = tail_binders(alternatives);
    let mut incoming: Vec<(Vec<Joined>, String)> = Vec::new();
    for (index, alternative) in alternatives.iter().enumerate() {
        let last = index + 1 == alternatives.len();
        let next = if last {
            failed.to_owned()
        } else {
            cg.fresh_label()
        };
        let captured = capture(cg, value.clone(), alternative, &next)?;
        let shape = incoming.first().map(|(first, _)| first.as_slice());
        let values = materialize(cg, captured, shape, &owned)?;
        let from = cg.snapshot_to(&join);
        incoming.push((values, from));
        if !last {
            cg.start_block(&next);
        }
    }
    cg.start_block(&join);
    Ok(joined(cg, &incoming))
}

/// Each capture as a plain value on this alternative's path, coerced to the
/// first alternative's representation and in name order, so every path
/// feeds the same phis. A binder another alternative takes as a list tail
/// is +1 there, so it is retained here to match.
fn materialize(
    cg: &mut Codegen,
    captured: Vec<Capture>,
    shape: Option<&[Joined]>,
    owned: &[String],
) -> Result<Vec<Joined>> {
    let mut values = Vec::new();
    for capture in captured {
        let (name, value, tail) = match capture {
            Capture::Value(name, value) => (name, value, false),
            Capture::Row(name, value, pattern) => {
                let arm = MatchArm {
                    pattern,
                    body: Expr::Identifier(name.clone()),
                };
                let bound = structural::gen_structural_match(cg, &value, &[arm])?;
                (name, bound, false)
            }
            Capture::Tail(name, value, count) => (name, list::tail(cg, &value, count), true),
        };
        let value = match shape.and_then(|first| first.iter().find(|bound| bound.name == name)) {
            Some(first) => crate::cast::coerce_to(cg, value, first.value.ty)?,
            None => value,
        };
        let shared = owned.contains(&name);
        if shared && !tail {
            crate::arc::escape_retain(cg, &value);
        }
        values.push(Joined {
            name,
            value,
            owned: shared,
        });
    }
    values.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(values)
}

/// One phi per binder at the join, owned once when its paths hand it +1.
fn joined(cg: &mut Codegen, incoming: &[(Vec<Joined>, String)]) -> Vec<Capture> {
    let Some((first, _)) = incoming.first() else {
        return Vec::new();
    };
    let mut captures = Vec::new();
    for (column, shape) in first.iter().enumerate() {
        let edges: Vec<String> = incoming
            .iter()
            .filter_map(|(values, from)| {
                values
                    .get(column)
                    .map(|bound| format!("[ {}, %{from} ]", bound.value.operand))
            })
            .collect();
        let register = cg.emit_reg(format!(
            "phi {} {}",
            shape.value.ty.as_str(),
            edges.join(", ")
        ));
        let mut value = shape.value.clone();
        value.operand = register;
        if shape.owned {
            crate::arc::own(cg, &value);
        }
        captures.push(Capture::Value(shape.name.clone(), value));
    }
    captures
}

/// The names some alternative binds as a list tail (`...rest`), anywhere in it.
fn tail_binders(alternatives: &[Pattern]) -> Vec<String> {
    let mut names = Vec::new();
    for alternative in alternatives {
        collect_tails(alternative, &mut names);
    }
    names
}

fn collect_tails(pattern: &Pattern, names: &mut Vec<String>) {
    match pattern {
        Pattern::List { elements, rest } => {
            names.extend(rest.iter().cloned());
            for element in elements {
                collect_tails(element, names);
            }
        }
        Pattern::Constructor { sub_patterns, .. } => {
            for slot in sub_patterns {
                collect_tails(slot, names);
            }
        }
        Pattern::Structural { fields, .. } => {
            for (_, slot) in fields {
                collect_tails(slot, names);
            }
        }
        Pattern::Or(alternatives) => {
            for alternative in alternatives {
                collect_tails(alternative, names);
            }
        }
        Pattern::Wildcard
        | Pattern::Literal(_)
        | Pattern::Binding(_)
        | Pattern::TypeAnnotated { .. } => {}
    }
}
