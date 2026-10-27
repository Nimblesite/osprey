//! The aggregate halves of a comparator body: the list, map and union walks
//! ([`super`]). Records stay in the parent — a record's walk is one straight
//! line of field loads, while these three each need their own control flow.

use super::{compare_fields, compare_word, load_slot, true_slot, word_ptr, Codegen, Result, Type};
use crate::error::CodegenError;
use crate::loops::{close_range_loop, open_range_loop};

/// The early-exit block of a comparator body and the join every path
/// converges on.
pub(super) struct Exits {
    pub(super) mismatch: String,
    pub(super) join: String,
}

impl Exits {
    fn fresh(cg: &mut Codegen) -> Self {
        Self {
            mismatch: cg.fresh_label(),
            join: cg.fresh_label(),
        }
    }
}

/// A list: equal lengths, then every element in order. The loop borrows both
/// handles and reads elements through the list runtime as uniform words.
pub(super) fn compare_list(cg: &mut Codegen, elem: &Type, a: &str, b: &str) -> Result<String> {
    let (la, lb) = (word_ptr(cg, a), word_ptr(cg, b));
    let (na, same) = equal_lengths(cg, "osprey_list_length", &la, &lb);
    let res = true_slot(cg);
    let (mismatch, join, lenok) = (cg.fresh_label(), cg.fresh_label(), cg.fresh_label());
    cg.emit(format!("br i1 {same}, label %{lenok}, label %{mismatch}"));
    cg.start_block(&lenok);
    let c = open_range_loop(cg, "0", &na);
    let ea = cg.call("i64", "osprey_list_get", "i8*, i64", &[&la, &c.i]);
    let eb = cg.call("i64", "osprey_list_get", "i8*, i64", &[&lb, &c.i]);
    let eq = compare_word(cg, elem, &ea, &eb)?;
    cg.emit(format!("br i1 {eq}, label %{}, label %{mismatch}", c.incr));
    close_range_loop(cg, &c);
    zero_exit(cg, &res, &mismatch, &join);
    Ok(load_slot(cg, &res))
}

/// Both handles' lengths through `length_fn`: the left one, and whether the
/// two are equal.
fn equal_lengths(cg: &mut Codegen, length_fn: &str, x: &str, y: &str) -> (String, String) {
    let nx = cg.call("i64", length_fn, "i8*", &[x]);
    let ny = cg.call("i64", length_fn, "i8*", &[y]);
    let same = cg.emit_reg(format!("icmp eq i64 {nx}, {ny}"));
    (nx, same)
}

/// Open `block` as an exit that writes `0`, then rejoin at `join`.
fn fail_and_join(cg: &mut Codegen, block: &str, res: &str, join: &str) {
    cg.start_block(block);
    cg.emit(format!("store i1 0, i1* {res}"));
    cg.emit(format!("br label %{join}"));
    cg.start_block(join);
}

/// The one early exit of a comparator: write `0` and rejoin the main path,
/// which reaches the join still holding `1`.
fn zero_exit(cg: &mut Codegen, res: &str, mismatch: &str, join: &str) {
    cg.emit(format!("br label %{join}"));
    cg.start_block(mismatch);
    cg.emit(format!("store i1 0, i1* {res}"));
    cg.emit(format!("br label %{join}"));
    cg.start_block(join);
}

/// A map: equal lengths, then every key of the left present in the right with
/// an equal value. The cursor borrows the map and is freed on every exit.
pub(super) fn compare_map(cg: &mut Codegen, value: &Type, a: &str, b: &str) -> Result<String> {
    let (ma, mb) = (word_ptr(cg, a), word_ptr(cg, b));
    let (_, same) = equal_lengths(cg, "osprey_map_length", &ma, &mb);
    let res = true_slot(cg);
    let exits = Exits::fresh(cg);
    // The length exit skips the cursor entirely: it exists before the walk
    // opens one, so its block must not free what was never created.
    let (lenok, lenfail) = (cg.fresh_label(), cg.fresh_label());
    cg.emit(format!("br i1 {same}, label %{lenok}, label %{lenfail}"));
    cg.start_block(&lenok);
    MapWalk::open(cg, value, (&ma, &mb), &res, &exits).run(cg)?;
    fail_and_join(cg, &lenfail, &res, &exits.join);
    Ok(load_slot(cg, &res))
}

/// One map walk's pieces: the compared value type, the cursor, the right-hand
/// handle, the result slot, the loop's blocks and the body's two exits.
struct MapWalk<'a> {
    value: &'a Type,
    iter: String,
    right: &'a str,
    res: &'a str,
    cond: String,
    body: String,
    getk: String,
    done: String,
    exits: &'a Exits,
}

impl<'a> MapWalk<'a> {
    /// A walk over `maps.0`'s entries, looking each key up in `maps.1`.
    fn open(
        cg: &mut Codegen,
        value: &'a Type,
        maps: (&str, &'a str),
        res: &'a str,
        exits: &'a Exits,
    ) -> Self {
        MapWalk {
            value,
            iter: cg.call("i8*", "osprey_map_iter_new", "i8*", &[maps.0]),
            right: maps.1,
            res,
            cond: cg.fresh_label(),
            body: cg.fresh_label(),
            getk: cg.fresh_label(),
            done: cg.fresh_label(),
            exits,
        }
    }

    /// Run the walk with its key and value cursor slots.
    fn run(&self, cg: &mut Codegen) -> Result<()> {
        let (kp, vp) = (cg.hoist_slot("i64"), cg.hoist_slot("i64"));
        self.entries(cg, &kp, &vp)
    }
}

/// One map-iteration test: another entry, or the walk is over.
impl MapWalk<'_> {
    fn entries(&self, cg: &mut Codegen, kp: &str, vp: &str) -> Result<()> {
        cg.emit(format!("br label %{}", self.cond));
        cg.start_block(&self.cond);
        self.next(cg, kp, vp);
        cg.start_block(&self.body);
        let k = cg.emit_reg(format!("load i64, i64* {kp}"));
        let has = cg.call("i32", "osprey_map_contains", "i8*, i64", &[self.right, &k]);
        let present = cg.emit_reg(format!("icmp ne i32 {has}, 0"));
        cg.emit(format!(
            "br i1 {present}, label %{}, label %{}",
            self.getk, self.exits.mismatch
        ));
        self.entry(cg, vp, &k)
    }

    fn next(&self, cg: &mut Codegen, kp: &str, vp: &str) {
        let has = cg.call(
            "i32",
            "osprey_map_iter_next",
            "i8*, i64*, i64*",
            &[&self.iter, kp, vp],
        );
        let more = cg.emit_reg(format!("icmp ne i32 {has}, 0"));
        cg.emit(format!(
            "br i1 {more}, label %{}, label %{}",
            self.body, self.done
        ));
    }

    /// One compared entry, then the walk's two exits. Both free the cursor —
    /// it borrows the map, so leaking one is a leak on every backend.
    fn entry(&self, cg: &mut Codegen, vp: &str, k: &str) -> Result<()> {
        cg.start_block(&self.getk);
        let va = cg.emit_reg(format!("load i64, i64* {vp}"));
        let wb = cg.call("i64", "osprey_map_get", "i8*, i64", &[self.right, k]);
        let eq = compare_word(cg, self.value, &va, &wb)?;
        cg.emit(format!(
            "br i1 {eq}, label %{}, label %{}",
            self.cond, self.exits.mismatch
        ));
        self.free_on_exits(cg);
        Ok(())
    }

    /// Both exits free the cursor; the mismatch exit also writes `0`.
    fn free_on_exits(&self, cg: &mut Codegen) {
        for is_mismatch in [false, true] {
            let block = if is_mismatch {
                &self.exits.mismatch
            } else {
                &self.done
            };
            cg.start_block(block);
            cg.call_void("osprey_map_iter_free", "i8*", &[&self.iter]);
            if is_mismatch {
                cg.emit(format!("store i1 0, i1* {}", self.res));
            }
            cg.emit(format!("br label %{}", self.exits.join));
        }
    }
}

/// A union: the same variant, then that variant's payload fields. The block's
/// leading tag is the variant index within its union
/// ([`crate::aggregate`]).
pub(super) fn compare_union(
    cg: &mut Codegen,
    owner: &str,
    args: &[Type],
    a: &str,
    b: &str,
) -> Result<String> {
    let variants = cg.union_variants(owner).unwrap_or(&[]).to_vec();
    let res = true_slot(cg);
    let exits = Exits::fresh(cg);
    let tag = same_variant_or_exit(cg, a, b, &exits.mismatch);
    UnionArms::new(variants, args, tag, (a, b), (res, exits)).chain(cg)
}

/// Branch to `mismatch` unless both blocks hold the same variant tag; the
/// fall-through block is open on return. Returns the left tag.
fn same_variant_or_exit(cg: &mut Codegen, a: &str, b: &str, mismatch: &str) -> String {
    let (ta, tb) = (union_tag(cg, a), union_tag(cg, b));
    let same = cg.emit_reg(format!("icmp eq i64 {ta}, {tb}"));
    let first = cg.fresh_label();
    cg.emit(format!("br i1 {same}, label %{first}, label %{mismatch}"));
    cg.start_block(&first);
    ta
}

/// One union walk's pieces: the variants in tag order with the union's type
/// arguments, the tag under test, both words, the result slot and the exits.
struct UnionArms<'a> {
    variants: Vec<String>,
    args: &'a [Type],
    tag: String,
    a: &'a str,
    b: &'a str,
    res: String,
    exits: Exits,
}

/// The leading variant tag of a union block, read through the shared
/// `{ i64 tag, … }` header every variant block starts with.
fn union_tag(cg: &mut Codegen, word: &str) -> String {
    let p8 = word_ptr(cg, word);
    let p = cg.emit_reg(format!("bitcast i8* {p8} to i64*"));
    cg.emit_reg(format!("load i64, i64* {p}"))
}

impl<'a> UnionArms<'a> {
    fn new(
        variants: Vec<String>,
        args: &'a [Type],
        tag: String,
        words: (&'a str, &'a str),
        outcome: (String, Exits),
    ) -> Self {
        Self {
            variants,
            args,
            tag,
            a: words.0,
            b: words.1,
            res: outcome.0,
            exits: outcome.1,
        }
    }
}

/// Chain one `icmp eq tag, i` test per variant: a payload-bearing hit stores
/// its field comparison, a payload-free hit (or a tag past the chain — the
/// tag already matched) keeps the slot's `1`.
impl UnionArms<'_> {
    fn chain(&self, cg: &mut Codegen) -> Result<String> {
        for (i, variant) in self.variants.iter().enumerate() {
            let (hit, next) = (cg.fresh_label(), cg.fresh_label());
            let is = cg.emit_reg(format!("icmp eq i64 {}, {i}", self.tag));
            cg.emit(format!("br i1 {is}, label %{hit}, label %{next}"));
            cg.start_block(&hit);
            self.variant_hit(cg, variant)?;
            cg.emit(format!("br label %{}", self.exits.join));
            cg.start_block(&next);
        }
        cg.emit(format!("br label %{}", self.exits.join));
        zero_exit(cg, &self.res, &self.exits.mismatch, &self.exits.join);
        Ok(load_slot(cg, &self.res))
    }

    /// One variant's hit: compare its payload fields, or nothing when the
    /// variant carries none (equal tags decided it).
    fn variant_hit(&self, cg: &mut Codegen, variant: &str) -> Result<()> {
        let Some(view) = cg.ctor_layout(variant) else {
            return Ok(());
        };
        if view.fields.is_empty() {
            return Ok(());
        }
        let struct_ty = cg
            .ctor_struct_ty(variant)
            .ok_or_else(|| rejected_variant(variant))?;
        let ty = Type::con(variant.to_string(), self.args.to_vec());
        let eq = compare_fields(cg, &ty, variant, &struct_ty, self.a, self.b, &view.fields)?;
        cg.emit(format!("store i1 {eq}, i1* {}", self.res));
        Ok(())
    }
}

/// A variant with fields but no resolvable block layout: nothing may be
/// compared against a guessed layout.
fn rejected_variant(variant: &str) -> CodegenError {
    CodegenError::unsupported(format!(
        "structuralEquals cannot compare variant `{variant}`: no block layout"
    ))
}
