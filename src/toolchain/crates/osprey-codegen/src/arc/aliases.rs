//! One value under two registers [GC-ARC-PERCEUS]. A pointer that travels
//! through a machine word and back, as it does on its way into an erased box
//! or a collection slot, comes out in a new register. The ledger knows an
//! owner by the register it was made in, so it follows these back to it.
use super::Codegen;

/// `new` holds the value `old` holds.
pub(crate) fn same_value(cg: &mut Codegen, new: &str, old: &str) {
    let first = root(cg, old).to_owned();
    let _ = cg.arc.aliases.insert(new.to_owned(), first);
}

/// `new` is the pointer read back out of the word `word`: the same value as
/// the pointer that went in, when the ledger saw it go in.
pub(crate) fn same_as_word(cg: &mut Codegen, new: &str, word: &str) {
    if cg.arc.aliases.contains_key(word) {
        same_value(cg, new, word);
    }
}

/// The register `operand`'s value was first made in.
pub(super) fn root<'a>(cg: &'a Codegen, operand: &'a str) -> &'a str {
    cg.arc.aliases.get(operand).map_or(operand, String::as_str)
}

/// A new function begins: its registers are numbered from zero again.
pub(crate) fn forget(cg: &mut Codegen) {
    cg.arc.aliases.clear();
}
