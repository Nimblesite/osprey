//! The website's syntax grammar, made runnable as a classic `<script>`.
//! Implements [DOC-EXPORT-HTML].
//!
//! The exporter inlines the one grammar the website highlights with, rather
//! than keeping a second copy that would drift into a second colour scheme.
//! That file is an ES module, and an inline classic script cannot hold `export`
//! or `import`: one stray keyword is a `SyntaxError` that takes highlighting,
//! search, navigation and the phone menu down together, because they share the
//! script. Rewriting the first `export const` and hoping was a silent contract.
//! The module's shape is checked instead, and an export whose grammar no longer
//! has that shape fails rather than shipping pages with no working JavaScript.

/// The embedded module, byte for byte as the website ships it.
const MODULE: &str = include_str!("../../../../../website/src/js/osprey-grammar.mjs");

/// Where the module lives, for a failure that has to say what to fix.
const MODULE_PATH: &str = "website/src/js/osprey-grammar.mjs";

/// The only module statement the grammar may hold: the binding `highlight.js`
/// reads by name.
const EXPORT: &str = "export const ospreyGrammar =";

/// The grammar as a classic script, or why the module can no longer be one.
pub(super) fn script() -> Result<String, String> {
    classic(MODULE)
}

/// `module` with its single expected `export` keyword removed, and nothing else
/// touched — a comment that mentions `export const` stays as written.
fn classic(module: &str) -> Result<String, String> {
    let statements: Vec<&str> = module
        .lines()
        .map(str::trim_start)
        .filter(|line| is_module_statement(line))
        .collect();
    match statements.as_slice() {
        [only] if only.starts_with(EXPORT) => Ok(module
            .lines()
            .map(strip_export)
            .collect::<Vec<_>>()
            .join("\n")),
        found => Err(format!(
            "{MODULE_PATH} can no longer be inlined into the documentation's classic \
             script: it must hold exactly one module statement, `{EXPORT} …`, and holds \
             {found:?}"
        )),
    }
}

/// The line with a leading `export` keyword removed, where it has one.
fn strip_export(line: &str) -> String {
    let body = line.trim_start();
    let indent = line.get(..line.len().saturating_sub(body.len()));
    match (indent, body.strip_prefix("export ")) {
        (Some(indent), Some(rest)) => format!("{indent}{rest}"),
        _ => line.to_owned(),
    }
}

/// A line opening an `export` or `import` statement. A keyword has to be
/// followed by a space, `{` or `*` to count, so `exported = 1` is not one.
fn is_module_statement(line: &str) -> bool {
    ["export", "import"].iter().any(|keyword| {
        line.strip_prefix(keyword)
            .and_then(|rest| rest.chars().next())
            .is_some_and(|next| next.is_whitespace() || matches!(next, '{' | '*'))
    })
}

#[cfg(test)]
mod tests {
    use super::{classic, script, EXPORT};

    /// A website change that breaks the shape fails this test in the compiler
    /// jobs, which CI runs for this file because it is classified as code.
    #[test]
    fn the_shipped_grammar_inlines_as_a_classic_script() {
        let inlined = script();
        assert!(inlined.is_ok(), "{inlined:?}");
        let inlined = inlined.unwrap_or_default();
        assert!(inlined.contains("const ospreyGrammar ="), "{inlined}");
        assert!(
            inlined
                .lines()
                .map(str::trim_start)
                .all(|line| !line.starts_with("export ") && !line.starts_with("import ")),
            "{inlined}"
        );
    }

    #[test]
    fn a_module_the_classic_script_cannot_hold_is_refused() {
        for module in [
            "export const ospreyGrammar = {};\nexport const extra = 1;\n",
            "import { tokens } from './tokens.mjs';\nexport const ospreyGrammar = tokens;\n",
            "export const renamedGrammar = {};\n",
            "export default {};\n",
            "const ospreyGrammar = {};\n",
            "export { ospreyGrammar };\nconst ospreyGrammar = {};\n",
        ] {
            let refused = classic(module);
            assert!(
                refused
                    .as_ref()
                    .is_err_and(|why| why.contains("osprey-grammar.mjs") && why.contains(EXPORT)),
                "{module} gave {refused:?}"
            );
        }
    }

    #[test]
    fn only_the_export_keyword_itself_is_removed() {
        let module = "// Rewriting `export const` in a comment would be wrong.\n  export const ospreyGrammar = {\n  exported: 1,\n};";
        assert_eq!(
            classic(module),
            Ok("// Rewriting `export const` in a comment would be wrong.\n  const ospreyGrammar = {\n  exported: 1,\n};".to_owned())
        );
    }
}
