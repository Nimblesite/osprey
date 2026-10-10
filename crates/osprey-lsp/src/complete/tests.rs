use super::*;
use crate::test_support::completion;
const U16: PositionEncoding = PositionEncoding::Utf16;

/// Complete at the end of `src` — where an author's cursor actually is.
fn at_end(src: &str, path: &str) -> Vec<CompletionItem> {
    let (line, column) = crate::text::end_position(src);
    completion(src, path, line, column, U16)
}

/// Owned labels: the caller compares them against a temporary list, and a
/// borrow would pin that temporary for the whole assertion.
fn labels(items: &[CompletionItem]) -> Vec<String> {
    items.iter().map(|i| i.label.clone()).collect()
}

fn has(names: &[String], label: &str) -> bool {
    names.iter().any(|name| name == label)
}

const SRC: &str = "fn add(a: int, b: int) -> int = wrapAdd(a, b)\nlet total = add(1, 2)\n";

#[test]
fn a_completion_item_carries_the_type_the_checker_proved() {
    // A completion item's detail is the symbol's rendered type, and it came
    // from the raw AST — so deleting the inferable `: int` that CLAUDE.md
    // requires deleting emptied the detail to `fn twice(n)`. Completion,
    // signature help, the outline and hover are four views of one type and
    // must agree ([`collect_inferred_symbols`]).
    let items = at_end(
        "fn twice(n) = wrapMul(n, 2)\nlet y = twice(2)\n",
        "file:///t.osp",
    );
    let twice = items
        .iter()
        .find(|i| i.label == "twice")
        .expect("`twice` is completable");
    // The total helper returns int; the detail exposes that inferred type
    // even though the author wrote no annotation.
    assert_eq!(
        twice.detail.as_deref(),
        Some("fn twice(n: int) -> int"),
        "a completion detail must carry the inferred types"
    );
}

#[test]
fn completion_includes_keywords_and_declarations_at_declaration_position() {
    let items = at_end(SRC, "file:///a.osp");
    assert!(items
        .iter()
        .any(|i| i.label == "fn" && i.kind == CompletionKind::Keyword));
    assert!(items
        .iter()
        .any(|i| i.label == "add" && i.kind == CompletionKind::Function));
}

#[test]
fn a_type_annotation_is_never_offered_a_declaration_snippet() {
    // The defect: `fn`, `let` and `namespace` were offered inside a type
    // annotation, and each expands to a whole declaration — source that
    // cannot appear after a `:` under any flavor.
    let items = at_end("fn f(x: ", "file:///a.osp");
    let names = labels(&items);
    for keyword in ["fn", "let", "match", "namespace", "type"] {
        assert!(!has(&names, keyword), "{keyword} in a type: {names:?}");
    }
    // What belongs there is offered instead: the built-in type names.
    assert!(has(&names, "int"), "{names:?}");
    assert!(has(&names, "string"), "{names:?}");
    // A declared type is offered; a function and a binding are not.
    let declared = at_end(
        "type Shade = Light | Dark\nlet c = 1\nfn f(x: ",
        "file:///a.osp",
    );
    let names = labels(&declared);
    assert!(has(&names, "Shade"), "{names:?}");
    assert!(!has(&names, "c"), "a binding is not a type: {names:?}");
}

#[test]
fn a_value_position_drops_declaration_keywords_but_keeps_expression_ones() {
    // `let x = fn …` and `let x = namespace …` do not parse; `match` does.
    let items = at_end("fn f() = ", "file:///a.osp");
    let names = labels(&items);
    for keyword in ["fn", "let", "type", "effect", "namespace", "import"] {
        assert!(!has(&names, keyword), "{keyword} as a value: {names:?}");
    }
    assert!(has(&names, "match"), "{names:?}");
    assert!(has(&names, "if"), "{names:?}");
    // Symbols are still offered — a value position is where they are used.
    let with_symbols = at_end("fn g() = 1\nfn f() = ", "file:///a.osp");
    assert!(has(&labels(&with_symbols), "g"));
}

#[test]
fn a_field_access_offers_only_that_records_fields() {
    // [LSP-COMPLETION-MEMBER]
    // Completing after `origin.` used to dump the entire symbol table.
    let src = "type Point = { x: int, y: int }\n\
               let origin = Point { x: 1, y: 2 }\n\
               print(origin.";
    let names = labels(&at_end(src, "file:///a.osp"));
    assert_eq!(names, vec!["x", "y"], "{names:?}");
    // An unknown receiver stays silent rather than falling back to noise.
    assert_eq!(
        at_end("fn f() = mystery.", "file:///a.osp"),
        Vec::<CompletionItem>::new()
    );
}

#[test]
fn a_match_arm_is_offered_constructors_and_the_wildcard_not_keywords() {
    let src = "type Shade = Light | Dark\nfn f(s) = match s {\n    ";
    let names = labels(&at_end(src, "file:///a.osp"));
    assert!(has(&names, "Light"), "{names:?}");
    assert!(has(&names, "Dark"), "{names:?}");
    assert!(has(&names, WILDCARD), "{names:?}");
    assert!(!has(&names, "fn"), "{names:?}");
    // Ordering is stable: the checker keys constructors by hash, so a raw
    // iteration would reshuffle the list between keystrokes.
    assert_eq!(names, labels(&at_end(src, "file:///a.osp")));
}

#[test]
fn a_parameter_name_has_nothing_to_complete() {
    assert_eq!(
        at_end("fn add(", "file:///a.osp"),
        Vec::<CompletionItem>::new()
    );
    assert_eq!(
        at_end("fn add(a: int, ", "file:///a.osp"),
        Vec::<CompletionItem>::new()
    );
}

#[test]
fn ml_completions_never_offer_a_keyword_the_ml_frontend_does_not_have() {
    // Completion presentation for [LSP-FLAVOR-RENDER].
    // `fn`, `let` and `if` are absent from `ml::token::keyword_or_ident`:
    // ML defines by bare clause and branches with `match` on true/false.
    // Completing them inserts plain identifiers and a guaranteed parse
    // error, and every brace snippet is rejected outright by the layout
    // parser. A `.ospml` document must be offered ML spellings only.
    let items = at_end("inc x = x + 1\n", "file:///tour.ospml");
    let labelled = |name: &str| items.iter().find(|i| i.label == name);
    for absent in ["fn", "let", "if"] {
        assert!(labelled(absent).is_none(), "ML has no `{absent}` keyword");
    }
    // The kept keywords must expand to LAYOUT, never braces. `${1:expr}` is
    // a snippet placeholder, so the tell is the Default block spelling
    // (` {` opening a body, ` | ` separating inline variants), not `{`.
    for (name, forbidden) in [("match", " {\n"), ("type", " | "), ("effect", " {\n")] {
        let snippet = labelled(name)
            .and_then(|i| i.insert_text.clone())
            .unwrap_or_else(|| panic!("ML keeps `{name}`"));
        assert!(!snippet.contains(forbidden), "{name}: {snippet}");
    }
    // A marker still outranks the extension here, exactly as it does for
    // the CLI and diagnostics — flavor resolution is one chain, not three.
    let marked = at_end("// osprey: flavor=ml\ninc x = x + 1\n", "file:///a.txt");
    assert!(
        marked.iter().all(|i| i.label != "fn"),
        "marker outranks ext"
    );
}

#[test]
fn completion_includes_qualified_symbols_and_flavor_specific_module_snippets() {
    // [MODULES-FLAVOR-PROJECTION] Both flavors expose the same concepts,
    // while insertion text stays idiomatic for the active surface.
    let src = "namespace billing { module Tax { export fn addTax(x) = x } }\n";
    let default = at_end(src, "file:///billing.osp");
    assert!(default
        .iter()
        .any(|item| item.label == "billing::Tax::addTax"));
    assert!(default.iter().any(|item| {
        item.label == "state"
            && item
                .insert_text
                .as_deref()
                .is_some_and(|text| text.starts_with("state module"))
    }));
    for keyword in [
        "namespace",
        "import",
        "module",
        "signature",
        "export",
        "opaque",
        "as",
    ] {
        assert!(
            default.iter().any(|item| item.label == keyword),
            "{keyword}"
        );
    }

    let ml = at_end("module Tax\n    x = 1\n", "file:///billing.ospml");
    assert!(ml.iter().any(|item| {
        item.label == "state"
            && item
                .insert_text
                .as_deref()
                .is_some_and(|text| text.starts_with("state ") && !text.starts_with("state module"))
    }));
}

#[test]
fn completion_maps_a_type_declaration_to_the_type_kind() {
    let src = "type Shade = Light | Dark\nlet c: int = 1\n";
    let items = at_end(src, "file:///a.osp");
    assert!(items
        .iter()
        .any(|i| i.label == "Shade" && i.kind == CompletionKind::Type));
    // The variable `c` is a Variable-kind completion with its detail.
    let c = items.iter().find(|i| i.label == "c").expect("c completion");
    assert_eq!(c.kind, CompletionKind::Variable);
    assert_eq!(c.detail.as_deref(), Some("int"));
}

#[test]
fn a_generic_records_field_completes_at_the_type_this_receiver_instantiated() {
    // `layout.fields` stores a generic field (`value: T`) as a `Type::Var`
    // — the layout table is what the BACKEND reads, where a variable means
    // "boxed representation". Rendering it straight into a completion
    // detail put the checker's private `t0` in front of a user, the same
    // artefact [TYPE-RENDER-HOLES] keeps out of hover and the outline.
    // Member completion reaches the layout by its own path, so it needed
    // the reader rendering too.
    let generic = "type Box<T> = { value: T }\nlet b = Box { value: 1 }\nlet v = b.\n";
    assert_member(
        &completion(generic, "file:///m.osp", 2, 10, PositionEncoding::Utf16),
        &[("value", "int")],
        "a `Box` built from an int instantiates the field to `int`, and the \
         receiver's own type knows it",
    );
    // A CONCRETE declaration is unaffected — the hole must not swallow what
    // the declaration actually fixed — and a record with MORE than one
    // field proves the list is the receiver's, in declaration order.
    let concrete =
        "type Pt = { x: int, label: string }\nlet p = Pt { x: 1, label: \"a\" }\nlet v = p.\n";
    assert_member(
        &completion(concrete, "file:///c.osp", 2, 10, PositionEncoding::Utf16),
        &[("x", "int"), ("label", "string")],
        "every declared field, at its declared type, in DECLARATION order — \
         the same list an annotated binding gets",
    );
}

/// Assert a member-completion result WHOLE: the exact set of labels, and
/// for each item its kind, detail and insert text.
///
/// Checking one field's `detail` and stopping would pass on a list that
/// also offered the whole symbol table, offered a field as a function, or
/// leaked an inference name into a neighbour — `origin.` is a promise that
/// only `origin`'s fields follow.
fn assert_member(items: &[CompletionItem], expected: &[(&str, &str)], why: &str) {
    assert_eq!(
        items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
        expected.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        "{why}"
    );
    for (item, (name, ty)) in items.iter().zip(expected) {
        assert_eq!(item.detail.as_deref(), Some(*ty), "{name}: {why}");
        assert_eq!(item.kind, CompletionKind::Variable, "{name} is a value");
        assert_eq!(item.insert_text, None, "{name} inserts its own label");
        assert!(
            !item.detail.as_deref().is_some_and(mentions_inference_name),
            "{name}: a private inference name reached a reader"
        );
    }
}

/// Whether `s` mentions an inference name (`t0`, `t42`) — a `t` starting an
/// identifier and followed by a digit.
fn mentions_inference_name(s: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    s.match_indices('t').any(|(at, _)| {
        s[..at].chars().next_back().is_none_or(|c| !is_ident(c))
            && s[at + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
    })
}

#[test]
fn the_constructor_layout_fallback_holes_an_open_field_and_keeps_a_fixed_one() {
    // `member_items` answers two ways and only the first was covered: the
    // receiver's own inferred record (instantiated), else the constructor
    // LAYOUT — which stores a generic field as a `Type::Var`, because that
    // table is what the BACKEND reads, where a variable means the boxed
    // representation. `field_item` must render it for a person
    // ([TYPE-RENDER-HOLES]); nothing asserted that, so restoring
    // `ty.to_string()` there would have put `t0` in front of a user with
    // every other test still green. Covers #218.
    //
    // An ANNOTATED binding is what reaches the fallback: the annotation
    // names the type, so the instantiated record inference built for the
    // initializer is not what answers.

    // Annotated + GENERIC: the annotation `Box` fixes nothing about
    // `value`, so a hole is the honest answer.
    assert_member(
        &completion(
            "type Box<T> = { value: T }\nlet b: Box = Box { value: 1 }\nlet v = b.\n",
            "file:///fallback-generic.osp",
            2,
            10,
            PositionEncoding::Utf16,
        ),
        &[("value", "_")],
        "the layout fallback holes a field the declaration leaves open",
    );

    // Annotated + CONCRETE, several fields: the fallback must not blank
    // everything it touches, and it must offer the whole record.
    assert_member(
        &completion(
            "type Pt = { x: int, label: string }\nlet p: Pt = Pt { x: 1, label: \"a\" }\nlet v = p.\n",
            "file:///fallback-concrete.osp",
            2,
            10,
            PositionEncoding::Utf16,
        ),
        // Declaration order, and the receiver path below now agrees. It
        // did not: that path read the inferred record's `BTreeMap` and
        // came out alphabetical, so one record completed two different
        // ways depending on whether its binding carried an annotation
        // nobody needed to write.
        &[("x", "int"), ("label", "string")],
        "a fixed field survives the fallback unchanged, in declaration order",
    );

    // THE CONTRAST: the same generic record with the annotation DELETED
    // takes the receiver path, which knows the instantiation and answers
    // `int`. These two differ by the annotation alone, so this pins that
    // the paths give genuinely different answers rather than one answer
    // reached twice — and that deleting an annotation never loses
    // information, which is what the house style promises.
    assert_member(
        &completion(
            "type Box<T> = { value: T }\nlet b = Box { value: 1 }\nlet v = b.\n",
            "file:///receiver-path.osp",
            2,
            10,
            PositionEncoding::Utf16,
        ),
        &[("value", "int")],
        "an inferred receiver reports what it instantiated, never a hole",
    );

    // And the two paths agree on the LIST, differing only where they must:
    // the type the annotation left open. Asserting the orders equal is the
    // point — they were `[x, label]` and `[label, x]`, so a contributor
    // obeying CLAUDE.md and deleting `: Pt` silently reordered their own
    // completion popup.
    let labels = |src: &str, uri: &str| {
        completion(src, uri, 2, 10, PositionEncoding::Utf16)
            .into_iter()
            .map(|i| i.label)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        labels(
            "type Pt = { x: int, label: string }\nlet p: Pt = Pt { x: 1, label: \"a\" }\nlet v = p.\n",
            "file:///order-annotated.osp",
        ),
        labels(
            "type Pt = { x: int, label: string }\nlet p = Pt { x: 1, label: \"a\" }\nlet v = p.\n",
            "file:///order-inferred.osp",
        ),
        "deleting an inferable annotation must not reorder the fields a \
         reader is offered"
    );
}
