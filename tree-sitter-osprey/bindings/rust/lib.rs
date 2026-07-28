//! Rust bindings to the tree-sitter Osprey grammar.

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_osprey() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_osprey) };

#[cfg(test)]
mod tests {
    #[test]
    fn can_load_grammar() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&super::LANGUAGE.into())
            .expect("Error loading Osprey grammar");
        let tree = parser.parse("let x = 1\n", None).unwrap();
        assert!(!tree.root_node().has_error());
    }
}
