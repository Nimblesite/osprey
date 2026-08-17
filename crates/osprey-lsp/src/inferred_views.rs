//! Every editor view of one function, held to the same answer.
//!
//! One function, every view. CLAUDE.md REQUIRES deleting every inferable
//! annotation, so the annotated and annotation-free spellings of a program are
//! two ways of writing the same thing — and every view an editor shows must be
//! unable to tell them apart. Each view had its own path to the AST and each
//! degraded independently: the outline and `--symbols` claimed `-> Unit`,
//! signature help dropped to a bare name, completion emptied its detail, and
//! hover disagreed with itself between a declaration and the body.
//!
//! Every test below asserts, per view: the EXACT inferred spelling, that the
//! annotated control is byte-identical, that no private inference name (`t5`)
//! escapes, and that no `Unit` is fabricated for a return the checker refutes.
//! Implements [LSP-HOVER-INFERRED-SIGNATURE], [TYPE-RENDER-HOLES].

#[cfg(test)]
mod tests {
    use lspkit_vfs::PositionEncoding;

    const U16: PositionEncoding = PositionEncoding::Utf16;

    /// `twice` annotation-free, and the identical program with every inferable
    /// annotation written out. `*` is fallible, so the proven return is
    /// `Result<int, MathError>` — a type the author who deleted the annotation
    /// cannot see any other way.
    const BARE: &str = "fn twice(n) = n * 2\nlet y = twice(2)\n";
    const ANNOTATED: &str =
        "fn twice(n: int) -> Result<int, MathError> = n * 2\nlet y = twice(2)\n";
    const SIGNATURE: &str = "fn twice(n: int) -> Result<int, MathError>";

    /// Every view of `name` in `src`, as the strings a user actually sees.
    struct Views {
        hover_decl: String,
        hover_body: String,
        symbols: String,
        sig_label: String,
        sig_params: Vec<String>,
        completion_detail: String,
    }

    fn views(src: &str, uri: &str) -> Views {
        let parsed = osprey_syntax::parse_program(src);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        assert!(
            osprey_types::check_program(&parsed.program).is_empty(),
            "the probe must type-check, or it pins nothing"
        );
        // Cursor sits on `twice` in the declaration (line 0) and on the call
        // site (line 1) — the same function reached two different ways.
        let hover_decl = crate::hover::hover(src, uri, 0, 4, U16).expect("hover over declaration");
        let hover_body = crate::hover::hover(src, uri, 1, 9, U16).expect("hover over call site");
        // Column 14 is the argument slot inside `twice(2)` on `let y = twice(2)`
        // — signature help and completion both answer about the OPEN call.
        let sig = crate::features::signature_help(src, uri, 1, 14, U16).expect("signature help");
        let items = crate::complete::completion(src, uri, 1, 14, U16);
        let completion_detail = items
            .iter()
            .find(|i| i.label == "twice")
            .and_then(|i| i.detail.clone())
            .expect("`twice` is completable");
        Views {
            hover_decl,
            hover_body,
            symbols: crate::analysis::symbols_json(&parsed.program),
            sig_label: sig.label,
            sig_params: sig.parameters,
            completion_detail,
        }
    }

    impl Views {
        /// Every view as `(name, rendered)`, so a test can sweep all five
        /// without repeating itself once per view.
        fn all(&self) -> Vec<(&'static str, &str)> {
            vec![
                ("hover (declaration)", &self.hover_decl),
                ("hover (call site)", &self.hover_body),
                ("symbols", &self.symbols),
                ("signature help label", &self.sig_label),
                ("completion detail", &self.completion_detail),
            ]
        }
    }

    #[test]
    fn every_view_reports_the_exact_inferred_signature() {
        let v = views(BARE, "file:///bare.osp");
        for (view, rendered) in v.all() {
            assert!(
                rendered.contains(SIGNATURE),
                "{view} must report `{SIGNATURE}`; got {rendered}"
            );
        }
        assert_eq!(v.sig_params, vec!["n: int".to_owned()]);
        assert!(v
            .symbols
            .contains("\"returnType\":\"Result<int, MathError>\""));
        assert!(v.symbols.contains("\"type\":\"int\""), "the parameter too");
    }

    #[test]
    fn deleting_every_inferable_annotation_changes_no_view() {
        // The house style's central promise. If this fails, obeying CLAUDE.md
        // costs the author information in their editor.
        let bare = views(BARE, "file:///same.osp");
        let annotated = views(ANNOTATED, "file:///same.osp");
        assert_eq!(bare.sig_label, annotated.sig_label);
        assert_eq!(bare.sig_params, annotated.sig_params);
        assert_eq!(bare.completion_detail, annotated.completion_detail);
        assert_eq!(bare.hover_decl, annotated.hover_decl);
        assert_eq!(bare.hover_body, annotated.hover_body);
        // `symbols` carries source positions, which the two spellings do NOT
        // share — the annotated form is longer. Compare the part that describes
        // the TYPE, which must be identical.
        for fragment in [
            "\"signature\":\"fn twice(n: int) -> Result<int, MathError>\"",
            "\"returnType\":\"Result<int, MathError>\"",
        ] {
            assert!(bare.symbols.contains(fragment), "bare: {}", bare.symbols);
            assert!(
                annotated.symbols.contains(fragment),
                "annotated: {}",
                annotated.symbols
            );
        }
    }

    #[test]
    fn no_view_leaks_a_private_inference_name_or_fabricates_unit() {
        // `classify` proves `xs` is a LIST but leaves its element open, so it
        // exercises both hazards at once: the artefact `t5` and the `Unit`
        // fallback that replaced it.
        let src = "fn classify(xs) = match xs {\n  [] => 0\n  [head, ...tail] => listLength(xs)\n}\nlet e = classify([1])\n";
        let parsed = osprey_syntax::parse_program(src);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let symbols = crate::analysis::symbols_json(&parsed.program);
        let decl = crate::hover::hover(src, "file:///c.osp", 0, 4, U16).expect("hover decl");
        // `xs` USED in the body, four lines from where it is bound: the two
        // views of one parameter that disagreed (`List<_>` vs `List<t6>`).
        let body = crate::hover::hover(src, "file:///c.osp", 2, 32, U16).expect("hover body");
        // Signature help and completion are DISPLAY paths too, and a partial
        // type is exactly where they would leak — they answer from their own
        // collectors, so checking only hover and the outline would leave the
        // two newest paths untested against the hazard this test is named for.
        let sig = crate::features::signature_help(src, "file:///c.osp", 4, 18, U16)
            .expect("signature help over `classify(`");
        let completion_detail = crate::complete::completion(src, "file:///c.osp", 4, 18, U16)
            .iter()
            .find(|i| i.label == "classify")
            .and_then(|i| i.detail.clone())
            .expect("`classify` is completable");

        for (view, rendered) in [
            ("symbols", &symbols),
            ("hover decl", &decl),
            ("hover body", &body),
            ("signature help label", &sig.label),
            ("completion detail", &completion_detail),
        ] {
            assert!(
                !regex_like_type_var(rendered),
                "{view} leaked a private inference name: {rendered}"
            );
            assert!(
                !rendered.contains("Unit"),
                "{view} fabricated Unit for a return the checker proved: {rendered}"
            );
        }
        // Every view keeps the PROVEN half — `xs` is a list — and holes only
        // the element the checker genuinely left open.
        assert!(
            symbols.contains("fn classify(xs: List<_>) -> int"),
            "{symbols}"
        );
        for (view, rendered) in [
            ("hover decl", &decl),
            ("hover body", &body),
            ("signature help label", &sig.label),
            ("completion detail", &completion_detail),
        ] {
            assert!(
                rendered.contains("List<_>"),
                "{view} keeps the proven `List` with a hole for its element: {rendered}"
            );
        }
        assert_eq!(sig.parameters, vec!["xs: List<_>".to_owned()]);
    }

    #[test]
    fn a_return_the_checker_could_not_prove_gets_no_arrow_rather_than_unit() {
        // THE pin for the original defect. `id` is generic: its return is a
        // bare type variable, so there is genuinely nothing to report — and
        // this is the ONLY shape that reaches the no-return-type branch. The
        // other probes in this file all have a proven return, so none of them
        // would notice the `-> Unit` fallback coming back.
        //
        // `Unit` is not a neutral placeholder: the checker REFUTES it. Writing
        // `-> Unit` on a body like
        // `if f { Success { value: 1 } } else { Error { message: "e" } }`
        // fails with "cannot unify Unit with Result<t5, t6>", so a tool that
        // prints it asserts something the compiler rejects. Saying nothing is
        // the only honest answer once there is nothing to say.
        let src = "fn id(x) = x\nlet a = id(1)\n";
        let parsed = osprey_syntax::parse_program(src);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let symbols = crate::analysis::symbols_json(&parsed.program);
        assert!(
            symbols.contains("\"signature\":\"fn id(x)\""),
            "an unprovable return drops the arrow entirely: {symbols}"
        );
        assert!(
            !symbols.contains("Unit"),
            "`Unit` is a claim the checker refutes, not a placeholder: {symbols}"
        );
        assert!(
            !symbols.contains("\"returnType\""),
            "an absent return type is absent, not empty-stringed: {symbols}"
        );
        // Hover must agree with the outline — same slot, same silence.
        let decl = crate::hover::hover(src, "file:///id.osp", 0, 4, U16).expect("hover over `id`");
        assert!(
            !decl.contains("Unit"),
            "hover must not claim Unit either: {decl}"
        );
        assert!(!regex_like_type_var(&decl), "{decl}");
    }

    #[test]
    fn a_hole_is_stable_across_views_and_across_the_same_type_twice() {
        // A hole must not carry a number, or two renderings of one type would
        // disagree and an edit elsewhere would churn the tooltip.
        let src = "fn pair(a, b) = [a, b]\nlet p = pair(1, 2)\n";
        let parsed = osprey_syntax::parse_program(src);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let symbols = crate::analysis::symbols_json(&parsed.program);
        assert!(!regex_like_type_var(&symbols), "{symbols}");
        // Rendering the same program twice is byte-identical: nothing in the
        // spelling depends on inference-run state.
        assert_eq!(symbols, crate::analysis::symbols_json(&parsed.program));
    }

    /// Whether `s` mentions an inference name (`t5`, `t42`): a `t` that starts
    /// an identifier and is followed by a digit. Anchoring on the identifier
    /// boundary is what keeps `int5` or a field named `total2` from matching,
    /// while still catching every position a type is rendered into — `<t5>`,
    /// `(t5)`, `-> t5`, ` t5,`.
    fn regex_like_type_var(s: &str) -> bool {
        let is_ident = |c: char| c.is_alphanumeric() || c == '_';
        s.match_indices('t').any(|(at, _)| {
            let starts_identifier = s[..at].chars().next_back().is_none_or(|c| !is_ident(c));
            let then_digit = s[at + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit());
            starts_identifier && then_digit
        })
    }

    #[test]
    fn the_leak_detector_catches_what_it_is_for_and_nothing_else() {
        // A detector used by three tests above is itself worth pinning: if it
        // silently stopped matching, those tests would pass while leaking.
        for leaked in [
            "List<t5>",
            "fn f(x: t0) -> t1",
            "(t5) -> List<t6>",
            "-> t42",
        ] {
            assert!(regex_like_type_var(leaked), "must catch {leaked}");
        }
        for clean in [
            "fn twice(n: int) -> Result<int, MathError>",
            "fn classify(xs: List<_>) -> string",
            "{ total2: int }",
        ] {
            assert!(!regex_like_type_var(clean), "must not flag {clean}");
        }
    }
}
