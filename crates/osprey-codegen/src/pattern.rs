//! `match` lowering. Three shapes, dispatched on the arm patterns:
//!   * literal arms (bool/int/float/string) + catch-all — a compare/branch chain;
//!   * `Success`/`Error` arms — Result discrimination (an already-unwrapped
//!     scalar scrutinee falls back to `disc >= 0` ⇒ Success);
//!   * user-union variant arms — tag comparison against the heap block's leading
//!     discriminant, binding the variant's fields.

use crate::builder::Codegen;
use crate::collections::LIST_OWNER;
use crate::conv::as_i64;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::{Expr, MatchArm, Pattern};

pub(crate) fn gen_match(cg: &mut Codegen, value: &Expr, arms: &[MatchArm]) -> Result<Value> {
    let disc = gen_expr(cg, value)?;
    // `result ?: default` lowers to `match result { true => result  false => default }`.
    // A Result discriminant matched against bool literals is only ever this Elvis
    // form, so dispatch it to unwrap-or-default.
    if disc.result_inner.is_some() && is_bool_elvis(arms) {
        return gen_result_elvis(cg, &disc, arms);
    }
    if arms.iter().any(|a| is_result_arm(&a.pattern)) {
        return gen_result_match(cg, disc, arms);
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
                let body_lbl = cg.fresh_label();
                let next_lbl = cg.fresh_label();
                cg.emit(format!(
                    "br i1 {cond}, label %{body_lbl}, label %{next_lbl}"
                ));
                cg.start_block(&body_lbl);
                bind_list_arm(cg, &list_val, elements, rest.as_deref(), n);
                finish_guarded_arm(cg, arm, &mut phi_in, &end, &next_lbl, i == last)?;
            }
            Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. } => {
                bind_catch_all(cg, &arm.pattern, &list_val);
                emit_arm_body(cg, arm, &mut phi_in, &end)?;
                break;
            }
            _ => return Err(CodegenError::unsupported("non-list arm in list match")),
        }
    }
    cg.start_block(&end);
    finish_phi(cg, &phi_in, mark)
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

/// Whether the arms are the bool-ternary shape (`true => …  false => …`) the
/// Elvis operator desugars to.
fn is_bool_elvis(arms: &[MatchArm]) -> bool {
    arms.len() == 2
        && arms
            .iter()
            .all(|a| matches!(&a.pattern, Pattern::Literal(e) if matches!(**e, Expr::Bool(_))))
}

/// `result ?: default` — branch on the Result discriminant, yielding the
/// unwrapped success payload or evaluating the `false` (default) arm.
fn gen_result_elvis(cg: &mut Codegen, disc: &Value, arms: &[MatchArm]) -> Result<Value> {
    let inner = disc.result_inner.unwrap_or(LType::I64);
    let default_arm = arms
        .iter()
        .find(|a| matches!(&a.pattern, Pattern::Literal(e) if matches!(**e, Expr::Bool(false))));
    let (_sl, el, end) = crate::result::open_result_branch(cg, disc);
    let succ = crate::result::load_value(cg, disc);
    let sb = cg.snapshot_to(&end);

    cg.start_block(&el);
    let def = match default_arm {
        Some(a) => gen_expr(cg, &a.body)?,
        None => Value::unit(),
    };
    let def = crate::cast::coerce_to(cg, def, inner)?;
    let eb = cg.snapshot_to(&end);

    cg.start_block(&end);
    let reg = cg.fresh_reg();
    cg.emit(format!(
        "{reg} = phi {inner} [ {}, %{sb} ], [ {}, %{eb} ]",
        succ.operand, def.operand
    ));
    Ok(Value::new(reg, inner))
}

/// Evaluate a matched arm's body in the current block and record its
/// `(value, block)` for the closing `phi` — the step every arm shape ends with.
fn push_arm(cg: &mut Codegen, body: &Expr, phi_in: &mut Vec<(Value, String)>) -> Result<()> {
    let v = gen_expr(cg, body)?;
    let blk = cg.cur_block().to_string();
    phi_in.push((v, blk));
    Ok(())
}

fn is_result_arm(p: &Pattern) -> bool {
    matches!(p, Pattern::Constructor { name, .. } if name == "Success" || name == "Error")
}

/// The constructor name a pattern selects, if any: an explicit `Ctor { … }` or a
/// bare `Ctor` (a nullary variant lowers to a `Binding` indistinguishable from a
/// capture until we know the constructor table).
fn pattern_ctor<'a>(cg: &Codegen, p: &'a Pattern) -> Option<(&'a str, &'a [String])> {
    match p {
        Pattern::Constructor { name, fields, .. } => Some((name, fields)),
        Pattern::Binding(name) if cg.is_ctor(name) => Some((name, &[])),
        _ => None,
    }
}

/// If any arm is a user-union variant constructor, the union's owner name.
fn union_owner(cg: &Codegen, arms: &[MatchArm]) -> Option<String> {
    for a in arms {
        if let Some((name, _)) = pattern_ctor(cg, &a.pattern) {
            if let Some(view) = cg.ctor_layout(name) {
                if !view.owner_is_record && cg.union_variants(&view.owner).is_some() {
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
fn gen_result_match(cg: &mut Codegen, disc: Value, arms: &[MatchArm]) -> Result<Value> {
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
        let d = crate::result::load_disc(cg, &disc);
        let c = cg.fresh_reg();
        cg.emit(format!("{c} = icmp eq i8 {d}, 0"));
        // Success binds the value slot; Error binds the errmsg slot (the real
        // reason), so `Error { message }` sees the message regardless of the
        // success payload type. Implements [ERR-PAYLOAD].
        (
            c,
            crate::result::load_value(cg, &disc),
            crate::result::load_errmsg_str(cg, &disc),
        )
    } else if matches!(disc.ty, LType::Str | LType::Ptr) {
        // A handle discriminant (e.g. a WHERE-constrained constructor that
        // currently always succeeds) has no numeric tag — take the Success arm
        // and bind the handle itself.
        let empty = Value::new(cg.string_constant("").operand, LType::Str);
        ("true".to_string(), disc.clone(), empty)
    } else {
        let scalar = disc.clone();
        let di = as_i64(cg, disc)?;
        let c = cg.fresh_reg();
        cg.emit(format!("{c} = icmp sge i64 {}, 0", di.operand));
        (
            c,
            scalar,
            Value::new(cg.string_constant("").operand, LType::Str),
        )
    };

    let sl = cg.fresh_label();
    let el = cg.fresh_label();
    let end = cg.fresh_label();
    cg.emit(format!("br i1 {cond}, label %{sl}, label %{el}"));

    let mark = crate::arc::frame_mark(cg);
    let mut phi_in: Vec<(Value, String)> = Vec::new();
    emit_result_arm(cg, &sl, success, succ_val, &end, &mut phi_in)?;
    emit_result_arm(cg, &el, error, err_val, &end, &mut phi_in)?;

    cg.start_block(&end);
    finish_phi(cg, &phi_in, mark)
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
    }
    cg.emit(format!("br label %{end}"));
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
            let fields = fields.to_vec();
            let vpos = variants.iter().position(|v| *v == name).unwrap_or(0);
            let vtag = i64::try_from(vpos).unwrap_or(0);
            let cond = cg.fresh_reg();
            cg.emit(format!("{cond} = icmp eq i64 {tag}, {vtag}"));
            let body_lbl = cg.fresh_label();
            let next_lbl = cg.fresh_label();
            cg.emit(format!(
                "br i1 {cond}, label %{body_lbl}, label %{next_lbl}"
            ));
            cg.start_block(&body_lbl);
            bind_variant_fields(cg, disc, &name, &fields);
            emit_arm_body(cg, arm, &mut phi_in, &end)?;
            cg.start_block(&next_lbl);
        } else {
            match &arm.pattern {
                Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. } => {
                    if let Pattern::Binding(n) | Pattern::TypeAnnotated { name: n, .. } =
                        &arm.pattern
                    {
                        cg.bind(n.clone(), disc.clone());
                    }
                    emit_arm_body(cg, arm, &mut phi_in, &end)?;
                    break;
                }
                _ => return Err(CodegenError::unsupported("structural union arm")),
            }
        }
    }
    // A non-exhaustive fall-through is unreachable by construction.
    cg.emit("unreachable");
    cg.start_block(&end);
    finish_phi(cg, &phi_in, mark)
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
    // Bind each pattern field to the layout slot of the SAME name, so a reordered
    // destructuring (`PersonData { age, name }`) binds correctly regardless of the
    // declaration order.
    for bind_name in pat_fields {
        let Some((idx, (_, fty))) = view
            .fields
            .iter()
            .enumerate()
            .find(|(_, (f, _))| f == bind_name)
        else {
            continue;
        };
        let loaded = crate::aggregate::load_field(cg, &struct_ty, src.as_str(), idx + 1, *fty);
        let owner = cg.ctor_field_owner(variant, bind_name);
        cg.bind(
            bind_name.clone(),
            Value::new(loaded, *fty).with_owner(owner),
        );
    }
}

/// Literal/catch-all match: compare-and-branch chain joined by a `phi`.
fn gen_literal_match(cg: &mut Codegen, disc: &Value, arms: &[MatchArm]) -> Result<Value> {
    let (end, mut phi_in, last, mark) = match_state(cg, arms);

    for (i, arm) in arms.iter().enumerate() {
        match &arm.pattern {
            Pattern::Wildcard | Pattern::Binding(_) | Pattern::TypeAnnotated { .. } => {
                bind_catch_all(cg, &arm.pattern, disc);
                emit_arm_body(cg, arm, &mut phi_in, &end)?;
                break;
            }
            Pattern::Literal(lit) => {
                let cond = gen_eq(cg, disc, lit)?;
                let body_lbl = cg.fresh_label();
                let next_lbl = cg.fresh_label();
                cg.emit(format!(
                    "br i1 {cond}, label %{body_lbl}, label %{next_lbl}"
                ));
                cg.start_block(&body_lbl);
                finish_guarded_arm(cg, arm, &mut phi_in, &end, &next_lbl, i == last)?;
            }
            _ => return Err(CodegenError::unsupported("destructuring match arm")),
        }
    }

    cg.start_block(&end);
    finish_phi(cg, &phi_in, mark)
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
    end: &str,
) -> Result<()> {
    push_arm(cg, &arm.body, phi_in)?;
    cg.emit(format!("br label %{end}"));
    Ok(())
}

/// Complete a guarded arm after its shape-specific bindings have been emitted.
fn finish_guarded_arm(
    cg: &mut Codegen,
    arm: &MatchArm,
    phi_in: &mut Vec<(Value, String)>,
    end: &str,
    next: &str,
    is_last: bool,
) -> Result<()> {
    emit_arm_body(cg, arm, phi_in, end)?;
    cg.start_block(next);
    if is_last {
        cg.emit("unreachable");
    }
    Ok(())
}

/// Join the arm values with a `phi`. A single arm needs none. When the arms
/// disagree on LLVM type the match is being used as a statement (its value is
/// discarded) — a `phi` would be ill-typed, so yield Unit instead of emitting
/// one. `Str`/`Ptr` count as the same type (both `i8*`). A common owner /
/// payload-owner across arms is preserved so a matched handle (record, nested
/// list) stays field-accessible / indexable.
#[expect(
    clippy::unnecessary_wraps,
    reason = "kept Result-returning for the uniform generator interface"
)]
fn finish_phi(cg: &mut Codegen, phi_in: &[(Value, String)], mark: usize) -> Result<Value> {
    let Some((first_val, _)) = phi_in.first() else {
        return Ok(Value::unit());
    };
    let ty = first_val.ty;
    if phi_in.iter().any(|(v, _)| v.ty.as_str() != ty.as_str()) {
        return Ok(Value::unit());
    }
    let incoming = phi_in
        .iter()
        .map(|(v, blk)| format!("[ {}, %{blk} ]", v.operand))
        .collect::<Vec<_>>()
        .join(", ");
    let reg = cg.fresh_reg();
    cg.emit(format!("{reg} = phi {ty} {incoming}"));
    let common = |sel: fn(&Value) -> Option<String>| {
        let first = sel(first_val);
        phi_in
            .iter()
            .all(|(v, _)| sel(v) == first)
            .then_some(first)
            .flatten()
    };
    // Preserve Result identity across the merge: when every arm is a Result of
    // the same block layout (Success `{Ptr,i8}` / Error `{Str,i8}` are both
    // `{i8*,i8}`), the phi *is* a Result, so carry the success arm's inner type.
    // Without this a `match … { Success … Error … }` looks like a bare handle and
    // a `-> Result` function body gets wrapped a second time.
    let result_inner = first_val.result_inner.filter(|first| {
        phi_in.iter().all(|(v, _)| {
            v.result_inner
                .is_some_and(|ri| ri.as_str() == first.as_str())
        })
    });
    let mut out = Value::new(reg, ty)
        .with_owner(common(|v| v.osp_ty.clone()))
        .with_payload_owner(common(|v| v.payload_owner.clone()));
    out.result_inner = result_inner;
    // Perceus join transfer: if every arm produced a fresh owner AFTER `mark`
    // (i.e. inside its own arm — never the scrutinee, which predates the mark
    // and lives on every path), the phi owns the merged value directly — the
    // arm entries move into it, no dup, no per-arm drop. Ledger bookkeeping
    // only; the repositioned dup/drop calls are no-ops off ARC.
    let incoming_ops = phi_in
        .iter()
        .map(|(v, _)| v.operand.clone())
        .collect::<Vec<_>>();
    crate::arc::move_phi_owners(cg, &incoming_ops, &out, mark);
    Ok(out)
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
    let reg = cg.fresh_reg();
    let is_str = |t: LType| t == LType::Str || t == LType::Ptr;
    if is_str(disc.ty) && is_str(pat.ty) {
        cg.add_extern("declare i32 @strcmp(i8*, i8*)");
        let c = cg.fresh_reg();
        cg.emit(format!(
            "{c} = call i32 @strcmp(i8* {}, i8* {})",
            disc.operand, pat.operand
        ));
        cg.emit(format!("{reg} = icmp eq i32 {c}, 0"));
    } else if disc.ty == LType::Double || pat.ty == LType::Double {
        cg.emit(format!(
            "{reg} = fcmp oeq double {}, {}",
            disc.operand, pat.operand
        ));
    } else {
        let d = as_i64(cg, disc.clone())?;
        let p = as_i64(cg, pat)?;
        cg.emit(format!("{reg} = icmp eq i64 {}, {}", d.operand, p.operand));
    }
    Ok(reg)
}
