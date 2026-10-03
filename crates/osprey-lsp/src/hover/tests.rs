use super::*;
use crate::test_support::hover;
use crate::test_support::{col_of, ADD_SRC as SRC};
use crate::testkit::shows;
const U16: PositionEncoding = PositionEncoding::Utf16;

#[test]
fn hovering_a_parameter_in_the_body_holes_it_exactly_as_the_declaration_does() {
    // The declaration and the body are two views of ONE type, so they must
    // spell it the same way. `inferred_parameter` and `displayed_type`
    // rendered with `ToString` while the declaration path went through
    // `render_with_holes`, so hovering `xs` on line 0 showed `List<_>` and
    // hovering the same `xs` on line 2 showed `List<t5>` — the artefact
    // [TYPE-RENDER-HOLES] exists to keep away from a reader, reachable by
    // moving the cursor four lines.
    let src = "fn classify(xs) = match xs {\n  [] => 0\n  [head, ...tail] => listLength(xs)\n}\nlet e = classify([1])\n";
    let in_body = hover(src, "file:///c.osp", 2, 32, U16).expect("hover over `xs` in the body");
    assert!(
        !in_body.contains("t5") && !in_body.contains("<t"),
        "a body hover must not leak an inference artefact: {in_body}"
    );
    assert!(
        in_body.contains("List<_>"),
        "a body hover holes the unsolved element exactly as the declaration does: {in_body}"
    );
}

#[test]
fn an_inferred_signature_fills_resolved_slots_and_holes_the_unsolved_ones() {
    // Implements [LSP-HOVER-INFERRED-SIGNATURE]. `r` resolves to a concrete
    // `int`, so the reader gets it.
    let concrete = hover(
        "fn area(r) = r * r\nlet a = area(5)\n",
        "file:///a.osp",
        0,
        3,
        U16,
    )
    .expect("hover over `area`");
    assert!(concrete.contains("fn area(r: int)"), "{concrete}");
    // `xs` is proven to be a LIST; only its element type stays open. The
    // choice used to be `List<t5>` or nothing, and nothing won, because
    // `t5` is an inference artefact whose number shifts when an unrelated
    // line moves. A hole is neither: it keeps everything the checker proved
    // and names nothing it did not ([`osprey_types::render_with_holes`]).
    let generic = hover(
            "fn classify(xs) = match xs {\n  [] => 0\n  [head, ...tail] => 1\n}\nlet e = classify([])\n",
            "file:///b.osp",
            0,
            3,
            U16,
        )
        .expect("hover over `classify`");
    assert!(
        generic.contains("fn classify(xs: List<_>) -> int"),
        "the proven `List` must survive with a hole for its element: {generic}"
    );
    // The artefact itself stays banned, which is what the bare rendering
    // was really protecting.
    assert!(
        !generic.contains("t5") && !generic.contains("<t"),
        "an inference artefact must never reach a reader: {generic}"
    );
}

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
    let ml = "inc : int -> int\ninc x = x + 1\n";
    let hov = hover(ml, "file:///tour.ospml", 1, 0, U16).expect("hover");
    shows(&hov, &["```osprey-ml", "inc : int -> int"]);
    assert!(!hov.contains("fn inc("), "{hov}");
    // The identical program under a `.osp` path keeps the Default spelling,
    // proving the flavor — not the content — drives the rendering.
    let default_src = "fn inc(x: int) -> int = x + 1\n";
    let plain = hover(default_src, "file:///a.osp", 0, 3, U16).expect("hover");
    shows(&plain, &["```osprey\n", "fn inc(x: int) -> int"]);
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
    shows(&md, &["greeting: string", "The greeting text."]);
}

#[test]
fn hover_on_a_documented_default_function_renders_its_docs() {
    // A `///` block above a function surfaces under its signature.
    // Implements [LSP-HOVER-DOCS]
    let src = "/// Doubles `x`.\nfn dbl(x: int) -> int = x * 2\n";
    let md = hover(src, "file:///a.osp", 1, 4, U16).expect("hover over `dbl`");
    shows(&md, &["fn dbl(x: int) -> int", "Doubles `x`."]);

    let src = include_str!("../../../../tests/effects/resume/resume_outer_handler_bridge.test.osp");
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
        "    control mark : string => Unit\n",
        "traced : Unit -> Unit ! Trace\n",
        "traced () = perform Trace.mark \"one\"\n",
        "handled () =\n",
        "    handle Trace\n",
        "        mark label => resume ()\n",
        "    traced ()\n",
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
        shows(
            &site,
            &["Trace.mark : string => Unit", "Records trace markers."],
        );
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
    let src = include_str!("../../../../tests/effects/resume/resume_lifo_audit.test.ospml");
    // The declaration is located by CONTENT, not a hard-coded line: this
    // fixture is a live regression suite that gains and loses lines.
    let (line, col) = decl_of(src, "pipeline ()");
    let md = hover(src, "file:///resume_lifo_audit.test.ospml", line, col, U16)
        .expect("hover over documented ML pipeline");

    assert!(md.contains("pipeline : Unit -> int"), "ML signature: {md}");
    assert!(
        md.contains("This helper performs two ordered steps and combines their supplied values."),
        "ML documentation: {md}"
    );
}

#[test]
fn hover_on_a_doc_link_resolves_to_the_referenced_element() {
    // A `[Symbol]` intra-doc link in a comment hovers to that symbol's own
    // docs ([DOC-LINK]) — here `[helper]` on the doc line of `main`.
    let src = "/// A helper.\n\
                   fn helper(n) = checkedAdd(n, 1)\n\
                   /// Calls [helper] to do the work.\n\
                   fn main() = helper(1)\n";
    let col = col_of(src, 2, "helper");
    let md = hover(src, "file:///a.osp", 2, col, U16).expect("hover over [helper]");
    // `helper` annotates nothing, so both slots come from the checker:
    // `n: int`, returning the `Result` that `checkedAdd` produces
    // ([ARITH-CHECKED], [LSP-HOVER-INFERRED-SIGNATURE]).
    assert!(
        md.contains("fn helper(n: int) -> Result<int, Error>"),
        "resolves to helper's inferred signature: {md}"
    );
    assert!(md.contains("A helper."), "shows helper's docs: {md}");
}

/// The annotation-free house style must still hover with real types: both
/// the parameter and the return type come from inference, in both flavors.
#[test]
fn unannotated_functions_hover_with_inferred_types() {
    let osp = "fn double(n) = n * 2\n";
    let md = hover(osp, "file:///d.osp", 0, col_of(osp, 0, "double"), U16)
        .expect("hover over unannotated Default function");
    assert!(md.contains("fn double(n: int) -> int"), "{md}");

    let ml = "double n = n * 2\n";
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
    let in_body = col_of(SRC, 0, "(a, b)");
    let annotated = hover(SRC, "file:///a.osp", 0, in_body, U16).expect("hover over `a`");
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
    shows(&md, &["int", "64-bit integer"]);
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
    // [TESTING-DOC-HOVER] The `test` callee resolves to a built-in whose
    // generic signature says nothing about the case being declared. Hovering
    // it must answer with the `///` block written above THAT case.
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
    shows(
        &first,
        &[
            "Addition is commutative.",
            "**Parameters**",
            "- `left` — the first addend",
            "**Since**",
            "0.3",
        ],
    );
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
    shows(&md, &["test: int", "A local shadow."]);
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
