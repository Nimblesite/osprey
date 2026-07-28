//! Counted-loop scaffolding shared by the iterator and collection builtins.
//! Both a list walk (by index over a runtime handle) and a range walk (over a
//! half-open `[start, end)`) are the same counter frame underneath — an `i64`
//! slot, a `cond` block testing `< bound`, and an increment/back-edge — so they
//! share one [`Counter`] core instead of hand-rolling `alloca`/`icmp`/`br` per
//! call site. Each `open_*` leaves the builder in the loop body; the matching
//! `close_*` emits the increment and back-edge.

use crate::builder::Codegen;

/// The counter frame behind every counted loop: an `i64` slot initialised to
/// `start`, a `cond` block that loads it and tests `< bound`, with the body
/// already open and the loaded index in [`Counter::i`]. [`close_counter`] emits
/// the increment + back-edge.
pub(crate) struct Counter {
    pub slot: String,
    pub i: String,
    pub cond: String,
    pub incr: String,
    pub endl: String,
}

fn open_counter(cg: &mut Codegen, start: &str, bound: &str) -> Counter {
    let slot = cg.fresh_reg();
    cg.emit(format!("{slot} = alloca i64"));
    cg.emit(format!("store i64 {start}, i64* {slot}"));
    let cond = cg.fresh_label();
    let body = cg.fresh_label();
    let incr = cg.fresh_label();
    let endl = cg.fresh_label();
    cg.emit(format!("br label %{cond}"));

    cg.start_block(&cond);
    let i = cg.fresh_reg();
    cg.emit(format!("{i} = load i64, i64* {slot}"));
    let more = cg.fresh_reg();
    cg.emit(format!("{more} = icmp slt i64 {i}, {bound}"));
    cg.emit(format!("br i1 {more}, label %{body}, label %{endl}"));

    cg.start_block(&body);
    Counter {
        slot,
        i,
        cond,
        incr,
        endl,
    }
}

fn close_counter(cg: &mut Codegen, c: &Counter) {
    cg.emit(format!("br label %{}", c.incr));
    cg.start_block(&c.incr);
    let next = cg.emit_reg(format!("add i64 {}, 1", c.i));
    cg.emit(format!("store i64 {next}, i64* {}", c.slot));
    cg.emit(format!("br label %{}", c.cond));
    cg.start_block(&c.endl);
}

/// A counted loop over a runtime list handle: the shared counter plus the
/// current element loaded as a uniform `i64` in [`ListLoop::elem`].
pub(crate) struct ListLoop {
    counter: Counter,
    pub elem: String,
}

pub(crate) fn open_list_loop(cg: &mut Codegen, l: &str) -> ListLoop {
    let len = cg.call("i64", "osprey_list_length", "i8*", &[l]);
    let counter = open_counter(cg, "0", &len);
    let elem = cg.call("i64", "osprey_list_get", "i8*, i64", &[l, &counter.i]);
    ListLoop { counter, elem }
}

pub(crate) fn close_list_loop(cg: &mut Codegen, lp: &ListLoop) {
    close_counter(cg, &lp.counter);
}

/// A counted loop over a half-open `[start, end)`, step 1 — a bare [`Counter`],
/// with the current index in [`Counter::i`].
pub(crate) fn open_range_loop(cg: &mut Codegen, start: &str, end: &str) -> Counter {
    open_counter(cg, start, end)
}

pub(crate) fn close_range_loop(cg: &mut Codegen, lp: &Counter) {
    close_counter(cg, lp);
}
