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
) -> Option<String> {
    let word = word_under(text, line, character, enc)?;
    let flavor = flavor_of(path, text);
    let parsed = osprey_syntax::parse_program_with_flavor(text, flavor);
    // A `test` callee resolves to the built-in's generic signature, which says
    // nothing about THIS case; the case's own `///` block does. Answer with it
    // before the generic lookups ([TESTING-DOC]).
    if word == TEST_CALLEE {
        if let Some(hov) = crate::testing::test_case_hover(&parsed.program, line.saturating_add(1))
        {
            return Some(hov);
        }
    }
    let symbols = collect_all_symbols(&parsed.program);
    // A `[Symbol]` intra-doc link under the cursor resolves to the referenced
    // element's own hover — the whole dotted target (`Effect.op`), not just the
    // sub-word the cursor happens to sit on ([DOC-LINK]).
    if let Some(target) = doc_link_target(text, line, character) {
        if let Some(hov) = resolve_link(&symbols, &target, &parsed.program, flavor) {
            return Some(hov);
        }
    }
    if let Some(hov) =
        crate::effects::operation_hover(&parsed.program, text, path, line, character, enc, flavor)
    {
        return Some(hov);
    }
    match best_match(&symbols, &word, line) {
        Some(sym) => Some(symbol_hover(sym, &parsed.program, flavor)),
        None => builtin_doc(word.rsplit("::").next().unwrap_or(&word), flavor)
            .or_else(|| written_hover(&symbols, &word, line, &parsed.program, flavor))
            .or_else(|| project_hover(path, &word, flavor))
            .or_else(|| keyword_hover(&word, flavor)),
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
/// the checker: unannotated parameters and an unwritten return type.
///
/// Osprey is Hindley-Milner and the house style omits every inferable
/// annotation, so blank slots are the COMMON case, not the exception. Rendering
/// them literally showed `fn fib(n) -> Unit` — the parameter untyped and the
/// return type flatly WRONG (`Unit` was the display fallback, never a claim
/// about the function). Hover is the main way a reader recovers the types the
/// source deliberately omits, so it must answer from inference.
/// Implements [LSP-HOVER-INFERRED-SIGNATURE].
fn inferred_signature(s: &SymbolInfo, sig: &str, program: &Program) -> String {
    let complete = s.return_type.is_some() && s.parameters.iter().all(|(_, t)| !t.is_empty());
    if complete {
        return sig.to_string();
    }
    let types = osprey_types::infer_program(program);
    let inferred = types.param_types(&s.name).unwrap_or_default();
    let shown: Vec<String> = s
        .parameters
        .iter()
        .enumerate()
        .map(|(slot, (name, written))| {
            let ty = match (written.is_empty(), inferred.get(slot)) {
                (true, Some(found)) => found.to_string(),
                _ => written.clone(),
            };
            crate::analysis::render_param(&(name.clone(), ty))
        })
        .collect();
    let ret = s
        .return_type
        .clone()
        .or_else(|| types.return_type(&s.name).map(ToString::to_string))
        .unwrap_or_else(|| String::from("Unit"));
    format!("fn {}({}) -> {ret}", s.name, shown.join(", "))
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

fn inferred_parameter(program: &Program, function: &str, index: usize) -> Option<String> {
    osprey_types::infer_program(program)
        .param_types(function)?
        .get(index)
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    const U16: PositionEncoding = PositionEncoding::Utf16;
    const SRC: &str = "fn add(a: int, b: int) -> int = (a + b) ?: 0\nlet total = add(1, 2)\n";

    #[test]
    fn hover_uses_signature_for_functions_and_builtins() {
        // Function and built-in signature rendering from [LSP-HOVER].
        assert!(hover(SRC, "file:///a.osp", 1, 12, U16)
            .is_some_and(|m| m.contains("fn add(a: int, b: int) -> int")));
        assert!(hover("fn main() = print(1)\n", "file:///a.osp", 0, 13, U16)
            .is_some_and(|m| m.contains("print")));
    }

    #[test]
    fn an_ml_document_is_answered_in_the_ml_flavor_end_to_end() {
        // [LSP-FLAVOR-RENDER]
        // [FLAVOR-BOUNDARY] erases the authoring surface at the AST, so every
        // editor answer used to come back in Default spelling: an ML author
        // hovering `inc` read `fn inc(x: int) -> int` — syntax their frontend
        // rejects — inside an `osprey`-fenced block the ML TextMate grammar
        // does not highlight. Re-apply the flavor at the presentation edge.
        let ml = "inc : int -> int\ninc x = (x + 1) ?: 0\n";
        let hov = hover(ml, "file:///tour.ospml", 1, 0, U16).expect("hover");
        assert!(hov.contains("```osprey-ml"), "{hov}");
        assert!(hov.contains("inc : int -> int"), "{hov}");
        assert!(!hov.contains("fn inc("), "{hov}");
        // The identical program under a `.osp` path keeps the Default spelling,
        // proving the flavor — not the content — drives the rendering.
        let default_src = "fn inc(x: int) -> int = (x + 1) ?: 0\n";
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
    fn hover_on_a_documented_default_function_renders_its_docs() {
        // A `///` block above a function surfaces under its signature.
        // Implements [LSP-HOVER-DOCS]
        let src = "/// Doubles `x`.\nfn dbl(x: int) -> int = (x * 2) ?: 0\n";
        let md = hover(src, "file:///a.osp", 1, 4, U16).expect("hover over `dbl`");
        assert!(md.contains("fn dbl(x: int) -> int"), "signature: {md}");
        assert!(md.contains("Doubles `x`."), "docs: {md}");

        let src =
            include_str!("../../../tests/effects/resume/resume_outer_handler_bridge.test.osp");
        let (line, col) = decl_of(src, "resumeOuterHandlerBridgeCase()");
        let md = hover(
            src,
            "file:///resume_outer_handler_bridge.test.osp",
            line,
            col,
            U16,
        )
        .expect("hover over documented bridge regression");
        assert!(
            md.contains("This case verifies outer-handler reachability before and after resume."),
            "real function documentation: {md}"
        );
    }

    #[test]
    fn hover_on_performed_effect_operation_shows_type_and_effect_docs() {
        let src = concat!(
            "(** Records trace markers. *)\n",
            "effect Trace\n",
            "    mark : string => Unit\n",
            "traced : Unit -> Unit ! Trace\n",
            "traced () = perform Trace.mark \"one\"\n",
            "handled () =\n",
            "    handle Trace\n",
            "        mark label => resume\n",
            "    in traced ()\n",
        );
        let col = col_of(src, 4, "mark");
        let md = hover(src, "file:///trace.ospml", 4, col, U16)
            .expect("hover over performed effect operation");

        assert!(md.contains("Trace.mark"), "qualified operation: {md}");
        assert!(
            md.contains("string") && md.contains("Unit"),
            "operation type: {md}"
        );
        assert!(
            md.contains("Records trace markers."),
            "owning effect docs: {md}"
        );
        for line in [2usize, 7] {
            let col = col_of(src, line, "mark");
            let row = u32::try_from(line).expect("line fits");
            let site = hover(src, "file:///trace.ospml", row, col, U16)
                .unwrap_or_else(|| panic!("hover over effect-operation site on line {line}"));
            assert!(site.contains("Trace.mark : string => Unit"), "{site}");
            assert!(site.contains("Records trace markers."), "{site}");
        }
    }

    /// Sibling operations must hover with their OWN prose. Before
    /// [DOC-EFFECT-OP] every operation was handed the owning effect's doc, so
    /// `ask` and `tell` rendered identically and the hover said nothing about
    /// the operation actually under the cursor.
    #[test]
    fn ml_effect_operations_hover_with_their_own_docs() {
        let src = concat!(
            "(** Console conversation capability. *)\n",
            "effect Prompt\n",
            "    (** Ask the operator a question and read back their answer. *)\n",
            "    ask : string => int\n",
            "    (** Announce a result without expecting a reply. *)\n",
            "    tell : string => Unit\n",
        );
        let ask = hover(src, "file:///p.ospml", 3, col_of(src, 3, "ask"), U16)
            .expect("hover over `ask` declaration");
        let tell = hover(src, "file:///p.ospml", 5, col_of(src, 5, "tell"), U16)
            .expect("hover over `tell` declaration");

        assert!(ask.contains("Ask the operator a question"), "{ask}");
        assert!(!ask.contains("Announce a result"), "leaked sibling: {ask}");
        assert!(tell.contains("Announce a result"), "{tell}");
        assert!(!tell.contains("Ask the operator"), "leaked sibling: {tell}");
    }

    /// The Default flavor's `///` operation docs lower to the same model.
    #[test]
    fn default_effect_operations_hover_with_their_own_docs() {
        let src = concat!(
            "/// Console conversation capability.\n",
            "effect Prompt {\n",
            "    /// Ask the operator a question and read back their answer.\n",
            "    ask: fn(string) -> int\n",
            "    /// Announce a result without expecting a reply.\n",
            "    tell: fn(string) -> Unit\n",
            "}\n",
        );
        let ask = hover(src, "file:///p.osp", 3, col_of(src, 3, "ask"), U16)
            .expect("hover over `ask` declaration");
        let tell = hover(src, "file:///p.osp", 5, col_of(src, 5, "tell"), U16)
            .expect("hover over `tell` declaration");

        assert!(ask.contains("Ask the operator a question"), "{ask}");
        assert!(!ask.contains("Announce a result"), "leaked sibling: {ask}");
        assert!(tell.contains("Announce a result"), "{tell}");
    }

    /// An operation with no doc of its own still shows the owning effect's, so
    /// adding per-operation docs never makes hover worse than it was.
    #[test]
    fn undocumented_operation_falls_back_to_the_effect_doc() {
        let src = concat!(
            "(** Records trace markers. *)\n",
            "effect Trace\n",
            "    mark : string => Unit\n",
        );
        let md = hover(src, "file:///t.ospml", 2, col_of(src, 2, "mark"), U16)
            .expect("hover over undocumented operation");
        assert!(md.contains("Records trace markers."), "{md}");
    }

    #[test]
    fn ml_pipeline_hover_shows_its_native_documentation() {
        let src = include_str!("../../../tests/effects/resume/resume_lifo_audit.test.ospml");
        // The declaration is located by CONTENT, not a hard-coded line: this
        // fixture is a live regression suite that gains and loses lines.
        let (line, col) = decl_of(src, "pipeline ()");
        let md = hover(src, "file:///resume_lifo_audit.test.ospml", line, col, U16)
            .expect("hover over documented ML pipeline");

        assert!(md.contains("pipeline : Unit -> int"), "ML signature: {md}");
        assert!(
            md.contains("Perform two ordered steps and combine their supplied values."),
            "ML documentation: {md}"
        );
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
        // `helper` annotates nothing, so both slots come from the checker:
        // `n: int`, returning the `Result` that checked `+` produces
        // ([ARITH-CHECKED], [LSP-HOVER-INFERRED-SIGNATURE]).
        assert!(
            md.contains("fn helper(n: int) -> Result<int, MathError>"),
            "resolves to helper's inferred signature: {md}"
        );
        assert!(md.contains("A helper."), "shows helper's docs: {md}");
    }

    /// The annotation-free house style must still hover with real types: both
    /// the parameter and the return type come from inference, in both flavors.
    #[test]
    fn unannotated_functions_hover_with_inferred_types() {
        let osp = "fn double(n) = n * 2 ?: 0\n";
        let md = hover(osp, "file:///d.osp", 0, col_of(osp, 0, "double"), U16)
            .expect("hover over unannotated Default function");
        assert!(md.contains("fn double(n: int) -> int"), "{md}");

        let ml = "double n = n * 2 ?: 0\n";
        let md = hover(ml, "file:///d.ospml", 0, col_of(ml, 0, "double"), U16)
            .expect("hover over unannotated ML function");
        assert!(md.contains("double : int -> int"), "{md}");
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
        let annotated = hover(SRC, "file:///a.osp", 0, 33, U16).expect("hover over `a`");
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

    #[test]
    fn hover_on_a_keyword_explains_it() {
        // Pure syntactic keywords — `match`, `handle`, `in` — had no hover at
        // all, while the type/constructor tokens the highlighter colours the
        // same way (`Unit`, `Result`, `Some`) did. Every keyword must hover.
        // Implements [LSP-HOVER-KEYWORD].
        let src = "fn main() =\n\
                   handle Log { emit msg => resume msg }\n\
                   in match 1 { _ => 0 }\n";
        for (line, kw) in [(1usize, "handle"), (2, "in"), (2, "match")] {
            let col = col_of(src, line, kw);
            let row = u32::try_from(line).expect("line fits");
            let md = hover(src, "file:///a.osp", row, col, U16)
                .unwrap_or_else(|| panic!("no hover for keyword `{kw}`"));
            assert!(md.contains(kw), "hover for `{kw}` names it: {md}");
        }
    }

    #[test]
    fn hovering_a_documented_test_call_shows_that_cases_own_documentation() {
        // [TESTING-DOC] The `test` callee resolves to a built-in whose generic
        // signature says nothing about the case being declared. Hovering it
        // must answer with the `///` block written above THAT case.
        let src = "\
fn add(a, b) = a + b

/// Addition is commutative.
///
/// # Parameters
/// - left: the first addend
///
/// # Since
/// 0.3
test(\"commutes\", fn() => expect(add(1, 2), add(2, 1)))

/// Zero is the additive identity.
test(\"identity\", fn() => expect(add(5, 0), 5))
";
        let first = hover(src, "file:///suite.test.osp", 9, 1, U16).expect("hover over `test`");
        assert!(first.starts_with("**Test:** commutes"), "{first}");
        assert!(first.contains("Addition is commutative."), "{first}");
        assert!(first.contains("**Parameters**"), "{first}");
        assert!(first.contains("- `left` — the first addend"), "{first}");
        assert!(first.contains("**Since**"), "{first}");
        assert!(first.contains("0.3"), "{first}");
        // The SECOND case's hover shows the second case's docs, not the first's.
        let second = hover(src, "file:///suite.test.osp", 12, 1, U16).expect("hover over `test`");
        assert!(second.starts_with("**Test:** identity"), "{second}");
        assert!(
            second.contains("Zero is the additive identity."),
            "{second}"
        );
        assert!(
            !second.contains("Addition is commutative."),
            "no bleed between cases: {second}"
        );
    }

    #[test]
    fn hovering_an_undocumented_test_call_names_the_case() {
        // [TESTING-DOC] With no doc comment there is still something better to
        // say than the built-in's signature: which case is declared here.
        let src = "test(\"bare case\", fn() => expect(1, 1))\n";
        let md = hover(src, "file:///suite.test.osp", 0, 1, U16).expect("hover over `test`");
        assert_eq!(md, "**Test:** bare case");
    }

    #[test]
    fn hovering_an_ml_test_call_shows_its_block_documentation() {
        // [TESTING-DOC][DOC-SIGIL-ML] the ML `(** … *)` form reaches the same
        // hover through the shared doc model.
        let src = "add a b = a + b\n\n\
                   (** Addition is commutative. *)\n\
                   test \"commutes\" (\\() => check \"c\" (add 1 2) (add 2 1))\n";
        let md = hover(src, "file:///suite.test.ospml", 3, 1, U16).expect("hover over `test`");
        assert!(md.starts_with("**Test:** commutes"), "{md}");
        assert!(md.contains("Addition is commutative."), "{md}");
    }

    #[test]
    fn hovering_test_away_from_a_case_falls_back_to_the_builtin() {
        // [TESTING-DOC] the special case is line-scoped: the word `test` used
        // anywhere else still resolves through the ordinary lookup chain, so a
        // user-declared `test` binding keeps hovering as itself.
        let src = "/// A local shadow.\nlet test = 1\nprint(\"${test}\")\n";
        let md = hover(src, "file:///a.osp", 1, 5, U16).expect("hover over the binding");
        assert!(md.contains("test: int"), "{md}");
        assert!(md.contains("A local shadow."), "{md}");
        assert!(!md.contains("**Test:**"), "not a test case: {md}");
    }

    /// The 0-based (line, column) of a cursor sitting inside the declaration
    /// whose line contains `needle`. Fixtures under `tests/` are live
    /// regression suites, so anchoring on content keeps these hovers pinned to
    /// the declaration rather than to a line number that drifts.
    fn decl_of(src: &str, needle: &str) -> (u32, u32) {
        let (index, text) = src
            .lines()
            .enumerate()
            .find(|(_, text)| text.contains(needle))
            .unwrap_or_else(|| panic!("no line containing `{needle}`"));
        let at = text.find(needle).expect("needle on found line");
        let line = u32::try_from(index).expect("line fits");
        (line, u32::try_from(at).expect("column fits") + 1)
    }

    /// The 0-based column just inside the first occurrence of `needle` on
    /// 0-based `line` of `src` — a cursor position over that word.
    fn col_of(src: &str, line: usize, needle: &str) -> u32 {
        let text = src.lines().nth(line).expect("line exists");
        let at = text.find(needle).expect("needle on line");
        u32::try_from(at).expect("column fits") + 1
    }
}
