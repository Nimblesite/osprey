//! `listSortBy(list, compare)`: a stable bottom-up merge sort emitted inline,
//! so the comparator lowers like every other list callback (beta-reduced
//! lambda, named function or closure value) rather than crossing into C. The
//! source's element words are copied into one buffer, each pass merges runs
//! of doubling width into the other, and the sorted words are pushed through a
//! list builder that dups each borrowed element, exactly as `filterList`
//! keeps the elements it reads. Implements [BUILTIN-LIST-SORT-BY].

use super::{invoke, list_arg, nth, Callback};
use crate::builder::Codegen;
use crate::conv::as_i64;
use crate::error::Result;
use crate::llty::Value;
use crate::loops::{close_range_loop, close_range_loop_by, open_range_loop};
use osprey_ast::Expr;

/// What every step of one sort shares: the source list (whose tag types the
/// elements), the comparator, the `i64**` slots of the two word buffers the
/// passes merge between (swapped after every pass, so `src` holds the latest
/// runs), the element count, and the two runs' read cursors.
struct Sort<'a> {
    list: &'a Value,
    compare: &'a Callback,
    src: String,
    dst: String,
    n: String,
    left: String,
    right: String,
}

/// One merge: the sorted runs `[lo, mid)` and `[mid, hi)`, read from `src`
/// and written to `dst`.
struct Run<'a> {
    lo: &'a str,
    mid: String,
    hi: String,
    src: String,
    dst: String,
}

/// Lower `listSortBy(list, compare)`.
pub(super) fn gen(cg: &mut Codegen, args: &[Expr]) -> Result<Value> {
    let list = list_arg(cg, args, 0)?;
    let compare = super::callback_of(cg, nth(args, 1)?)?;
    let sort = copied_words(cg, &list, &compare);
    let width = open_range_loop(cg, "1", &sort.n);
    let step = cg.emit_reg(format!("add i64 {0}, {0}", width.i));
    merge_pass(cg, &sort, &width.i, &step)?;
    swap(cg, &sort);
    close_range_loop_by(cg, &width, &width.i);
    Ok(sorted_list(cg, &sort))
}

/// Merge every adjacent pair of `width`-long runs from `src` into `dst`.
fn merge_pass(cg: &mut Codegen, sort: &Sort<'_>, width: &str, step: &str) -> Result<()> {
    let lo = open_range_loop(cg, "0", &sort.n);
    let run = Run {
        lo: &lo.i,
        mid: bounded(cg, &lo.i, width, &sort.n),
        hi: bounded(cg, &lo.i, step, &sort.n),
        src: buffer(cg, &sort.src),
        dst: buffer(cg, &sort.dst),
    };
    merge(cg, sort, &run)?;
    close_range_loop_by(cg, &lo, step);
    Ok(())
}

/// `min(base + offset, n)`.
fn bounded(cg: &mut Codegen, base: &str, offset: &str, n: &str) -> String {
    let end = cg.emit_reg(format!("add i64 {base}, {offset}"));
    let past = cg.emit_reg(format!("icmp sgt i64 {end}, {n}"));
    cg.emit_reg(format!("select i1 {past}, i64 {n}, i64 {end}"))
}

/// Merge one pair of runs, taking the left element unless the comparator
/// orders the right one strictly first: a tie keeps the original order.
fn merge(cg: &mut Codegen, sort: &Sort<'_>, run: &Run<'_>) -> Result<()> {
    cg.emit(format!("store i64 {}, i64* {}", run.lo, sort.left));
    cg.emit(format!("store i64 {}, i64* {}", run.mid, sort.right));
    let k = open_range_loop(cg, run.lo, &run.hi);
    let (take_left, take_right, next) = (cg.fresh_label(), cg.fresh_label(), cg.fresh_label());
    let i = cg.emit_reg(format!("load i64, i64* {}", sort.left));
    let j = cg.emit_reg(format!("load i64, i64* {}", sort.right));
    choose(cg, sort, run, (&i, &j), (&take_left, &take_right))?;
    take(cg, run, (&sort.left, &i), &k.i, (&take_left, &next));
    take(cg, run, (&sort.right, &j), &k.i, (&take_right, &next));
    cg.start_block(&next);
    close_range_loop(cg, &k);
    Ok(())
}

/// Branch to the side the next merged element comes from: an exhausted run
/// yields to the other, and otherwise the comparator decides.
fn choose(
    cg: &mut Codegen,
    sort: &Sort<'_>,
    run: &Run<'_>,
    (i, j): (&str, &str),
    (take_left, take_right): (&str, &str),
) -> Result<()> {
    let (check, ask) = (cg.fresh_label(), cg.fresh_label());
    let left_done = cg.emit_reg(format!("icmp sge i64 {i}, {}", run.mid));
    cg.emit(format!(
        "br i1 {left_done}, label %{take_right}, label %{check}"
    ));
    cg.start_block(&check);
    let right_done = cg.emit_reg(format!("icmp sge i64 {j}, {}", run.hi));
    cg.emit(format!(
        "br i1 {right_done}, label %{take_left}, label %{ask}"
    ));
    cg.start_block(&ask);
    let after = orders_after(cg, sort, &run.src, (i, j))?;
    cg.emit(format!(
        "br i1 {after}, label %{take_right}, label %{take_left}"
    ));
    Ok(())
}

/// Whether `compare(src[i], src[j]) > 0`, in its own ARC region so whatever
/// the comparator allocates drops before the merge moves on [GC-ARC-PERCEUS].
fn orders_after(
    cg: &mut Codegen,
    sort: &Sort<'_>,
    src: &str,
    (i, j): (&str, &str),
) -> Result<String> {
    crate::arc::push_frame(cg, crate::arc::FrameKind::Scope);
    let left_word = load_word(cg, src, i);
    let right_word = load_word(cg, src, j);
    let left = crate::collections::elem_value(cg, sort.list, &left_word);
    let right = crate::collections::elem_value(cg, sort.list, &right_word);
    let ordered = invoke(cg, sort.compare, vec![left, right])?;
    let ordered = as_i64(cg, ordered)?;
    let after = cg.emit_reg(format!("icmp sgt i64 {}, 0", ordered.operand));
    crate::arc::pop_frame(cg);
    Ok(after)
}

/// In block `label`: move `src[index]` to `dst[k]`, advance the run's cursor
/// and continue at `next`.
fn take(
    cg: &mut Codegen,
    run: &Run<'_>,
    (cursor, index): (&str, &str),
    k: &str,
    (label, next): (&str, &str),
) {
    cg.start_block(label);
    let word = load_word(cg, &run.src, index);
    store_word(cg, &run.dst, k, &word);
    let advanced = cg.emit_reg(format!("add i64 {index}, 1"));
    cg.emit(format!("store i64 {advanced}, i64* {cursor}"));
    cg.emit(format!("br label %{next}"));
}

/// The list's element words copied into a fresh buffer, beside a second one
/// of the same size to merge into.
fn copied_words<'a>(cg: &mut Codegen, list: &'a Value, compare: &'a Callback) -> Sort<'a> {
    let n = cg.call("i64", "osprey_list_length", "i8*", &[&list.operand]);
    let src = word_buffer(cg, &n);
    let dst = word_buffer(cg, &n);
    let lp = open_range_loop(cg, "0", &n);
    let word = cg.call(
        "i64",
        "osprey_list_get",
        "i8*, i64",
        &[&list.operand, &lp.i],
    );
    store_word(cg, &src, &lp.i, &word);
    close_range_loop(cg, &lp);
    Sort {
        list,
        compare,
        src: buffer_slot(cg, &src),
        dst: buffer_slot(cg, &dst),
        n,
        left: cg.hoist_slot("i64"),
        right: cg.hoist_slot("i64"),
    }
}

/// A raw block of `n` words plus one spare, which keeps an empty list's
/// buffer non-null. It holds borrowed words only: no backend ever walks it
/// for children, and it is released, uniquely owned, once the sort is built.
fn word_buffer(cg: &mut Codegen, n: &str) -> String {
    let words = cg.emit_reg(format!("add i64 {n}, 1"));
    let bytes = cg.emit_reg(format!("mul i64 {words}, 8"));
    let raw = cg.heap_alloc(&bytes);
    cg.emit_reg(format!("bitcast i8* {raw} to i64*"))
}

fn buffer_slot(cg: &mut Codegen, words: &str) -> String {
    let slot = cg.hoist_slot("i64*");
    cg.emit(format!("store i64* {words}, i64** {slot}"));
    slot
}

fn buffer(cg: &mut Codegen, slot: &str) -> String {
    cg.emit_reg(format!("load i64*, i64** {slot}"))
}

fn swap(cg: &mut Codegen, sort: &Sort<'_>) {
    let src = buffer(cg, &sort.src);
    let dst = buffer(cg, &sort.dst);
    cg.emit(format!("store i64* {dst}, i64** {}", sort.src));
    cg.emit(format!("store i64* {src}, i64** {}", sort.dst));
}

fn load_word(cg: &mut Codegen, words: &str, index: &str) -> String {
    let at = cg.emit_reg(format!("getelementptr i64, i64* {words}, i64 {index}"));
    cg.emit_reg(format!("load i64, i64* {at}"))
}

fn store_word(cg: &mut Codegen, words: &str, index: &str, word: &str) {
    let at = cg.emit_reg(format!("getelementptr i64, i64* {words}, i64 {index}"));
    cg.emit(format!("store i64 {word}, i64* {at}"));
}

/// The sorted words as a list owning its elements, tagged like the source,
/// with both scratch buffers released.
fn sorted_list(cg: &mut Codegen, sort: &Sort<'_>) -> Value {
    let managed = cg.call(
        "i32",
        "osprey_list_elem_managed",
        "i8*",
        &[&sort.list.operand],
    );
    let builder = crate::collections::list_builder_new_of(cg, &managed);
    let src = buffer(cg, &sort.src);
    let lp = open_range_loop(cg, "0", &sort.n);
    let word = load_word(cg, &src, &lp.i);
    crate::collections::list_builder_push_borrowed(cg, &builder, &word);
    close_range_loop(cg, &lp);
    release_buffers(cg, sort);
    let sealed = crate::collections::list_builder_seal(cg, &builder);
    let elem = crate::collections::tagged_elem(sort.list);
    sealed.with_owner(Some(crate::collections::list_owner(elem.as_deref())))
}

fn release_buffers(cg: &mut Codegen, sort: &Sort<'_>) {
    for slot in [&sort.src, &sort.dst] {
        let words = buffer(cg, slot);
        let raw = cg.emit_reg(format!("bitcast i64* {words} to i8*"));
        crate::arc::release_unique(cg, &raw);
    }
}
