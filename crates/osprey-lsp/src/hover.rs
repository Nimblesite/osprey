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
use crate::features::{best_match, nth_line, symbol_matches, word_under};
use crate::mlrender;
use crate::reference_docs::{keyword_hover, type_hover};
use crate::workspace;

/// The built-in that declares a test case; hovering it shows that case's own
/// documentation rather than the built-in's signature ([TESTING-DOC]).
const TEST_CALLEE: &str = "test";

/// Hover markdown for the identifier at `(line, character)`: the symbol's
/// signature, or `name: type` for a binding — inferring an unannotated `let`'s
/// type from the checker — followed by its `///` documentation. Built-ins fall
/// back to their reference docs. Implements [LSP-HOVER], [LSP-HOVER-VARIABLES],
/// [LSP-HOVER-DOCS]
#[must_use]
pub(crate) fn hover(
    text: &str,
    path: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
    project: &workspace::View,
) -> Option<String> {
    let siblings = &project.siblings;
    let word = word_under(text, line, character, enc)?;
    let flavor = project.flavor(path, text);
    let program = project.program(path, text);
    // A `test` callee resolves to the built-in's generic signature, which says
    // nothing about THIS case; the case's own `///` block does. Answer with it
    // before the generic lookups. A `test` that names no case falls through, so
    // a user binding called `test` keeps its own hover. Implements
    // [TESTING-DOC-HOVER].
    if word == TEST_CALLEE {
        if let Some(hov) = crate::testing::test_case_hover(&program, line.saturating_add(1)) {
            return Some(hov);
        }
    }
    let symbols = collect_all_symbols(&program);
    // A `[Symbol]` intra-doc link under the cursor resolves to the referenced
    // element's own hover — the whole dotted target (`Effect.op`), not just the
    // sub-word the cursor happens to sit on ([DOC-LINK]).
    if let Some(target) = doc_link_target(text, line, character) {
        if let Some(hov) = resolve_link(&symbols, &target, &program, flavor) {
            return Some(hov);
        }
    }
    if let Some(hov) =
        crate::effects::operation_hover(&program, text, siblings, line, character, enc, flavor)
    {
        return Some(hov);
    }
    match best_match(&symbols, &word, line) {
        Some(sym) => Some(symbol_hover(sym, &program, flavor)),
        None => builtin_doc(word.rsplit("::").next().unwrap_or(&word), flavor)
            .or_else(|| written_hover(&symbols, &word, line, &program, flavor))
            .or_else(|| project_hover(siblings, &word, flavor))
            .or_else(|| keyword_hover(&word, flavor)),
    }
}

/// A symbol declared in a sibling file of the same project. The open buffer is
/// searched first — a local declaration shadows an imported one — and a
/// standalone script never reaches here at all. Implements [LSP-WORKSPACE].
fn project_hover(siblings: &[workspace::Sibling], word: &str, flavor: Flavor) -> Option<String> {
    siblings.iter().find_map(|sibling| {
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
        (SymbolKind::Function, Some(sig)) => inferred_signature(s, sig, program),
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

/// A function's signature with every slot the author left blank filled in by
/// the checker ([`crate::analysis::fill_inferred`], the same path `--symbols`
/// answers from, so hover and the outline cannot disagree).
///
/// Osprey is Hindley-Milner and the house style omits every inferable
/// annotation, so blank slots are the COMMON case, not the exception. Rendering
/// them literally showed `fn fib(n) -> Unit` — the parameter untyped and the
/// return type flatly WRONG (`Unit` was the display fallback, never a claim
/// about the function). Hover is the main way a reader recovers the types the
/// source deliberately omits, so it must answer from inference.
/// Implements [LSP-HOVER-INFERRED-SIGNATURE].
fn inferred_signature(s: &SymbolInfo, sig: &str, program: &Program) -> String {
    let mut filled = s.clone();
    crate::analysis::fill_inferred(&mut filled, &osprey_types::infer_program(program));
    filled.signature.unwrap_or_else(|| sig.to_string())
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
        .map_or_else(String::new, osprey_types::render_with_holes)
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
    Some(mlrender::fenced(flavor, &format!("{name}: {ty}")))
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

/// The checker's type for one parameter, rendered the way a declaration hover
/// renders it — holes and all, so the two views cannot disagree
/// ([TYPE-RENDER-HOLES]).
fn inferred_parameter(program: &Program, function: &str, index: usize) -> Option<String> {
    osprey_types::infer_program(program)
        .param_types(function)?
        .get(index)
        .map(osprey_types::render_with_holes)
}

#[cfg(test)]
mod tests;
