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

mod maps;
pub(crate) use maps::{gen_map_literal, merge_handles, runtime_map_get};
use maps::{map_contains, map_get, map_merge, map_remove, map_set, map_to_list};

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
    element_word(cg, list, raw, osprey_types::names::LIST, LIST_TAG)
}

fn element_word(
    cg: &mut Codegen,
    collection: &Value,
    raw: &str,
    container: &str,
    tag: &str,
) -> Value {
    if cg
        .source_function
        .as_deref()
        .is_some_and(|name| name.ends_with("3x61746c"))
    {
        eprintln!("TRACE atl element: {collection:?}");
    }
    let inferred = collection.element_type(container);
    let descriptor = crate::types::elem_tag(&cg.prog, inferred.as_ref())
        .or_else(|| crate::llty::elem_of_tag(collection, tag));
    let mut value = crate::conv::from_word(cg, raw, descriptor.as_deref());
    value.inferred_type = inferred;
    value
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
        "listSort" => one_list_handle(cg, "osprey_list_sort", args)?,
        "listGet" => list_get(cg, args)?,
        "listContains" => list_contains(cg, args)?,
        "structuralEquals" => crate::structural_equals::structural_equals(cg, args)?,
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
        let r = cg.emit_reg(format!("icmp eq i64 {}, 0", count.operand));
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
    let (l, i) = list_get_operands(cg, args)?;
    runtime_list_get(cg, &l, &i)
}

/// `listGet`'s list handle and boxed index, evaluated left to right.
pub(crate) fn list_get_operands(cg: &mut Codegen, args: &[Expr]) -> Result<(Value, Value)> {
    let l = handle_arg(cg, args, 0)?;
    let i = boxed_arg(cg, args, 1)?;
    Ok((l, i))
}

/// The `i32` answer of the runtime predicate `gate` and the `i64` word `read`
/// returns, both called on the collection `handle` and the boxed `key`.
fn gated_read(
    cg: &mut Codegen,
    handle: &Value,
    key: &Value,
    gate: &str,
    read: &str,
) -> (String, String) {
    let args = [handle.operand.as_str(), key.operand.as_str()];
    let open = cg.call("i32", gate, "i8*, i64", &args);
    let word = cg.call("i64", read, "i8*, i64", &args);
    (open, word)
}

/// Shared runtime list read → `Result<T, _>`, gated on `osprey_list_in_bounds`.
/// Both spellings of [BUILTIN-LIST-GET] land here — `listGet(xs, i)` and the
/// `xs[i]` form once its target turns out to be a runtime handle rather than a
/// flat literal — so the two cannot drift apart.
pub(crate) fn runtime_list_get(cg: &mut Codegen, l: &Value, i: &Value) -> Result<Value> {
    let (inb, raw) = gated_read(cg, l, i, "osprey_list_in_bounds", "osprey_list_get");
    // The success payload is the ELEMENT's type, not the storage word's, so a
    // `List<float>` yields `Result<float, _>` [`LIST_TAG`].
    let value = elem_value(cg, l, &raw);
    crate::result::make_result_unless_in_bounds(cg, &inb, value, INDEX_OOB)
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
    let boxed = box_to_i64(cg, needle.clone());

    let res = cg.emit_reg("alloca i1");
    cg.emit(format!("store i1 0, i1* {res}"));

    let lp = open_list_loop(cg, &l.operand);
    let eq = element_equals(cg, &lp.elem, &needle, &boxed.operand);
    let found = cg.fresh_label();
    let cont = cg.fresh_label();
    cg.emit(format!("br i1 {eq}, label %{found}, label %{cont}"));
    cg.start_block(&found);
    cg.emit(format!("store i1 1, i1* {res}"));
    cg.emit(format!("br label %{cont}"));
    cg.start_block(&cont);
    close_list_loop(cg, &lp);

    let out = cg.emit_reg(format!("load i1, i1* {res}"));
    Ok(Value::new(out, LType::I1))
}

/// Whether one stored element word equals the needle, by the same `==` the
/// operator uses ([BOOL-EQUALITY], [BUILTIN-LIST-CONTAINS]): strings by bytes,
/// floats under [FLOAT-COMPARE] so NaN is never found, and every other decided
/// element (int, bool, payload-free variant) by its word.
fn element_equals(cg: &mut Codegen, elem: &str, needle: &Value, boxed: &str) -> String {
    let eq = cg.fresh_reg();
    match needle.ty {
        LType::Str => {
            let ep = cg.emit_reg(format!("inttoptr i64 {elem} to i8*"));
            let c = cg.call("i32", "strcmp", "i8*, i8*", &[&ep, &needle.operand]);
            cg.emit(format!("{eq} = icmp eq i32 {c}, 0"));
        }
        LType::Double => {
            let ed = cg.emit_reg(format!("bitcast i64 {elem} to double"));
            cg.emit(format!("{eq} = fcmp oeq double {ed}, {}", needle.operand));
        }
        _ => cg.emit(format!("{eq} = icmp eq i64 {elem}, {boxed}")),
    }
    eq
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
