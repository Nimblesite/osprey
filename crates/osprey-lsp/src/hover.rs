//! `textDocument/hover`.
//!
//! Hover answers three different questions with one entry point: what a
//! **declaration** is (its signature and docs), what a **binding** holds (its
//! declared or inferred type), and what a **written name** means where it is
//! written — a parameter inside its own body, a type inside an annotation.
//! Every answer is respelled in the document's authoring flavor
//! ([`crate::mlrender`], [LSP-FLAVOR-RENDER]) and falls back to the project's
//! sibling files when the open buffer cannot answer ([LSP-WORKSPACE]).
//! Implements [LSP-HOVER], [LSP-HOVER-VARIABLES], [LSP-HOVER-DOCS],
//! [LSP-HOVER-WRITTEN].

use lspkit_vfs::PositionEncoding;

use osprey_ast::Program;
use osprey_syntax::Flavor;

use crate::analysis::{builtin_hover, collect_all_symbols, SymbolInfo, SymbolKind};
use crate::features::{best_match, flavor_of, nth_line, symbol_matches, word_under};
use crate::mlrender;
use crate::workspace;

/// Hover markdown for the identifier at `(line, character)`: the symbol's
/// signature, or `name: type` for a binding — inferring an unannotated `let`'s
/// type from the checker — followed by its `///` documentation. Built-ins fall
/// back to their reference docs. Implements [LSP-HOVER], [LSP-HOVER-VARIABLES],
/// [LSP-HOVER-DOCS]
#[must_use]
pub fn hover(
    text: &str,
    path: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
) -> Option<String> {
    let word = word_under(text, line, character, enc)?;
    let flavor = flavor_of(path, text);
    let parsed = osprey_syntax::parse_program_with_flavor(text, flavor);
    let symbols = collect_all_symbols(&parsed.program);
    // A `[Symbol]` intra-doc link under the cursor resolves to the referenced
    // element's own hover — the whole dotted target (`Effect.op`), not just the
    // sub-word the cursor happens to sit on ([DOC-LINK]).
    if let Some(target) = doc_link_target(text, line, character) {
        if let Some(hov) = resolve_link(&symbols, &target, &parsed.program, flavor) {
            return Some(hov);
        }
    }
    match best_match(&symbols, &word, line) {
        Some(sym) => Some(symbol_hover(sym, &parsed.program, flavor)),
        None => builtin_doc(word.rsplit("::").next().unwrap_or(&word), flavor)
            .or_else(|| written_hover(&symbols, &word, line, &parsed.program, flavor))
            .or_else(|| project_hover(path, &word, flavor)),
    }
}

/// A symbol declared in a sibling file of the same project. The open buffer is
/// searched first — a local declaration shadows an imported one — and a
/// standalone script never reaches here at all. Implements [LSP-WORKSPACE].
fn project_hover(path: &str, word: &str, flavor: Flavor) -> Option<String> {
    workspace::siblings(path).into_iter().find_map(|sibling| {
        let symbols = collect_all_symbols(&sibling.program);
        let found = symbols.iter().find(|s| symbol_matches(s, word))?;
        Some(symbol_hover(found, &sibling.program, flavor))
    })
}

/// A built-in's reference hover, re-fenced and respelled for `flavor`. The docs
/// themselves live once in `osprey_types` and stay flavor-blind — one reference,
/// two presentations. Implements [LSP-FLAVOR-RENDER].
fn builtin_doc(name: &str, flavor: Flavor) -> Option<String> {
    builtin_hover(name).map(|md| mlrender::hover_markdown(flavor, &md))
}

/// The `[Symbol]` link the cursor sits inside on `line`, if any: the bracketed
/// content when the cursor is between a `[` and its matching `]` and the
/// content is a dotted identifier (not a `[text](url)` markdown link).
/// Implements [DOC-LINK].
fn doc_link_target(text: &str, line: u32, character: u32) -> Option<String> {
    let src = nth_line(text, line)?;
    let col = usize::try_from(character).ok()?;
    let open = src.get(..col)?.rfind('[')?;
    let close_rel = src.get(open + 1..)?.find(']')?;
    let close = open + 1 + close_rel;
    if col > close {
        return None;
    }
    let inner = src.get(open + 1..close)?;
    let followed_by_paren = src.get(close + 1..).and_then(|s| s.chars().next()) == Some('(');
    let dotted = !inner.is_empty()
        && !inner.contains(char::is_whitespace)
        && inner
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':')
        && inner.chars().next().is_some_and(char::is_alphabetic);
    (dotted && !followed_by_paren).then(|| inner.to_string())
}

/// Resolve a `[Symbol]` link target to its hover: a bare name resolves to its
/// declaration or a builtin; a dotted `Effect.op` / `Type.variant` resolves to
/// the owner declaration's hover. Implements [DOC-LINK].
fn resolve_link(
    symbols: &[SymbolInfo],
    target: &str,
    program: &Program,
    flavor: Flavor,
) -> Option<String> {
    let head = target
        .split(['.', ':'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(target);
    symbols
        .iter()
        .find(|symbol| symbol_matches(symbol, head))
        .map(|s| symbol_hover(s, program, flavor))
        .or_else(|| builtin_doc(head, flavor))
}

/// Render `s` as hover markdown: a code-fenced signature/type, then its docs.
/// Both the fence language and the signature are re-spelled in the document's
/// **authoring** flavor ([`mlrender`]) — an ML author never wrote `fn f(x: int)`
/// and should not be shown it. Implements [LSP-FLAVOR-RENDER], [FLAVOR-ML-FN].
fn symbol_hover(s: &SymbolInfo, program: &Program, flavor: Flavor) -> String {
    let code = match (s.kind, &s.signature) {
        (_, Some(sig)) => sig.clone(),
        (SymbolKind::Namespace | SymbolKind::Module | SymbolKind::Signature, None) => {
            format!("{} {}", s.kind.as_str(), s.name)
        }
        (_, None) => format!("{}: {}", s.name, displayed_type(s, program)),
    };
    let code = mlrender::signature(flavor, &code);
    let mut out = format!("```{}\n{code}\n```", mlrender::fence(flavor));
    if let Some(doc) = &s.doc {
        out.push_str("\n\n");
        out.push_str(doc);
    }
    out
}

/// The type shown for a non-function symbol: its declared/category type, or —
/// for an unannotated `let` — the type the checker inferred for that binding.
/// Implements [LSP-HOVER-VARIABLES]
fn displayed_type(s: &SymbolInfo, program: &Program) -> String {
    if !s.ty.is_empty() {
        return s.ty.clone();
    }
    osprey_types::infer_program(program)
        .let_type(s.position)
        .map_or_else(String::new, ToString::to_string)
}

/// What a name means at the place it is *written*, when no declaration of it is
/// in scope: a **parameter** inside its own function's body, or a **type name**
/// inside an annotation.
///
/// Neither is a `let`, so neither is in the binding table
/// ([LSP-HOVER-VARIABLES]) — hovering either used to return nothing at all,
/// which is the most common hover in any typed body. Implements
/// [LSP-HOVER-WRITTEN].
fn written_hover(
    symbols: &[SymbolInfo],
    word: &str,
    line: u32,
    program: &Program,
    flavor: Flavor,
) -> Option<String> {
    parameter_hover(symbols, word, line, program, flavor).or_else(|| type_hover(word, flavor))
}

/// A parameter of the function whose declaration encloses `line`.
///
/// The parameter's type is its annotation when it has one, and otherwise the
/// type the checker resolved for that argument position, so an unannotated
/// parameter of `fn twice(n) = n * 2` still hovers as `n: int`.
fn parameter_hover(
    symbols: &[SymbolInfo],
    word: &str,
    line: u32,
    program: &Program,
    flavor: Flavor,
) -> Option<String> {
    let owner = enclosing_function(symbols, line)?;
    let index = owner.parameters.iter().position(|(name, _)| name == word)?;
    let (name, written) = owner.parameters.get(index)?;
    let ty = if written.is_empty() {
        inferred_parameter(program, &owner.name, index)?
    } else {
        written.clone()
    };
    Some(fenced(flavor, &format!("{name}: {ty}")))
}

/// The declared function whose body contains `line` — the nearest declaration
/// at or above the cursor. A parameter is only in scope inside its own body, so
/// resolving without this would let one function's `x` answer for another's.
fn enclosing_function(symbols: &[SymbolInfo], line: u32) -> Option<&SymbolInfo> {
    let cursor = line.saturating_add(1); // AST positions are 1-based lines.
    symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Function)
        .filter(|s| s.position.is_some_and(|p| p.line <= cursor))
        .max_by_key(|s| s.position.map_or(0, |p| p.line))
}

fn inferred_parameter(program: &Program, function: &str, index: usize) -> Option<String> {
    osprey_types::infer_program(program)
        .param_types(function)?
        .get(index)
        .map(ToString::to_string)
}

/// A type name written in an annotation. A declared type is already a symbol
/// and never reaches here; this covers the built-in constructors, which have no
/// declaration site to navigate to.
fn type_hover(word: &str, flavor: Flavor) -> Option<String> {
    BUILTIN_TYPE_DOCS
        .iter()
        .find(|(name, _)| *name == word)
        .map(|(name, summary)| format!("{}\n\n{summary}", fenced(flavor, name)))
}

/// One-line summaries for the type names no source file declares.
const BUILTIN_TYPE_DOCS: [(&str, &str); 8] = [
    (osprey_types::names::INT, "The 64-bit integer primitive."),
    (osprey_types::names::FLOAT, "The floating-point primitive."),
    (osprey_types::names::STRING, "The string primitive."),
    (osprey_types::names::BOOL, "The boolean primitive."),
    (
        osprey_types::names::UNIT,
        "The type of an expression with no meaningful value.",
    ),
    (
        osprey_types::names::RESULT,
        "`Result<ok, err>` — `Success { value }` or `Error { message }`.",
    ),
    (
        osprey_types::names::LIST,
        "`List<elem>` — a persistent list.",
    ),
    (
        osprey_types::names::MAP,
        "`Map<key, value>` — a persistent map.",
    ),
];

/// `code` in a fence labelled for `flavor`. Implements [LSP-FLAVOR-RENDER].
fn fenced(flavor: Flavor, code: &str) -> String {
    format!(
        "```{}\n{}\n```",
        mlrender::fence(flavor),
        mlrender::signature(flavor, code)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    const U16: PositionEncoding = PositionEncoding::Utf16;
    const SRC: &str = "fn add(a: int, b: int) -> int = a + b\nlet total = add(1, 2)\n";

    #[test]
    fn hover_uses_signature_for_functions_and_builtins() {
        assert!(hover(SRC, "file:///a.osp", 1, 12, U16)
            .is_some_and(|m| m.contains("fn add(a: int, b: int) -> int")));
        assert!(hover("fn main() = print(1)\n", "file:///a.osp", 0, 13, U16)
            .is_some_and(|m| m.contains("print")));
    }

    #[test]
    fn an_ml_document_is_answered_in_the_ml_flavor_end_to_end() {
        // [FLAVOR-BOUNDARY] erases the authoring surface at the AST, so every
        // editor answer used to come back in Default spelling: an ML author
        // hovering `inc` read `fn inc(x: int) -> int` — syntax their frontend
        // rejects — inside an `osprey`-fenced block the ML TextMate grammar
        // does not highlight. Re-apply the flavor at the presentation edge.
        let ml = "inc : int -> int\ninc x = x + 1\n";
        let hov = hover(ml, "file:///tour.ospml", 1, 0, U16).expect("hover");
        assert!(hov.contains("```osprey-ml"), "{hov}");
        assert!(hov.contains("inc : int -> int"), "{hov}");
        assert!(!hov.contains("fn inc("), "{hov}");
        // The identical program under a `.osp` path keeps the Default spelling,
        // proving the flavor — not the content — drives the rendering.
        let default_src = "fn inc(x: int) -> int = x + 1\n";
        let plain = hover(default_src, "file:///a.osp", 0, 3, U16).expect("hover");
        assert!(plain.contains("```osprey\n"), "{plain}");
        assert!(plain.contains("fn inc(x: int) -> int"), "{plain}");
    }

    #[test]
    fn hover_on_a_let_binding_uses_the_name_and_type_form() {
        // A `let` has no signature, so hover renders the `name: type` fallback.
        let src = "let limit: int = 10\nfn main() -> Unit = print(limit)\n";
        let md = hover(src, "file:///a.osp", 0, 5, U16).expect("hover");
        assert!(md.contains("limit: int"), "{md}");
    }

    #[test]
    fn hover_on_a_local_let_shows_inferred_type_and_docs() {
        // A `let` nested in a function block, with no type annotation, hovers
        // with the type the checker inferred for it plus its `///` docs — the
        // case the top-level-only outline used to miss entirely.
        // Implements [LSP-HOVER-VARIABLES], [LSP-HOVER-DOCS]
        let src = "fn main() -> int = {\n/// The greeting text.\nlet greeting = \"hi\"\n0\n}\n";
        let md = hover(src, "file:///a.osp", 2, 6, U16).expect("hover over the `greeting` binding");
        assert!(md.contains("greeting: string"), "inferred type: {md}");
        assert!(md.contains("The greeting text."), "docs: {md}");
    }

    #[test]
    fn hover_on_a_documented_function_renders_its_docs() {
        // A `///` block above a function surfaces under its signature.
        // Implements [LSP-HOVER-DOCS]
        let src = "/// Doubles `x`.\nfn dbl(x: int) -> int = x * 2\n";
        let md = hover(src, "file:///a.osp", 1, 4, U16).expect("hover over `dbl`");
        assert!(md.contains("fn dbl(x: int) -> int"), "signature: {md}");
        assert!(md.contains("Doubles `x`."), "docs: {md}");
    }

    #[test]
    fn hover_on_a_doc_link_resolves_to_the_referenced_element() {
        // A `[Symbol]` intra-doc link in a comment hovers to that symbol's own
        // docs ([DOC-LINK]) — here `[helper]` on the doc line of `main`.
        let src = "/// A helper.\n\
                   fn helper(n) = n + 1\n\
                   /// Calls [helper] to do the work.\n\
                   fn main() = helper(1)\n";
        let col = col_of(src, 2, "helper");
        let md = hover(src, "file:///a.osp", 2, col, U16).expect("hover over [helper]");
        assert!(
            md.contains("fn helper(n)"),
            "resolves to helper's signature: {md}"
        );
        assert!(md.contains("A helper."), "shows helper's docs: {md}");
    }

    #[test]
    fn hover_on_a_dotted_doc_link_resolves_the_owner() {
        // `[Console.emit]` resolves to the `Console` effect declaration.
        let src = "/// Emits lines.\n\
                   effect Console { emit: fn(string) -> Unit }\n\
                   /// Uses [Console.emit] to print.\n\
                   fn go() = 1\n";
        let col = col_of(src, 2, "Console");
        let md = hover(src, "file:///a.osp", 2, col, U16).expect("hover over [Console.emit]");
        assert!(
            md.contains("Console") && md.contains("Emits lines."),
            "{md}"
        );
    }

    #[test]
    fn hover_on_a_parameter_shows_its_type_inside_its_own_body() {
        // A parameter is not a `let`, so the binding table never held it and
        // hovering one — the most common hover in any typed body — returned
        // nothing at all. Implements [LSP-HOVER-WRITTEN].
        let annotated = hover(SRC, "file:///a.osp", 0, 32, U16).expect("hover over `a`");
        assert!(annotated.contains("a: int"), "{annotated}");

        // With no annotation the type still comes from the checker, which is
        // the whole point of a Hindley-Milner surface: nothing was written.
        let inferred = "fn twice(n) = n * 2\n";
        let md = hover(inferred, "file:///a.osp", 0, 14, U16).expect("hover over `n`");
        assert!(md.contains("n: int"), "{md}");

        // A parameter is in scope only inside its own function: `n` must not
        // answer from `twice` while the cursor is in a later declaration.
        let two = "fn twice(n) = n * 2\nfn other() = 1\n";
        assert!(hover(two, "file:///a.osp", 1, 13, U16).is_none());
    }

    #[test]
    fn hover_on_a_written_type_name_explains_it() {
        // Hovering the `int` in an annotation used to return nothing, because
        // no source file declares it. Implements [LSP-HOVER-WRITTEN].
        let md = hover(SRC, "file:///a.osp", 0, 11, U16).expect("hover over `int`");
        assert!(md.contains("int"), "{md}");
        assert!(md.contains("64-bit integer"), "{md}");
        // A declared type still resolves to its declaration, not to this table.
        let declared = "type Shade = Light | Dark\nfn pick(s: Shade) = s\n";
        let hovered = hover(declared, "file:///a.osp", 1, 12, U16).expect("hover over `Shade`");
        assert!(hovered.contains("Shade"), "{hovered}");
    }

    /// The 0-based column just inside the first occurrence of `needle` on
    /// 0-based `line` of `src` — a cursor position over that word.
    fn col_of(src: &str, line: usize, needle: &str) -> u32 {
        let text = src.lines().nth(line).expect("line exists");
        let at = text.find(needle).expect("needle on line");
        u32::try_from(at).expect("column fits") + 1
    }
}
