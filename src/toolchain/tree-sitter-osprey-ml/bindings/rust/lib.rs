//! Native tree-sitter grammar for Osprey's ML source flavor.
use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_osprey_ml() -> *const ();
}

/// Native parser entry point, retaining original ML source byte ranges.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_osprey_ml) };

/// Tree-sitter's native node inventory.
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

#[cfg(test)]
mod tests;
