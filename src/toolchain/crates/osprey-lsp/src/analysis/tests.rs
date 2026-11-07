use super::*;

/// Parse `src` and assert a clean parse, returning the AST.
fn parse_ok(src: &str) -> Program {
    let parsed = osprey_syntax::parse_program(src);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    parsed.program
}

/// Parse `src`, requiring a clean parse AND a clean type-check (`why` says what that proves), and return its outline JSON.
fn probe_json(src: &str, why: &str) -> String {
    let parsed = parse_ok(src);
    assert!(
        osprey_types::check_program(&parsed).is_empty(),
        "the probe must type-check, or it pins nothing: {why}"
    );
    symbols_json(&parsed)
}

/// Assert the symbol named `name` carries a doc comment mentioning `needle`
/// — the find-then-unwrap chain these cases otherwise repeat per symbol.
fn assert_doc(syms: &[SymbolInfo], name: &str, needle: &str) {
    let doc = syms
        .iter()
        .find(|s| s.name == name)
        .and_then(|s| s.doc.clone());
    assert!(
        doc.is_some_and(|d| d.contains(needle)),
        "no doc mentioning {needle:?} on {name}"
    );
}

#[test]
fn module_and_namespace_hovers_carry_both_doc_scopes() {
    // `container_sym` hardcoded `doc: None`, so hovering a module or a
    // namespace showed no documentation at all — not even the `///` a
    // caller wrote above it, which [DOC-ATTACH] lists as a documented
    // declaration form and [LSP-HOVER-DOCS] requires hovers to render.
    // The inner `//!` belongs there too: it is what a maintainer reading
    // the body wrote about that same scope ([DOC-SIGIL-INNER]).
    let syms = collect_all_symbols(&parse_ok(
        "/// What callers need to know.\nnamespace billing {\n\
             //! What a maintainer needs to know.\n/// The module from outside.\n\
             module Tax {\n//! The module from inside.\nexport let rate = 10\n}\n}\n",
    ));
    assert_doc(&syms, "billing", "What callers need to know.");
    assert_doc(&syms, "billing", "What a maintainer needs to know.");
    assert_doc(&syms, "billing::Tax", "The module from outside.");
    assert_doc(&syms, "billing::Tax", "The module from inside.");
}

#[test]
fn outline_covers_every_declaration_form() {
    let json = symbols_json(&parse_ok(
        "type Shade = Light | Dark\n\
             effect Log { info: fn(string) -> Unit }\n\
             extern fn puts(s: string) -> int\n\
             let limit: int = 10\n\
             fn multiply(a: int, b: int) -> int = (a * b) ?: 0\n\
             type Box<T> = { item: T }\n\
             effect Feed<out T> { next: fn() -> T }\n\
             fn pick<T, out U>(a: T, b: U) -> T = a\n\
             fn main() -> Unit = print(multiply(a: limit, b: 2))\n",
    ));
    const OUTLINE: [&str; 13] = [
        "\"name\":\"Shade\",\"kind\":\"type\",\"type\":\"type\",\"line\":1,\"column\":1",
        "\"name\":\"Log\",\"kind\":\"type\",\"type\":\"effect\",\"line\":2",
        "\"name\":\"puts\",\"kind\":\"function\"",
        "\"signature\":\"fn puts(s: string) -> int\"",
        "\"name\":\"limit\",\"kind\":\"variable\",\"type\":\"int\",\"line\":4",
        "\"name\":\"multiply\",\"kind\":\"function\"",
        "\"signature\":\"fn multiply(a: int, b: int) -> int\"",
        "\"parameters\":[{\"name\":\"a\",\"type\":\"int\"},{\"name\":\"b\",\"type\":\"int\"}]",
        "\"returnType\":\"int\"",
        // A declared type-parameter binder reaches the outline, variance
        // markers included ([TYPE-GENERICS-DECL]).
        "\"signature\":\"type Box<T>\"",
        "\"signature\":\"effect Feed<out T>\"",
        "\"signature\":\"fn pick<T, out U>(a: T, b: U) -> T\"",
        "\"name\":\"main\",\"kind\":\"function\",\"type\":\"fn main() -> Unit\",\"line\":9",
    ];
    crate::testkit::contents(&json, &OUTLINE, &[]);
}

#[test]
fn an_inferred_return_type_reaches_the_outline_instead_of_unit() {
    // RED: `--symbols` renders the DECLARED return type and falls back to
    // `Unit` when there is none, so a function whose return type is
    // inferred is reported as returning `Unit`. Every arm below yields a
    // string, and the program only compiles because the caller concatenates
    // the result — so `string` is not a guess, it is the type the checker
    // already proved.
    //
    // This is what makes the annotation rule unenforceable from tooling:
    // CLAUDE.md requires deleting an inferable `-> string`, and doing so
    // silently downgrades what `--symbols` reports to `Unit`. Hover renders
    // `string` correctly from the same source, so the type IS available;
    // `symbols_json` takes only the AST and never consults inference.
    // [TYPE-INFERENCE]
    let inferred = r#"fn describeAny(v: any) = match v {
    { held, .. } => "held=${held}"
    _            => "other"
}
print(describeAny(42) + "!")
"#;
    // The same program with the annotation written out. It is the control:
    // identical behaviour, identical inferred type, and it reports
    // correctly today — so any difference below is the annotation's
    // presence alone, not the program.
    let annotated = inferred.replacen(") = match", ") -> string = match", 1);

    const STRING_RETURN: &str = "\"returnType\":\"string\"";
    const STRING_SIG: &str = "\"signature\":\"fn describeAny(v: any) -> string\"";

    let json = probe_json(
        inferred,
        "`+ \"!\"` is what proves the return type is string rather than a guess",
    );

    // The control reports the truth, so the type is derivable from a
    // program the checker already accepted.
    let control_json = symbols_json(&parse_ok(&annotated));
    crate::testkit::contents(&control_json, &[STRING_RETURN, STRING_SIG], &[]);

    // Hover renders the same declaration from the same AST and gets it
    // right, so the outline is not missing information — it is discarding
    // it.
    let hovered = crate::test_support::hover(
        inferred,
        "file:///inferred.osp",
        0,
        4,
        lspkit::vfs::PositionEncoding::Utf16,
    )
    .expect("hover over `describeAny`");
    crate::testkit::shows(&hovered, &["-> string"]);

    // The bug, asserted three ways so a partial fix cannot pass.
    crate::testkit::contents(&json, &[], &["\"returnType\":\"Unit\"", "-> Unit"]);
    crate::testkit::shows(
        &json,
        &[
            STRING_RETURN,
            STRING_SIG,
            "\"type\":\"fn describeAny(v: any) -> string\"",
        ],
    );

    // Dropping an inferable annotation is REQUIRED by CLAUDE.md, so the two
    // spellings must be indistinguishable to tooling.
    assert_eq!(
        json, control_json,
        "removing an inferable annotation must not change what tooling reports"
    );
}

#[test]
fn a_type_the_author_named_underscore_is_a_real_type_not_a_hole() {
    // `_` is only a RENDERING for an unsolved slot; nothing stops an author
    // declaring a type of that name. Deciding "carries no information" by
    // comparing the rendered text to `"_"` confused the two and threw away
    // a fully proven return type, reporting `fn make()`. The question is
    // structural — is this a type VARIABLE — and must be asked of the type,
    // never of its spelling.
    let src = "type _ = { x: int }\nfn make() = _ { x: 1 }\nlet m = make()\n";
    let json = probe_json(src, "the declared `_` name is the fact under test");
    assert!(
        json.contains("\"signature\":\"fn make() -> _ { x: int }\""),
        "a declared type named `_` is proven and must be reported, name and \
             row alike: {json}"
    );
    assert!(
        !json.contains("\"returnType\":\"_\""),
        "and it must not be mistaken for the hole that shares its spelling: \
             {json}"
    );
}

#[test]
fn a_partially_resolved_return_type_reports_its_proven_part_never_unit() {
    // Both arms build a `Result`, so the checker proves the payload is
    // `int` — but the ERROR side stays free, because `Error { message }`
    // unifies with whichever error type a caller supplies. Annotating this
    // body `-> Result<int, string>`, `-> Result<int, Error>` and
    // `-> Result<int, string>` all check, which is what "free" means.
    //
    // [`fill_inferred`] refuses a type holding a variable — correctly, `t6`
    // is a private name — but the fallback then claimed `-> Unit`, and the
    // checker refutes that itself: annotating `-> Unit` fails with
    // "cannot unify Unit with Result<t5, t6>". A tool must not assert what
    // the compiler rejects. `Result<int, _>` says exactly what is proven.
    let src = "fn bothArms(f) = if f { Success { value: 1 } } else { Error { message: \"e\" } }\n";
    let json = symbols_json(&parse_ok(src));
    crate::testkit::contents(&json, &["\"returnType\":\"Result<int, _>\""], &["Unit"]);
    assert!(
        !json.contains(">\"t") && !json.contains(", t6"),
        "a private inference name must never reach a reader; got {json}"
    );
}

/// The rendered hover markdown for `name` in `src`, via the real symbol
/// path (`collect_all_symbols` → `doc`).
fn doc_for(src: &str, name: &str) -> Option<String> {
    collect_all_symbols(&parse_ok(src))
        .into_iter()
        .find(|s| s.name == name)
        .and_then(|s| s.doc)
}

#[test]
fn doc_comments_reach_hover_for_every_declaration_kind() {
    // Default flavor: fn, type, effect each carry their /// doc into hover.
    let src = "/// Doubles the input.\nfn double(x) = x * 2\n\
                   /// A performance tier.\ntype Tier = Epic | Solid\n\
                   /// Emits a line.\neffect Console { emit: fn(string) -> Unit }\n";
    assert!(doc_for(src, "double").is_some_and(|d| d.contains("Doubles the input.")));
    assert!(doc_for(src, "Tier").is_some_and(|d| d.contains("A performance tier.")));
    assert!(doc_for(src, "Console").is_some_and(|d| d.contains("Emits a line.")));
}

#[path = "more_tests.rs"]
mod more_tests;
