//! Deep content comparison for `structuralEquals(a, b)`. One comparison
//! function per distinct compared type, generated on demand and memoised in
//! [`Codegen::seq_fns`], exactly as the erased-`any` renderers are
//! ([`crate::anybox`]): a recursive type therefore calls its own comparator
//! instead of being inlined forever. Comparison only BORROWS its operands —
//! both travel the uniform `i64` word ABI ([`crate::conv::box_to_i64`]), so
//! one comparator works under every memory backend. Implements
//! [BUILTIN-STRUCTURAL-EQUALS] (docs/specs/runtime/0012-Built-InFunctions.md):
//! scalars by value (floats with `fcmp oeq`, so NaN stays unequal to itself
//! wherever it is stored), records and tuples field by field, unions by
//! variant then payload, lists by length then elements, maps by keys then
//! values. No pointer fast path: same-handle equality would make a value
//! holding NaN equal to itself, which the spec forbids.

use crate::builder::Codegen;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use osprey_ast::Expr;
use osprey_types::{names, Type};

mod constructed;
mod walks;
use walks::{compare_list, compare_map, compare_union};

/// Comparator function prefix. The suffix is a deterministic slug of the
/// compared type's rendering, so a Default/ML twin pair mints the same name.
const SEQ_FN: &str = "osp.seq.";

/// The call site: evaluate both operands, borrow them into the uniform word
/// ABI, and dispatch to the memoised comparator for their type.
pub(crate) fn structural_equals(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let a = operand(cg, args, 0)?;
    let b = operand(cg, args, 1)?;
    let ty = compared_type(cg, &a, &b)?;
    let name = ensure_comparator(cg, &ty)?;
    let (wa, wb) = (
        crate::conv::box_to_i64(cg, a),
        crate::conv::box_to_i64(cg, b),
    );
    let r = comparator_call(cg, &name, &wa.operand, &wb.operand);
    Ok(Value::new(r, LType::I1))
}

/// A call to an emitted comparator. Emitted directly, never through
/// [`Codegen::call`]: the function is DEFINED in this module, so declaring it
/// too would be a duplicate definition clang rejects.
fn comparator_call(cg: &mut Codegen, name: &str, a: &str, b: &str) -> String {
    cg.emit_reg(format!("call i1 @{name}(i64 {a}, i64 {b})"))
}

/// The `i`-th operand, with a flat list literal rebuilt as the runtime list
/// the comparator walks. An unhandled `Result` never reaches here — the
/// checker rejects the type — so this is the emitter's own guarantee.
fn operand(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<Value> {
    let e = args
        .get(i)
        .ok_or_else(|| CodegenError::invalid("structuralEquals needs two arguments"))?;
    let mut v = gen_expr(cg, e)?;
    if v.inferred_type
        .as_ref()
        .is_none_or(osprey_types::has_type_var)
    {
        v.inferred_type = constructed::operand_type(cg, e).or(v.inferred_type);
    }
    let v = crate::listlit::to_runtime_list(cg, v);
    match v.result_inner {
        Some(_) => Err(CodegenError::unsupported(
            "structuralEquals cannot compare an unhandled Result",
        )),
        None => Ok(v),
    }
}

/// The one type both operands unified to, as inference published it. A
/// constructor expression carries its owner instead of an inferred type, and
/// a variant's own fields decide its comparison, so the owner stands in.
fn compared_type(cg: &Codegen, a: &Value, b: &Value) -> Result<Type> {
    concrete(a)
        .or_else(|| concrete(b))
        .or_else(|| owned(cg, a).or_else(|| owned(cg, b)))
        .ok_or_else(open_operand)
}

fn concrete(v: &Value) -> Option<Type> {
    v.inferred_type
        .clone()
        .filter(|ty| !osprey_types::has_type_var(ty))
}

fn owned(cg: &Codegen, v: &Value) -> Option<Type> {
    v.osp_ty
        .clone()
        .filter(|owner| cg.is_ctor(owner) || cg.union_variants(owner).is_some())
        .map(|owner| Type::con(owner, Vec::new()))
}

/// The comparator for `ty`, emitting it on first use. Registration precedes
/// emission so a self-referential type terminates.
fn ensure_comparator(cg: &mut Codegen, ty: &Type) -> Result<String> {
    let key = ty.to_string();
    if let Some(name) = cg.seq_fns.get(&key) {
        return Ok(name.clone());
    }
    let name = format!("{SEQ_FN}{}", slug(&key));
    let _ = cg.seq_fns.insert(key, name.clone());
    let saved = cg.enter_nested_fn();
    let body = compare_body(cg, ty, "%a", "%b").map(|eq| format!("ret i1 {eq}"));
    let params = [(LType::I64, "a".into()), (LType::I64, "b".into())];
    let rendered = body?;
    cg.emit(rendered);
    cg.exit_nested_fn(saved, "i1", &name, &params);
    Ok(name)
}

/// A deterministic LLVM-identifier-safe rendering of a type: Osprey type names
/// are identifiers, so joining the non-identifier punctuation as `.` cannot
/// collide with a name another type spells.
fn slug(key: &str) -> String {
    key.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '.' })
        .collect()
}

/// The body of `ty`'s comparator: scalars inline, aggregates walked.
fn compare_body(cg: &mut Codegen, ty: &Type, a: &str, b: &str) -> Result<String> {
    if let Some(scalar) = compare_scalar(cg, ty, a, b) {
        return Ok(scalar);
    }
    compare_shape(cg, ty, a, b)
}

fn compare_scalar(cg: &mut Codegen, ty: &Type, a: &str, b: &str) -> Option<String> {
    let Type::Con { name, .. } = ty else {
        return None;
    };
    let r = match name.as_str() {
        names::INT | names::UNIT | names::BOOL => cg.emit_reg(format!("icmp eq i64 {a}, {b}")),
        names::FLOAT => floats_equal(cg, a, b),
        names::STRING => strings_equal(cg, a, b),
        _ => return None,
    };
    Some(r)
}

/// [FLOAT-COMPARE]: ordered equality, so NaN is unequal to itself.
fn floats_equal(cg: &mut Codegen, a: &str, b: &str) -> String {
    let da = crate::conv::unbox_from_i64(cg, a, LType::Double);
    let db = crate::conv::unbox_from_i64(cg, b, LType::Double);
    cg.emit_reg(format!("fcmp oeq double {}, {}", da.operand, db.operand))
}

fn strings_equal(cg: &mut Codegen, a: &str, b: &str) -> String {
    let pa = crate::conv::unbox_from_i64(cg, a, LType::Str);
    let pb = crate::conv::unbox_from_i64(cg, b, LType::Str);
    let c = cg.call("i32", "strcmp", "i8*, i8*", &[&pa.operand, &pb.operand]);
    cg.emit_reg(format!("icmp eq i32 {c}, 0"))
}

/// Compare two `i64` words holding values of `ty`: a scalar compares inline,
/// an aggregate dispatches to its own memoised comparator — the boundary that
/// keeps a recursive type finite.
fn compare_word(cg: &mut Codegen, ty: &Type, a: &str, b: &str) -> Result<String> {
    if let Some(scalar) = compare_scalar(cg, ty, a, b) {
        return Ok(scalar);
    }
    let name = ensure_comparator(cg, ty)?;
    Ok(comparator_call(cg, &name, a, b))
}

/// The aggregate shapes a comparator body walks: containers and aggregates
/// compare their contents, everything the checker rejected fails loudly.
fn compare_shape(cg: &mut Codegen, ty: &Type, a: &str, b: &str) -> Result<String> {
    match ty {
        Type::Var(_) => Err(open_operand()),
        Type::Fun { .. } | Type::Union { .. } => Err(rejected_operand(ty)),
        Type::Con { name, args } => match name.as_str() {
            names::LIST => compare_list(cg, arg(args, 0)?, a, b),
            names::MAP => compare_map(cg, arg(args, 1)?, a, b),
            _ if cg.union_variants(name).is_some() => compare_union(cg, name, args, a, b),
            _ if cg.ctor_layout(name).is_some() => compare_record(cg, ty, a, b),
            _ => Err(rejected_operand(ty)),
        },
        Type::Record { .. } => compare_record(cg, ty, a, b),
    }
}

fn open_operand() -> CodegenError {
    CodegenError::unsupported("structuralEquals cannot compare an unresolved type")
}

fn rejected_operand(ty: &Type) -> CodegenError {
    CodegenError::unsupported(format!(
        "structuralEquals cannot compare `{ty}`: the checker rejects functions, \
         runtime handles, `any` and `Result`"
    ))
}

fn arg(args: &[Type], i: usize) -> Result<&Type> {
    args.get(i).ok_or_else(open_operand)
}

/// An `i1` result slot pre-set to `1`, the shape every comparator's early
/// exits write `0` into.
fn true_slot(cg: &mut Codegen) -> String {
    let res = cg.hoist_slot("i1");
    cg.emit(format!("store i1 1, i1* {res}"));
    res
}

fn load_slot(cg: &mut Codegen, res: &str) -> String {
    cg.emit_reg(format!("load i1, i1* {res}"))
}

fn word_ptr(cg: &mut Codegen, word: &str) -> String {
    crate::conv::unbox_from_i64(cg, word, LType::Ptr).operand
}

fn typed_ptr(cg: &mut Codegen, word: &str, struct_ty: &str) -> String {
    cg.emit_reg(format!("inttoptr i64 {word} to {struct_ty}*"))
}

/// A record (or tuple): every field compared, and-folded. One block shape for
/// declared records, generic instantiations and anonymous rows — the layout
/// [`crate::aggregate::record_block`] resolves.
fn compare_record(cg: &mut Codegen, ty: &Type, a: &str, b: &str) -> Result<String> {
    let owner = crate::types::owner_name(&cg.prog, ty).ok_or_else(|| rejected_operand(ty))?;
    let block =
        crate::aggregate::record_block(cg, &owner, Some(ty)).ok_or_else(|| rejected_operand(ty))?;
    compare_fields(cg, ty, &owner, &block.struct_ty, a, b, &block.fields)
}

/// Compare every slot of one `{ i64 tag, fields… }` block at `a`/`b`, and
/// folded into a single `i1`. An empty row is equal (only the tags could
/// differ, and a record's tag is constant).
fn compare_fields(
    cg: &mut Codegen,
    record_ty: &Type,
    owner: &str,
    struct_ty: &str,
    a: &str,
    b: &str,
    fields: &[(String, LType)],
) -> Result<String> {
    let (pa, pb) = (typed_ptr(cg, a, struct_ty), typed_ptr(cg, b, struct_ty));
    let mut eq = "true".to_string();
    for (i, (field, fty)) in fields.iter().enumerate() {
        let fty_ty = field_type(cg, record_ty, field)?;
        let va = slot_word(cg, owner, struct_ty, &pa, i, *fty);
        let vb = slot_word(cg, owner, struct_ty, &pb, i, *fty);
        let e = compare_word(cg, &fty_ty, &va, &vb)?;
        eq = cg.emit_reg(format!("and i1 {eq}, {e}"));
    }
    Ok(eq)
}

/// One slot loaded at its own LLVM type and erased into the word ABI the
/// comparisons take — a round trip that preserves every representation.
fn slot_word(
    cg: &mut Codegen,
    owner: &str,
    struct_ty: &str,
    p: &str,
    i: usize,
    fty: LType,
) -> String {
    let loaded = crate::aggregate::load_record_field(cg, owner, struct_ty, p, i, fty);
    crate::conv::box_to_i64(cg, Value::new(loaded, fty)).operand
}

/// The published Osprey type of one field, or a loud error when inference
/// left it open — a wrong guess would compare the wrong layout.
fn field_type(cg: &Codegen, record_ty: &Type, field: &str) -> Result<Type> {
    cg.prog
        .field_type(record_ty, field)
        .filter(|ty| !osprey_types::has_type_var(ty))
        .ok_or_else(|| {
            CodegenError::unsupported(format!(
                "structuralEquals cannot compare field `{field}` of `{record_ty}`: \
                 its type was left open"
            ))
        })
}
