//! `match` lowering. Three shapes, dispatched on the arm patterns:
//!   * literal arms (bool/int/float/string) + catch-all — a compare/branch chain;
//!   * `Success`/`Error` arms — Result discrimination (a scrutinee that is not
//!     a `Result` takes the Success arm unconditionally, per the auto-wrap
//!     rule: any value may be matched as if wrapped in `Success`);
//!   * user-union variant arms — tag comparison against the heap block's leading
//!     discriminant, binding the variant's fields.

use crate::builder::Codegen;
use crate::collections::LIST_OWNER;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, MatchArm, Pattern};

pub(crate) fn gen_match(cg: &mut Codegen, value: &Expr, arms: &[MatchArm]) -> Result<Value> {
    let disc = gen_expr(cg, value)?;
    if arms.iter().any(|a| is_result_arm(&a.pattern)) {
        return gen_result_match(cg, &disc, arms);
    }
    if arms
        .iter()
        .any(|a| matches!(a.pattern, Pattern::List { .. }))
    {
        return gen_list_match(cg, &disc, arms);
    }
    if let Some(owner) = union_owner(cg, arms) {
        return gen_union_match(cg, &disc, arms, &owner);
    }
    gen_literal_match(cg, &disc, arms)
}

/// List-pattern match: each arm is length-guarded — `== n` for a fixed-length
/// `[a, b]`, `>= n` for a `[a, ...rest]` — then its prefix elements and tail are
/// bound from the runtime list. Coexists with a trailing catch-all
/// (`xs => …` / `_`). Implements [TYPE-LIST-PATTERNS].
fn gen_list_match(cg: &mut Codegen, disc: &Value, arms: &[MatchArm]) -> Result<Value> {
    let list_val = crate::cast::coerce_to(cg, disc.clone(), LType::Ptr)?;
    let len = cg.call("i64", "osprey_list_length", "i8*", &[&list_val.operand]);
    let (end, mut phi_in, last, mark) = match_state(cg, arms);

    for (i, arm) in arms.iter().enumerate() {
        match &arm.pattern {
            Pattern::List { elements, rest } => {
                let n = elements.len();
                let op = if rest.is_some() { "sge" } else { "eq" };
                let cond = cg.emit_reg(format!("icmp {op} i64 {len}, {n}"));
                let next_lbl = open_guarded_arm(cg, &cond);
                bind_list_arm(cg, &list_val, elements, rest.as_deref(), n);
                finish_guarded_arm(cg, arm, &mut phi_in, &next_lbl, i == last)?;
            }
            Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. } => {
                bind_catch_all(cg, &arm.pattern, &list_val);
                emit_arm_body(cg, arm, &mut phi_in)?;
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
            let elem = cg.call(
                "i64",
                "osprey_list_get",
                "i8*, i64",
                &[&list_val.operand, &idx.to_string()],
            );
            cg.bind(
                name.clone(),
                Value::new(elem, LType::I64).with_owner(list_val.payload_owner.clone()),
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
        let v = Value::handle(tail, LIST_OWNER).with_payload_owner(list_val.payload_owner.clone());
        // `osprey_list_drop` returns +1 on EVERY path (fresh view or retained
        // alias, plan 0011 M4a), so the arm owns it and must drop it at region
        // end — without this a `[head, ...tail]` recursion leaks one list
        // header (and, before the O(1)-view rewrite, a whole rebuilt trie) per
        // step. [GC-ARC-PERCEUS]
        crate::arc::own(cg, &v);
        cg.bind(name.to_string(), v);
    }
}

/// Evaluate a matched arm's body, then branch to a deferred exit block.  The
/// exit is emitted only after every arm has revealed its physical value shape,
/// allowing [`finish_phi`] to re-layout placeholder Error Results before they
/// meet at the closing `phi`.
fn push_arm(cg: &mut Codegen, body: &Expr, phi_in: &mut Vec<(Value, String)>) -> Result<()> {
    let v = gen_expr(cg, body)?;
    let exit = cg.fresh_label();
    cg.emit(format!("br label %{exit}"));
    phi_in.push((v, exit));
    Ok(())
}

fn is_result_arm(p: &Pattern) -> bool {
    matches!(p, Pattern::Constructor { name, .. } if name == "Success" || name == "Error")
}

/// The constructor name a pattern selects, if any: an explicit `Ctor { … }` or a
/// bare `Ctor` (a nullary variant lowers to a `Binding` indistinguishable from a
/// capture until we know the constructor table).
fn pattern_ctor<'a>(cg: &Codegen, p: &'a Pattern) -> Option<(&'a str, Vec<String>)> {
    match p {
        // `Ctor { a, b }` names its binders directly; `Ctor(a, b)` carries them
        // as sub-patterns, which bind by slot ([TYPE-UNION-POSITIONAL]). A
        // sub-pattern that is not a plain binder contributes no name — nested
        // destructuring is not supported and is rejected upstream.
        Pattern::Constructor {
            name,
            fields,
            sub_patterns,
        } if fields.is_empty() => Some((name, sub_patterns.iter().map(binder_name).collect())),
        Pattern::Constructor { name, fields, .. } => Some((name, fields.clone())),
        Pattern::Binding(name) if cg.is_ctor(name) => Some((name, Vec::new())),
        _ => None,
    }
}

/// The name a positional sub-pattern binds, or the ignored-slot placeholder for
/// a wildcard / unsupported shape.
fn binder_name(p: &Pattern) -> String {
    match p {
        Pattern::Binding(name) | Pattern::TypeAnnotated { name, .. } => name.clone(),
        _ => String::new(),
    }
}

/// The owner type name a constructor arm destructures, if any. That owner is
/// either a multi-variant union (present in `union_variants`) or a single-variant
/// record: the record shorthand `type V = V { … }` is classified as a record, so
/// it never enters `union_variants`, yet its heap block is the same
/// `{ i64 tag, fields… }` shape carrying tag `0` — so `gen_union_match` binds its
/// fields identically. Without the record arm here such a match falls through to
/// `gen_literal_match`, which rejects the constructor pattern (#175). Result
/// (`Success`/`Error`) arms are dispatched earlier and never reach this.
fn union_owner(cg: &Codegen, arms: &[MatchArm]) -> Option<String> {
    for a in arms {
        if let Some((name, _)) = pattern_ctor(cg, &a.pattern) {
            if let Some(view) = cg.ctor_layout(name) {
                if view.owner_is_record || cg.union_variants(&view.owner).is_some() {
                    return Some(view.owner);
                }
            }
        }
    }
    None
}

/// Result match. A struct-pointer Result (the uniform runtime ABI) branches on
/// its `i8` discriminant (`== 0` ⇒ Success) and binds the success arm's field to
/// the loaded payload; a bare scalar discriminant falls back to `disc >= 0`
/// (always Success), preserving the scalar's own type for the binding.
fn gen_result_match(cg: &mut Codegen, disc: &Value, arms: &[MatchArm]) -> Result<Value> {
    let success = arms.iter().find(|a| {
        matches!(&a.pattern,
        Pattern::Constructor { name, .. } if name == "Success")
    });
    let error = arms.iter().find(|a| {
        matches!(&a.pattern,
        Pattern::Constructor { name, .. } if name == "Error")
    });

    // (cond, success-binding, error-binding) by Result shape.
    let (cond, succ_val, err_val) = if disc.result_inner.is_some() {
        let d = crate::result::load_disc(cg, disc);
        let c = cg.fresh_reg();
        cg.emit(format!("{c} = icmp eq i8 {d}, 0"));
        // Success binds the value slot; Error binds the errmsg slot (the real
        // reason), so `Error { message }` sees the message regardless of the
        // success payload type. Implements [ERR-PAYLOAD].
        let bound = (
            c,
            crate::result::load_value(cg, disc),
            crate::result::load_errmsg_str(cg, disc),
        );
        // Both slots are now in registers, so a freshly produced block is dead
        // here. Retiring it at the match — rather than letting the region-end
        // drop do it — keeps the release off the path after the arms, which is
        // what allows a self-call in an arm to stay in tail position.
        // `consume_fresh` only fires for a pure-scalar block, whose errmsg is
        // rodata and so outlives the release. [GC-ARC-PERCEUS]
        crate::arc::consume_fresh(cg, disc);
        bound
    } else if matches!(disc.ty, LType::Str | LType::Ptr) {
        // A handle discriminant (e.g. a WHERE-constrained constructor that
        // currently always succeeds) has no numeric tag — take the Success arm
        // and bind the handle itself.
        let empty = Value::new(cg.string_constant("").operand, LType::Str);
        ("true".to_string(), disc.clone(), empty)
    } else {
        // A scalar that is NOT a `Result` is matched under the auto-wrap rule:
        // "any value may be matched as if wrapped in `Success`"
        // ([`crate::pattern`] mirror of the checker's rule in
        // osprey-types/src/pattern.rs). So the Success arm is taken
        // UNCONDITIONALLY, exactly as the handle branch above does.
        //
        // This used to branch on `icmp sge i64 value, 0`, treating a negative
        // scalar as an Error — a negative-sentinel heuristic no builtin relies
        // on. It made `-1 ?: 99` evaluate to `99` and `abs(-1)` see `0`: a
        // SILENT wrong answer on every negative value reaching `?:`.
        // Implements [PATTERN-RESULT-AUTOWRAP].
        let empty = Value::new(cg.string_constant("").operand, LType::Str);
        ("true".to_string(), disc.clone(), empty)
    };

    let sl = cg.fresh_label();
    let el = cg.fresh_label();
    let end = cg.fresh_label();
    cg.emit(format!("br i1 {cond}, label %{sl}, label %{el}"));

    let mark = crate::arc::frame_mark(cg);
    let mut phi_in: Vec<(Value, String)> = Vec::new();
    emit_result_arm(cg, &sl, success, succ_val, &end, &mut phi_in)?;
    emit_result_arm(cg, &el, error, err_val, &end, &mut phi_in)?;

    finish_phi(cg, &phi_in, &end, mark)
}

/// Emit one Result arm: open `label`, bind the constructor's payload field (if
/// the arm destructures one) to `bound`, evaluate the body into `phi_in`, then
/// branch to `end`. A `None` arm just falls through to `end`.
fn emit_result_arm(
    cg: &mut Codegen,
    label: &str,
    arm: Option<&MatchArm>,
    bound: Value,
    end: &str,
    phi_in: &mut Vec<(Value, String)>,
) -> Result<()> {
    cg.start_block(label);
    if let Some(arm) = arm {
        if let Pattern::Constructor { fields, .. } = &arm.pattern {
            if let Some(f) = fields.first() {
                cg.bind(f.clone(), bound);
            }
        }
        push_arm(cg, &arm.body, phi_in)?;
    } else {
        cg.emit(format!("br label %{end}"));
    }
    Ok(())
}

/// User-union match: read the leading tag of the heap block and branch per
/// variant, binding that variant's fields.
fn gen_union_match(
    cg: &mut Codegen,
    disc: &Value,
    arms: &[MatchArm],
    owner: &str,
) -> Result<Value> {
    // Load the discriminant tag (every variant block starts with `{ i64 tag, … }`).
    let tagp = cg.fresh_reg();
    cg.emit(format!("{tagp} = bitcast i8* {} to i64*", disc.operand));
    let tag = cg.fresh_reg();
    cg.emit(format!("{tag} = load i64, i64* {tagp}"));

    let end = cg.fresh_label();
    let mark = crate::arc::frame_mark(cg);
    let mut phi_in: Vec<(Value, String)> = Vec::new();
    let variants = cg.union_variants(owner).unwrap_or(&[]).to_vec();

    for arm in arms {
        if let Some((name, fields)) = pattern_ctor(cg, &arm.pattern) {
            let name = name.to_string();
            let vpos = variants.iter().position(|v| *v == name).unwrap_or(0);
            let vtag = i64::try_from(vpos).unwrap_or(0);
            let cond = cg.fresh_reg();
            cg.emit(format!("{cond} = icmp eq i64 {tag}, {vtag}"));
            let next_lbl = open_guarded_arm(cg, &cond);
            bind_variant_fields(cg, disc, &name, &fields);
            emit_arm_body(cg, arm, &mut phi_in)?;
            cg.start_block(&next_lbl);
        } else {
            match &arm.pattern {
                Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. } => {
                    bind_catch_all(cg, &arm.pattern, disc);
                    emit_arm_body(cg, arm, &mut phi_in)?;
                    break;
                }
                _ => return Err(CodegenError::unsupported("structural union arm")),
            }
        }
    }
    // A non-exhaustive fall-through is unreachable by construction.
    cg.emit("unreachable");
    finish_phi(cg, &phi_in, &end, mark)
}

/// Bind a matched variant's fields (in declared order) from the heap block.
fn bind_variant_fields(cg: &mut Codegen, disc: &Value, variant: &str, pat_fields: &[String]) {
    let Some(view) = cg.ctor_layout(variant) else {
        return;
    };
    let Some(struct_ty) = cg.ctor_struct_ty(variant) else {
        return;
    };
    if view.fields.is_empty() || pat_fields.is_empty() {
        return;
    }
    let src = cg.fresh_reg();
    cg.emit(format!(
        "{src} = bitcast i8* {} to {struct_ty}*",
        disc.operand
    ));
    // A NAMED payload binds each pattern field to the layout slot of the SAME
    // name, so a reordered destructuring (`PersonData { age, name }`) binds
    // correctly regardless of declaration order. A POSITIONALLY-declared variant
    // has no field names to resolve against and binds by slot instead: the
    // binder in column i takes payload slot i ([TYPE-UNION-POSITIONAL]).
    let positional = view
        .fields
        .first()
        .is_some_and(|(f, _)| osprey_ast::is_positional_field(f));
    for (column, bind_name) in pat_fields.iter().enumerate() {
        if bind_name.is_empty() {
            continue; // an ignored slot binds nothing
        }
        let slot = if positional {
            view.fields.get(column).map(|(_, fty)| (column, fty))
        } else {
            view.fields
                .iter()
                .enumerate()
                .find(|(_, (f, _))| f == bind_name)
                .map(|(idx, (_, fty))| (idx, fty))
        };
        let Some((idx, fty)) = slot else { continue };
        let fty = *fty;
        let loaded = crate::aggregate::load_field(cg, &struct_ty, src.as_str(), idx + 1, fty);
        let owner = cg.ctor_field_owner(variant, bind_name);
        cg.bind(bind_name.clone(), Value::new(loaded, fty).with_owner(owner));
    }
}

/// Literal/catch-all match: compare-and-branch chain joined by a `phi`.
fn gen_literal_match(cg: &mut Codegen, disc: &Value, arms: &[MatchArm]) -> Result<Value> {
    let (end, mut phi_in, last, mark) = match_state(cg, arms);

    for (i, arm) in arms.iter().enumerate() {
        match &arm.pattern {
            Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. } => {
                bind_catch_all(cg, &arm.pattern, disc);
                emit_arm_body(cg, arm, &mut phi_in)?;
                break;
            }
            Pattern::Literal(lit) => {
                let cond = gen_eq(cg, disc, lit)?;
                let next_lbl = open_guarded_arm(cg, &cond);
                finish_guarded_arm(cg, arm, &mut phi_in, &next_lbl, i == last)?;
            }
            _ => return Err(CodegenError::unsupported("destructuring match arm")),
        }
    }

    finish_phi(cg, &phi_in, &end, mark)
}

/// Allocate the join state for a match chain: the end label, the phi inputs,
/// the last-arm index, and the arc frame mark taken BEFORE any arm runs (the
/// [`crate::arc::move_phi_owners`] scrutinee gate).
fn match_state(
    cg: &mut Codegen,
    arms: &[MatchArm],
) -> (String, Vec<(Value, String)>, usize, usize) {
    let mark = crate::arc::frame_mark(cg);
    (
        cg.fresh_label(),
        Vec::new(),
        arms.len().saturating_sub(1),
        mark,
    )
}

/// Generate a successful match arm and branch to the common result block.
fn emit_arm_body(
    cg: &mut Codegen,
    arm: &MatchArm,
    phi_in: &mut Vec<(Value, String)>,
) -> Result<()> {
    push_arm(cg, &arm.body, phi_in)
}

/// Open a guarded arm: branch on `cond` into a fresh body block, make that
/// block current, and hand back the fall-through label the next arm starts at.
fn open_guarded_arm(cg: &mut Codegen, cond: &str) -> String {
    let body_lbl = cg.fresh_label();
    let next_lbl = cg.fresh_label();
    cg.emit(format!(
        "br i1 {cond}, label %{body_lbl}, label %{next_lbl}"
    ));
    cg.start_block(&body_lbl);
    next_lbl
}

/// Complete a guarded arm after its shape-specific bindings have been emitted.
fn finish_guarded_arm(
    cg: &mut Codegen,
    arm: &MatchArm,
    phi_in: &mut Vec<(Value, String)>,
    next: &str,
    is_last: bool,
) -> Result<()> {
    emit_arm_body(cg, arm, phi_in)?;
    cg.start_block(next);
    if is_last {
        cg.emit("unreachable");
    }
    Ok(())
}

/// Join the arm values with a `phi`. A single arm needs none. `Str`/`Ptr` count
/// as the same type (both `i8*`). A common owner / payload-owner across arms is
/// preserved so a matched handle (record, nested list) stays field-accessible /
/// indexable.
///
/// Arms that disagree on LLVM type are a hard error, not a silent Unit: a `phi`
/// over them would be ill-typed, and yielding Unit instead converted a class of
/// type-system mistakes into an expression that quietly evaluated to nothing.
fn finish_phi(
    cg: &mut Codegen,
    phi_in: &[(Value, String)],
    end: &str,
    mark: usize,
) -> Result<Value> {
    let target_result_inner = result_join_inner(phi_in)?;
    let mut incoming_values = Vec::with_capacity(phi_in.len());
    for (value, exit) in phi_in {
        cg.start_block(exit);
        let adapted = match target_result_inner {
            Some(inner) => crate::result::repack_to_inner(cg, value.clone(), inner)?,
            None => value.clone(),
        };
        let pred = cg.snapshot_to(end);
        incoming_values.push((adapted, pred));
    }
    cg.start_block(end);

    let Some((first_val, _)) = incoming_values.first() else {
        return Ok(Value::unit());
    };
    let ty = first_val.ty;
    let llvm_ty = first_val.llvm_ty();
    if let Some((odd, _)) = incoming_values.iter().find(|(v, _)| v.llvm_ty() != llvm_ty) {
        if !cg.value_discarded {
            return Err(CodegenError::invalid(format!(
                "match arms disagree on type: `{}` and `{}`",
                llvm_ty,
                odd.llvm_ty()
            )));
        }
        return Ok(Value::unit());
    }
    let incoming = incoming_values
        .iter()
        .map(|(v, blk)| format!("[ {}, %{blk} ]", v.operand))
        .collect::<Vec<_>>()
        .join(", ");
    let reg = cg.fresh_reg();
    cg.emit(format!("{reg} = phi {llvm_ty} {incoming}"));
    let common = |sel: fn(&Value) -> Option<String>| {
        let first = sel(first_val);
        incoming_values
            .iter()
            .all(|(v, _)| sel(v) == first)
            .then_some(first)
            .flatten()
    };
    // Preserve Result identity across the merge only when every arm has the
    // exact same block layout. The LLVM-type check above rejects mixed Result
    // payload layouts rather than emitting a broad pointer phi that discards
    // the discriminant-bearing type.
    let result_inner = first_val.result_inner.filter(|first| {
        incoming_values.iter().all(|(v, _)| {
            v.result_inner
                .is_some_and(|ri| ri.as_str() == first.as_str())
        })
    });
    let mut out = match result_inner {
        Some(inner) => Value::result(reg, inner),
        None => Value::new(reg, ty).with_owner(common(|v| v.osp_ty.clone())),
    };
    out.result_inner_is_placeholder = result_inner.is_some()
        && incoming_values
            .iter()
            .all(|(v, _)| v.result_inner_is_placeholder);
    out.payload_owner = common(|v| v.payload_owner.clone());
    // Perceus join transfer: if every arm produced a fresh owner AFTER `mark`
    // (i.e. inside its own arm — never the scrutinee, which predates the mark
    // and lives on every path), the phi owns the merged value directly — the
    // arm entries move into it, no dup, no per-arm drop. Ledger bookkeeping
    // only; the repositioned dup/drop calls are no-ops off ARC.
    let incoming_ops = incoming_values
        .iter()
        .map(|(v, _)| v.operand.clone())
        .collect::<Vec<_>>();
    crate::arc::move_phi_owners(cg, &incoming_ops, &out, mark);
    Ok(out)
}

/// Resolve the concrete success-slot layout for a Result-valued match.  A bare
/// Error constructor contributes only a placeholder layout; any real producer
/// fixes the contextual `T`.  Multiple concrete layouts remain a hard backend
/// error (the type checker should already have rejected such arms).
fn result_join_inner(phi_in: &[(Value, String)]) -> Result<Option<LType>> {
    if phi_in.is_empty() || phi_in.iter().any(|(v, _)| v.result_inner.is_none()) {
        return Ok(None);
    }
    let mut concrete = phi_in
        .iter()
        .filter(|(v, _)| !v.result_inner_is_placeholder)
        .filter_map(|(v, _)| v.result_inner);
    let target = concrete
        .next()
        .or_else(|| phi_in.first().and_then(|(v, _)| v.result_inner));
    if let Some(target_inner) = target {
        if let Some(other) = concrete.find(|inner| *inner != target_inner) {
            return Err(CodegenError::invalid(format!(
                "match Result arms disagree on success type: `{target_inner}` and `{other}`"
            )));
        }
    }
    Ok(target)
}

fn bind_catch_all(cg: &mut Codegen, pattern: &Pattern, disc: &Value) {
    match pattern {
        Pattern::Binding(name) | Pattern::TypeAnnotated { name, .. } => {
            cg.bind(name.clone(), disc.clone());
        }
        _ => {}
    }
}

/// Equality test between the discriminant and a literal pattern → the `i1`
/// operand.
fn gen_eq(cg: &mut Codegen, disc: &Value, lit: &Expr) -> Result<String> {
    let pat = gen_expr(cg, lit)?;
    Ok(crate::expr::gen_comparison(cg, "==", disc.clone(), pat)?.operand)
}
