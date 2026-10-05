//! List pattern lowering.
use super::{
    bind_value, finish_guarded_arm, finish_phi, match_state, open_guarded_arm, take_catch_all,
};
use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::llty::{LType, Value};
use osprey_ast::{MatchArm, Pattern};

/// List-pattern match: each arm is length-guarded — `== n` for a fixed-length
/// `[a, b]`, `>= n` for a `[a, ...rest]` — then its prefix elements and tail are
/// bound from the runtime list. Coexists with a trailing catch-all
/// (`xs => …` / `_`). Implements [TYPE-LIST-PATTERNS].
///
/// A flat list **literal** is a different layout from an `OspreyList` handle, so
/// the scrutinee is rebuilt into a runtime list first (a no-op for one that
/// already is). Without that, `osprey_list_length` read the literal's foreign
/// `{ i64, i8* }` header and the program **segfaulted** — reachable from ordinary
/// code, since `fn headOf(xs) = match xs { [] => -1  [h, ...t] => h }` crashed on
/// `headOf([7, 8])` while the same call on a `listAppend` chain worked.
///
/// The catch-all arm binds the ORIGINAL scrutinee, not the rebuilt list: a
/// literal bound by `xs => xs[0]` must keep the literal layout, because
/// [`crate::listlit::gen_index`] reads its element type off the literal's owner
/// tag. A literal carries no `payload_owner`, so the rebuilt value loses no
/// element-owner information the guarded arms could have used.
pub(super) fn gen_list_match(cg: &mut Codegen, disc: &Value, arms: &[MatchArm]) -> Result<Value> {
    let original = crate::cast::coerce_to(cg, disc.clone(), LType::Ptr)?;
    let rebuilt = crate::listlit::to_runtime_list(cg, disc.clone());
    let list_val = crate::cast::coerce_to(cg, rebuilt, LType::Ptr)?;
    let len = cg.call("i64", "osprey_list_length", "i8*", &[&list_val.operand]);
    let (end, mut phi_in, last, mark) = match_state(cg, arms);

    for (i, arm) in arms.iter().enumerate() {
        match &arm.pattern {
            Pattern::List { elements, rest } => {
                let n = elements.len();
                let op = if rest.is_some() { "sge" } else { "eq" };
                let cond = cg.emit_reg(format!("icmp {op} i64 {len}, {n}"));
                let next_lbl = open_guarded_arm(cg, &cond);
                finish_guarded_arm(cg, arm, &mut phi_in, &next_lbl, i == last, |cg| {
                    bind_list_arm(cg, &list_val, elements, rest.as_deref(), n);
                })?;
            }
            Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. } => {
                take_catch_all(cg, arm, &original, &mut phi_in)?;
                break;
            }
            _ => return Err(CodegenError::unsupported("non-list arm in list match")),
        }
    }
    finish_phi(cg, &phi_in, &end, mark)
}

/// Bind a matched list arm's prefix elements (`osprey_list_get(l, i)`) and its
/// `...rest` tail (`osprey_list_drop(l, n)`). The length guard at the call site
/// proves every index is in bounds. Elements cross as the uniform `i64`,
/// carrying the scrutinee's element owner so a list-of-handles stays usable; a
/// `_` element binds nothing.
///
/// A head binding BORROWS: `osprey_list_get` hands back the list's own
/// reference with no count, and the `i64` spelling keeps the ARC ledger from
/// ever registering it as an owner — so nothing dups it and nothing drops it,
/// and it stays valid exactly as long as the scrutinee does. The `...rest`
/// view below is the opposite: a real +1 the arm owns. [GC-ARC-PERCEUS]
fn bind_list_arm(
    cg: &mut Codegen,
    list_val: &Value,
    elements: &[Pattern],
    rest: Option<&str>,
    n: usize,
) {
    for (idx, el) in elements.iter().enumerate() {
        if let Pattern::Binding(name) = el {
            let raw = cg.call(
                "i64",
                "osprey_list_get",
                "i8*, i64",
                &[&list_val.operand, &idx.to_string()],
            );
            // A destructured element is bound at the list's element type, so a
            // `[first, second]` arm over a `List<float>` binds floats
            // ([`crate::collections::LIST_TAG`]).
            let elem = crate::collections::elem_value(cg, list_val, &raw);
            bind_value(
                cg,
                name.clone(),
                elem.with_owner(list_val.payload_owner.clone()),
            );
        }
    }
    if let Some(name) = rest {
        let tail = cg.call(
            "i8*",
            "osprey_list_drop",
            "i8*, i64",
            &[&list_val.operand, &n.to_string()],
        );
        let tail_owner =
            crate::collections::list_owner(crate::collections::tagged_elem(list_val).as_deref());
        let v = Value::handle(tail, tail_owner).with_payload_owner(list_val.payload_owner.clone());
        // `osprey_list_drop` returns +1 on EVERY path (fresh view or retained
        // alias, plan 0011 M4a), so the arm owns it and must drop it at region
        // end — without this a `[head, ...tail]` recursion leaks one list
        // header (and, before the O(1)-view rewrite, a whole rebuilt trie) per
        // step. [GC-ARC-PERCEUS]
        crate::arc::own(cg, &v);
        bind_value(cg, name.to_string(), v);
    }
}
