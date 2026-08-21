//! List<T> and Map<K,V> builtins backed by the C runtime (`osprey_list_*` /
//! `osprey_map_*` in `libfiber_runtime`, whose signatures are the contract).
//! Element values cross the boundary as a uniform `i64`; pointers are
//! `ptrtoint`-boxed. List/Map handles are `i8*` tagged with their owner so the
//! `+` operator and `toString` can tell them from records. Implements
//! [TYPE-LIST-OPS], [TYPE-MAP-OPS], [BUILTIN-LIST], and [BUILTIN-MAP].

use crate::builder::Codegen;
use crate::cast::coerce_to;
use crate::conv::box_to_i64;
use crate::error::{CodegenError, Result};
use crate::expr::gen_expr;
use crate::llty::{LType, Value};
use crate::loops::{close_list_loop, open_list_loop};
use osprey_ast::{Expr, NamedArgument};

/// The owner tag carried by runtime list / map handles.
pub(crate) const LIST_OWNER: &str = "List";
pub(crate) const MAP_OWNER: &str = "Map";

/// The `Error { message }` an out-of-range list read reports.
///
/// One constant, because [BUILTIN-LIST-GET] declares `listGet(list, index)`
/// "equivalent to `list[index]`" — two spellings of one operation, which must
/// therefore fail identically. They did not: the flat-literal index path said
/// `index out of bounds` while `listGet` said `listGet: index out of bounds`,
/// so a program that swapped one spelling for the other silently stopped
/// matching its own error text [ERR-PAYLOAD].
pub(crate) const INDEX_OOB: &str = "index out of bounds";

/// Owner-tag prefix marking a runtime list whose element type is known; the
/// suffix is the element's LLVM spelling (`List#double`), the same convention
/// GPU buffers (`Gpu#double`) and flat literals (`[]double`) use.
///
/// The list runtime stores every element as a uniform `i64` word, so this tag
/// is the ONLY surviving record of what that word means. Without it
/// `listGet([1.5, 2.5], 0)` handed back IEEE-754 bits typed as an `int` — a
/// `?: 0.0` default then failed to type — and `forEachList` printed
/// `4609434218613702656` for `1.5`. [BUILTIN-LIST-GET]
pub(crate) const LIST_TAG: &str = "List#";

/// The owner tag for a runtime list holding `elem` words.
pub(crate) fn list_owner(elem: Option<&str>) -> String {
    crate::llty::elem_tagged_owner(LIST_TAG, LIST_OWNER, elem)
}

/// Owner-tag prefix marking a runtime map whose VALUE type is known; the
/// suffix is the value's LLVM spelling (`Map#i8*`), the same convention runtime
/// lists (`List#double`) and flat literals (`[]double`) use.
///
/// The map runtime stores every value as a uniform `i64` word, so this tag is
/// the ONLY surviving record of what that word means. Without it `mapGet` typed
/// every value `int`: a `Map<string, string>` answered a `char*` as an integer
/// and `?: "none"` printed `0`, and a `float` value came back as its IEEE-754
/// bits. [BUILTIN-MAP-GET]
pub(crate) const MAP_TAG: &str = "Map#";

/// The owner tag for a runtime map holding `elem` value words.
pub(crate) fn map_owner(elem: Option<&str>) -> String {
    crate::llty::elem_tagged_owner(MAP_TAG, MAP_OWNER, elem)
}

/// Whether an owner tag names a runtime map handle — value-typed (`Map#i8*`)
/// or bare.
pub(crate) fn is_map_owner(owner: &str) -> bool {
    owner == MAP_OWNER || owner.starts_with(MAP_TAG)
}

/// The value descriptor recorded on a map handle's owner tag, if any.
pub(crate) fn tagged_map_elem(v: &Value) -> Option<String> {
    crate::llty::elem_of_tag(v, MAP_TAG)
}

/// Whether an owner tag names a runtime list handle — element-typed
/// (`List#double`) or bare.
pub(crate) fn is_list_owner(owner: &str) -> bool {
    owner == LIST_OWNER || owner.starts_with(LIST_TAG)
}

/// The element descriptor recorded on a list handle's owner tag, if any.
pub(crate) fn tagged_elem(v: &Value) -> Option<String> {
    crate::llty::elem_of_tag(v, LIST_TAG)
}

/// One element word read out of `list`, recovered at its tagged element type —
/// what every consumer (a callback, a `?:` default, a `match` arm) must see
/// instead of the storage word [`LIST_TAG`].
pub(crate) fn elem_value(cg: &mut Codegen, list: &Value, raw: &str) -> Value {
    crate::conv::from_word(cg, raw, tagged_elem(list).as_deref())
}

/// A fresh list handle tagged with the element type of the list it derives
/// from (`listReverse`, `filterList`), or with the element it stores
/// (`listAppend`).
fn derived_list(cg: &mut Codegen, operand: String, elem: Option<&str>) -> Value {
    own_handle(cg, Value::handle(operand, list_owner(elem)))
}

/// Dispatch a collection builtin by name, or `None` if `name` is not one.
pub(crate) fn gen(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    _named: &[NamedArgument],
) -> Result<Option<Value>> {
    let v = match name {
        "List" => list_empty(cg),
        "listLength" => one_list_i64(cg, "osprey_list_length", args)?,
        "listAppend" => list_insert(cg, "osprey_list_append_of", args)?,
        "listPrepend" => list_insert(cg, "osprey_list_prepend_of", args)?,
        "listConcat" => list_concat(cg, args)?,
        "listReverse" => one_list_handle(cg, "osprey_list_reverse", args)?,
        "listGet" => list_get(cg, args)?,
        "listContains" => list_contains(cg, args)?,
        "Map" => map_empty(cg),
        "mapLength" => one_list_i64(cg, "osprey_map_length", args)?,
        "mapSet" => map_set(cg, args)?,
        "mapGet" => map_get(cg, args)?,
        "mapContains" => map_contains(cg, args)?,
        "mapRemove" => map_remove(cg, args)?,
        "mapMerge" => map_merge(cg, args)?,
        "mapKeys" => map_to_list(cg, args, true)?,
        "mapValues" => map_to_list(cg, args, false)?,
        _ => return Ok(None),
    };
    Ok(Some(v))
}

/// The bare spec spellings whose meaning is fixed by the **receiver**, not by
/// the name: [BUILTIN-COLLECTION-LENGTH] and [BUILTIN-COLLECTION-ISEMPTY] give
/// `length` and `isEmpty` one spelling across `string`, `List<T>` and
/// `Map<K, V>`.
const RECEIVER_DIRECTED: [&str; 2] = ["length", "isEmpty"];

/// Dispatch a receiver-directed bare builtin, or `None` if `name` is not one.
///
/// The receiver is lowered EXACTLY ONCE and handed to the chosen runtime —
/// re-lowering it per candidate would duplicate its side effects. The
/// collection arms exist because sending a `List`/`Map` handle to the string
/// runtime reads an `i8*` heap pointer as a NUL-terminated string: a wrong
/// answer and an out-of-bounds read. A flat list *literal* is a third layout
/// and needs its own arm ([`crate::listlit::lit_length`]) — it is not an
/// `OspreyList` handle, so neither collection tag matches it.
pub(crate) fn gen_receiver_directed(
    cg: &mut Codegen,
    name: &str,
    args: &[Expr],
    named: &[NamedArgument],
) -> Result<Option<Value>> {
    if !RECEIVER_DIRECTED.contains(&name) {
        return Ok(None);
    }
    let e = crate::expr::first_arg(args, named)
        .ok_or_else(|| CodegenError::invalid(format!("{name} needs one argument")))?;
    let lowered = gen_expr(cg, e)?;
    if lowered.result_inner.is_some() {
        return Err(CodegenError::invalid(format!(
            "{name} cannot consume an unhandled Result"
        )));
    }
    let recv = lowered;
    let count = match recv.osp_ty.as_deref() {
        Some(owner) if is_list_owner(owner) => handle_i64(cg, &recv, "osprey_list_length"),
        Some(o) if is_map_owner(o) => handle_i64(cg, &recv, "osprey_map_length"),
        _ => match crate::listlit::lit_length(cg, &recv) {
            Some(n) => n,
            None => return crate::strings::gen_size(cg, name, recv).map(Some),
        },
    };
    Ok(Some(if name == "isEmpty" {
        let r = cg.fresh_reg();
        cg.emit(format!("{r} = icmp eq i64 {}, 0", count.operand));
        Value::new(r, LType::I1)
    } else {
        count
    }))
}

/// `f(handle) -> i64` over an already-lowered collection handle.
fn handle_i64(cg: &mut Codegen, recv: &Value, cname: &str) -> Value {
    Value::new(cg.call("i64", cname, "i8*", &[&recv.operand]), LType::I64)
}

/// The `i`-th positional argument as an opaque `i8*` collection handle. A flat
/// list literal is rebuilt as a runtime list first — the list runtime cannot
/// read the literal layout ([`crate::listlit::to_runtime_list`]).
fn handle_arg(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<Value> {
    let e = args
        .get(i)
        .ok_or_else(|| CodegenError::invalid("collection builtin: missing argument"))?;
    let v = gen_expr(cg, e)?;
    let v = crate::listlit::to_runtime_list(cg, v);
    coerce_to(cg, v, LType::Ptr)
}

/// The `i`-th positional argument, evaluated without changing its Result
/// representation.
///
/// This is the element/key funnel, so it is where a nested flat literal
/// ESCAPES. The flat layout is a codegen-local optimization whose tag rides on
/// the value; once stored, the container is described by its TYPE alone, and
/// `List<List<int>>` promises runtime lists — a slot still holding a
/// `{ length, data }` block reaches `osprey_list_*` as an `OspreyList` and
/// segfaults ([`crate::listlit::escaping`]).
fn unboxed_arg(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<Value> {
    let e = args
        .get(i)
        .ok_or_else(|| CodegenError::invalid("collection builtin: missing argument"))?;
    let v = gen_expr(cg, e)?;
    Ok(crate::listlit::escaping(cg, v))
}

/// The `i`-th positional argument, boxed to the uniform `i64` element ABI.
fn boxed_arg(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<Value> {
    let v = unboxed_arg(cg, args, i)?;
    if v.result_inner.is_some() {
        return Err(CodegenError::invalid(
            "an index or map key cannot be an unhandled Result",
        ));
    }
    Ok(box_to_i64(cg, v))
}

/// The runtime's element-kind flag for a value about to be stored: `"1"` when
/// its inferred type is a managed pointer, `"0"` for a scalar. The container
/// records it and walks exactly the slots it names when it dies — releasing a
/// type-blind `i64` that happened to collide with a live heap address would be
/// a use-after-free. Keyed on the same static type the dup below is.
pub(crate) fn managed_flag(v: &Value) -> &'static str {
    if v.ty.is_managed_ptr() {
        "1"
    } else {
        "0"
    }
}

/// [`boxed_arg`] for an element/key the container will STORE, paired with its
/// [`managed_flag`]. Insertion TRANSFERS the reference, so dup managed values
/// here — before the pointer is erased into the `i64` element ABI and the
/// region drop can reclaim them [GC-ARC-PERCEUS].
fn stored_boxed_arg(cg: &mut Codegen, args: &[Expr], i: usize) -> Result<(Value, &'static str)> {
    let v = unboxed_arg(cg, args, i)?;
    stored_element(cg, v)
}

/// [`stored_boxed_arg`] for an element already lowered — the insert path reads
/// the element's type before it is erased into the `i64` ABI.
fn stored_element(cg: &mut Codegen, v: Value) -> Result<(Value, &'static str)> {
    if v.result_inner.is_some() {
        return Err(CodegenError::unsupported(
            "Result-valued collection elements are not yet supported; handle the Result before storing it",
        ));
    }
    let flag = managed_flag(&v);
    crate::arc::escape_retain(cg, &v);
    Ok((box_to_i64(cg, v), flag))
}

/// Own a fresh runtime container handle: every `osprey_list_*`/`osprey_map_*`
/// producer returns +1 (fresh allocations; alias returns retain-on-return and
/// the empty singletons are immortal (`memory_arc.c`, [GC-ARC-PERCEUS]).
fn own_handle(cg: &mut Codegen, v: Value) -> Value {
    crate::arc::own(cg, &v);
    v
}

fn list_empty(cg: &mut Codegen) -> Value {
    let v = Value::handle(cg.call("i8*", "osprey_list_empty", "", &[]), LIST_OWNER);
    own_handle(cg, v)
}

fn map_empty(cg: &mut Codegen) -> Value {
    // OSPREY_KEY_STRING = 1 (Map() defaults to string keys).
    let v = Value::handle(cg.call("i8*", "osprey_map_empty", "i32", &["1"]), MAP_OWNER);
    own_handle(cg, v)
}

/// `f(handle) -> int`.
fn one_list_i64(cg: &mut Codegen, cname: &str, args: &[Expr]) -> Result<Value> {
    let h = handle_arg(cg, args, 0)?;
    let r = cg.call("i64", cname, "i8*", &[&h.operand]);
    Ok(Value::new(r, LType::I64))
}

/// `f(handle) -> handle`. The result holds the source's own elements, so it
/// keeps the source's element tag.
fn one_list_handle(cg: &mut Codegen, cname: &str, args: &[Expr]) -> Result<Value> {
    let h = handle_arg(cg, args, 0)?;
    let r = cg.call("i8*", cname, "i8*", &[&h.operand]);
    Ok(derived_list(cg, r, tagged_elem(&h).as_deref()))
}

/// `f(handle, element, elem_managed) -> handle` — `listAppend` / `listPrepend`.
/// The element is STORED, so it is dup'd and the new list records its kind.
/// The element's own type tags the result, so `listAppend(List(), 1.5)` yields
/// a `List<float>` that reads back as one.
fn list_insert(cg: &mut Codegen, cname: &str, args: &[Expr]) -> Result<Value> {
    let h = handle_arg(cg, args, 0)?;
    let x = unboxed_arg(cg, args, 1)?;
    let elem = tagged_elem(&h).or_else(|| crate::llty::elem_spelling(&x));
    let (x, managed) = stored_element(cg, x)?;
    let r = cg.call(
        "i8*",
        cname,
        "i8*, i64, i32",
        &[&h.operand, &x.operand, managed],
    );
    Ok(derived_list(cg, r, elem.as_deref()))
}

/// A binary runtime op on two collection-handle arguments → a new handle
/// (`listConcat`, `mapMerge`): evaluate both, then [`combine_handles`].
/// A runtime op combining two collection handles into a new one — the body
/// behind both list concat and map merge.
fn combine_handles(cg: &mut Codegen, a: &Value, b: &Value, cname: &str, owner: &str) -> Value {
    let r = cg.call("i8*", cname, "i8*, i8*", &[&a.operand, &b.operand]);
    let v = Value::handle(r, owner);
    own_handle(cg, v)
}

/// `listConcat(a, b)` — the two lists' elements in one list.
fn list_concat(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let a = handle_arg(cg, args, 0)?;
    let b = handle_arg(cg, args, 1)?;
    Ok(concat_handles(cg, &a, &b))
}

/// Emit `osprey_list_concat` on two already-evaluated list handles. Both hold
/// the same element type, so either operand's tag types the result.
pub(crate) fn concat_handles(cg: &mut Codegen, a: &Value, b: &Value) -> Value {
    let owner = list_owner(tagged_elem(a).or_else(|| tagged_elem(b)).as_deref());
    combine_handles(cg, a, b, "osprey_list_concat", &owner)
}

/// `listGet(l, i) -> Result<T, _>` gated on `osprey_list_in_bounds`.
fn list_get(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let l = handle_arg(cg, args, 0)?;
    let i = boxed_arg(cg, args, 1)?;
    runtime_list_get(cg, &l, &i)
}

/// Shared runtime list read → `Result<T, _>`, gated on `osprey_list_in_bounds`.
/// Both spellings of [BUILTIN-LIST-GET] land here — `listGet(xs, i)` and the
/// `xs[i]` form once its target turns out to be a runtime handle rather than a
/// flat literal — so the two cannot drift apart.
pub(crate) fn runtime_list_get(cg: &mut Codegen, l: &Value, i: &Value) -> Result<Value> {
    let inb = cg.call(
        "i32",
        "osprey_list_in_bounds",
        "i8*, i64",
        &[&l.operand, &i.operand],
    );
    let raw = cg.call(
        "i64",
        "osprey_list_get",
        "i8*, i64",
        &[&l.operand, &i.operand],
    );
    // The success payload is the ELEMENT's type, not the storage word's, so a
    // `List<float>` yields `Result<float, _>` [`LIST_TAG`].
    let value = elem_value(cg, l, &raw);
    let inner = value.ty;
    let is_err = cg.emit_reg(format!("icmp eq i32 {inb}, 0"));
    crate::result::make_result_if_err(cg, value, inner, &is_err, Some(INDEX_OOB))
}

/// `listContains(l, x) -> bool`: linear scan, content-equality for strings.
fn list_contains(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let l = handle_arg(cg, args, 0)?;
    let needle_e = args
        .get(1)
        .ok_or_else(|| CodegenError::invalid("listContains: missing argument"))?;
    let needle = gen_expr(cg, needle_e)?;
    if needle.result_inner.is_some() {
        return Err(CodegenError::unsupported(
            "listContains cannot compare an unhandled Result element",
        ));
    }
    let is_str = needle.ty == LType::Str;
    let boxed = box_to_i64(cg, needle.clone());

    let res = cg.fresh_reg();
    cg.emit(format!("{res} = alloca i1"));
    cg.emit(format!("store i1 0, i1* {res}"));

    let lp = open_list_loop(cg, &l.operand);
    let eq = cg.fresh_reg();
    if is_str {
        let ep = cg.fresh_reg();
        cg.emit(format!("{ep} = inttoptr i64 {} to i8*", lp.elem));
        let c = cg.call("i32", "strcmp", "i8*, i8*", &[&ep, &needle.operand]);
        cg.emit(format!("{eq} = icmp eq i32 {c}, 0"));
    } else {
        cg.emit(format!("{eq} = icmp eq i64 {}, {}", lp.elem, boxed.operand));
    }
    let found = cg.fresh_label();
    let cont = cg.fresh_label();
    cg.emit(format!("br i1 {eq}, label %{found}, label %{cont}"));
    cg.start_block(&found);
    cg.emit(format!("store i1 1, i1* {res}"));
    cg.emit(format!("br label %{cont}"));
    cg.start_block(&cont);
    close_list_loop(cg, &lp);

    let out = cg.fresh_reg();
    cg.emit(format!("{out} = load i1, i1* {res}"));
    Ok(Value::new(out, LType::I1))
}

/// `mapSet(m, k, v) -> Map`. Both key and value are STORED; the key's kind is
/// already carried by the map's `OspreyKeyType`, so only the value's flag
/// crosses.
fn map_set(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let (k, _) = stored_boxed_arg(cg, args, 1)?;
    // Read the value's type BEFORE the `i64` erasure: it is what the new
    // handle's tag records, and the only place it is still visible.
    let raw = unboxed_arg(cg, args, 2)?;
    let elem = crate::llty::elem_spelling(&raw).or_else(|| tagged_map_elem(&m));
    let (v, managed) = stored_element(cg, raw)?;
    let r = cg.call(
        "i8*",
        "osprey_map_set_of",
        "i8*, i64, i64, i32",
        &[&m.operand, &k.operand, &v.operand, managed],
    );
    Ok(own_handle(cg, Value::handle(r, map_owner(elem.as_deref()))))
}

/// `mapRemove(m, k) -> Map`.
fn map_remove(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let k = boxed_arg(cg, args, 1)?;
    let r = cg.call(
        "i8*",
        "osprey_map_remove",
        "i8*, i64",
        &[&m.operand, &k.operand],
    );
    Ok(own_handle(
        cg,
        Value::handle(r, map_owner(tagged_map_elem(&m).as_deref())),
    ))
}

/// `mapContains(m, k) -> bool`.
fn map_contains(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let k = boxed_arg(cg, args, 1)?;
    let raw = cg.call(
        "i32",
        "osprey_map_contains",
        "i8*, i64",
        &[&m.operand, &k.operand],
    );
    let r = cg.fresh_reg();
    cg.emit(format!("{r} = icmp ne i32 {raw}, 0"));
    Ok(Value::new(r, LType::I1))
}

/// `mapGet(m, k) -> Result<V, _>` gated on `osprey_map_contains`.
/// Implements [TYPE-MAP-LOOKUP] and [BUILTIN-MAP-GET].
fn map_get(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    let k = boxed_arg(cg, args, 1)?;
    runtime_map_get(cg, &m, &k)
}

/// `mapMerge(a, b) -> Map`, carrying whichever operand still knows the value
/// type — an empty `Map()` operand carries none.
fn map_merge(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let a = handle_arg(cg, args, 0)?;
    let b = handle_arg(cg, args, 1)?;
    Ok(merge_handles(cg, &a, &b))
}

/// Emit `osprey_map_merge` on two already-evaluated map handles.
pub(crate) fn merge_handles(cg: &mut Codegen, a: &Value, b: &Value) -> Value {
    let owner = map_owner(tagged_map_elem(a).or_else(|| tagged_map_elem(b)).as_deref());
    combine_handles(cg, a, b, "osprey_map_merge", &owner)
}

/// Runtime list-builder protocol — `new` → `push`* → `seal`, shared by every
/// list-producing builtin (`mapList`/`filterList`, `mapKeys`/`mapValues`).
pub(crate) fn list_builder_new(cg: &mut Codegen) -> String {
    cg.call("i8*", "osprey_list_builder_new", "", &[])
}

/// A builder whose element kind is known up front — `managed` is either a
/// literal `"0"`/`"1"` or a register holding a source container's flag.
pub(crate) fn list_builder_new_of(cg: &mut Codegen, managed: &str) -> String {
    cg.call("i8*", "osprey_list_builder_new_of", "i32", &[managed])
}

/// Push an element the builder TAKES OVER (the caller already dup'd it),
/// latching the builder's element kind.
pub(crate) fn list_builder_push(cg: &mut Codegen, bld: &str, elem: &str, managed: &str) {
    cg.call_void(
        "osprey_list_builder_push_of",
        "i8*, i64, i32",
        &[bld, elem, managed],
    );
}

/// Push an element BORROWED out of another container. The runtime dups it per
/// the builder's own element kind — a conditional the flag only knows at run
/// time when it came from a source handle. [GC-ARC-PERCEUS]
pub(crate) fn list_builder_push_borrowed(cg: &mut Codegen, bld: &str, elem: &str) {
    cg.call_void(
        "osprey_list_builder_push_borrowed",
        "i8*, i64",
        &[bld, elem],
    );
}

pub(crate) fn list_builder_seal(cg: &mut Codegen, bld: &str) -> Value {
    let v = Value::handle(
        cg.call("i8*", "osprey_list_builder_seal", "i8*", &[bld]),
        LIST_OWNER,
    );
    own_handle(cg, v)
}

/// `mapKeys`/`mapValues` → a `List` built by iterating the map.
fn map_to_list(cg: &mut Codegen, args: &[Expr], take_key: bool) -> Result<Value> {
    let m = handle_arg(cg, args, 0)?;
    // The produced list OWNS its elements while the map still holds its own,
    // so the builder inherits the map's element kind and dups every entry it
    // copies out — without that the temporary list's death would free the
    // map's keys/values [GC-ARC-PERCEUS].
    let kind = if take_key {
        "osprey_map_key_managed"
    } else {
        "osprey_map_value_managed"
    };
    let managed = cg.call("i32", kind, "i8*", &[&m.operand]);
    let bld = list_builder_new_of(cg, &managed);
    let iter = cg.call("i8*", "osprey_map_iter_new", "i8*", &[&m.operand]);
    let kp = cg.fresh_reg();
    cg.emit(format!("{kp} = alloca i64"));
    let vp = cg.fresh_reg();
    cg.emit(format!("{vp} = alloca i64"));

    let cond = cg.fresh_label();
    let body = cg.fresh_label();
    let endl = cg.fresh_label();
    cg.emit(format!("br label %{cond}"));

    cg.start_block(&cond);
    let has = cg.call(
        "i32",
        "osprey_map_iter_next",
        "i8*, i64*, i64*",
        &[&iter, &kp, &vp],
    );
    let more = cg.fresh_reg();
    cg.emit(format!("{more} = icmp ne i32 {has}, 0"));
    cg.emit(format!("br i1 {more}, label %{body}, label %{endl}"));

    cg.start_block(&body);
    let slot = if take_key { &kp } else { &vp };
    let elem = cg.fresh_reg();
    cg.emit(format!("{elem} = load i64, i64* {slot}"));
    list_builder_push_borrowed(cg, &bld, &elem);
    cg.emit(format!("br label %{cond}"));

    cg.start_block(&endl);
    // The cursor borrows the map and is dead once the walk ends; without this
    // every mapKeys/mapValues call leaks one OspreyMapIter on every backend.
    cg.call_void("osprey_map_iter_free", "i8*", &[&iter]);
    // Keys are the map's own string keys; values are whatever its tag records.
    // The produced list must say so, or its elements read back as raw words
    // [`LIST_TAG`].
    let elem = if take_key {
        Some(LType::Str.as_str().to_string())
    } else {
        tagged_map_elem(&m)
    };
    Ok(list_builder_seal(cg, &bld).with_owner(Some(list_owner(elem.as_deref()))))
}

/// `{ k: v, … }` — build a runtime string-key map [TYPE-MAP-LITERAL].
pub(crate) fn gen_map_literal(cg: &mut Codegen, entries: &[osprey_ast::MapEntry]) -> Result<Value> {
    // OSPREY_KEY_STRING = 1.
    let bld = cg.call("i8*", "osprey_map_builder_new", "i32", &["1"]);
    // The sealed handle's tag records the entries' value type; an empty
    // literal has none to record [`MAP_TAG`].
    let mut elem: Option<String> = None;
    for e in entries {
        let k = gen_expr(cg, &e.key)?;
        let k = coerce_to(cg, k, LType::Str)?;
        // The map stores both key and value: dup before the i64 erasure, and
        // hand the value's kind over so the sealed map walks it on death
        // (see stored_boxed_arg) [GC-ARC-PERCEUS].
        crate::arc::escape_retain(cg, &k);
        let k = box_to_i64(cg, k);
        // Stored, so a nested literal escapes into its slot for the same
        // reason it does in [`unboxed_arg`].
        let raw = gen_expr(cg, &e.value)?;
        let v = crate::listlit::escaping(cg, raw);
        if v.result_inner.is_some() {
            return Err(CodegenError::unsupported(
                "Result-valued map entries are not yet supported; handle the Result before storing it",
            ));
        }
        let managed = managed_flag(&v);
        crate::arc::escape_retain(cg, &v);
        elem = crate::llty::elem_spelling(&v);
        let v = box_to_i64(cg, v);
        cg.call_void(
            "osprey_map_builder_put_of",
            "i8*, i64, i64, i32",
            &[&bld, &k.operand, &v.operand, managed],
        );
    }
    let sealed = cg.call("i8*", "osprey_map_builder_seal", "i8*", &[&bld]);
    Ok(own_handle(
        cg,
        Value::handle(sealed, map_owner(elem.as_deref())),
    ))
}

/// Shared runtime map lookup → `Result<V, _>` (also used by `m[key]` indexing).
pub(crate) fn runtime_map_get(cg: &mut Codegen, m: &Value, k: &Value) -> Result<Value> {
    let has = cg.call(
        "i32",
        "osprey_map_contains",
        "i8*, i64",
        &[&m.operand, &k.operand],
    );
    let got = cg.call(
        "i64",
        "osprey_map_get",
        "i8*, i64",
        &[&m.operand, &k.operand],
    );
    // The success payload is the VALUE's type, not the storage word's, so a
    // `Map<string, string>` yields `Result<string, _>` [`MAP_TAG`].
    let value = crate::conv::from_word(cg, &got, tagged_map_elem(m).as_deref());
    let inner = value.ty;
    let is_err = cg.emit_reg(format!("icmp eq i32 {has}, 0"));
    crate::result::make_result_if_err(cg, value, inner, &is_err, Some("mapGet: key not found"))
}
