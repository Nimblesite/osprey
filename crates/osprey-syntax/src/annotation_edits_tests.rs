use crate::{annotation_edits, parse_program_with_flavor, AnnotationTarget, Flavor};

fn erased(source: &str, flavor: Flavor, choose: impl Fn(&AnnotationTarget) -> bool) -> String {
    let candidates = annotation_edits(source, flavor);
    assert!(
        !candidates.is_empty(),
        "missing source annotations: {source}"
    );
    let mut edits: Vec<_> = candidates
        .into_iter()
        .filter(|e| choose(&e.target))
        .flat_map(|e| e.edits)
        .collect();
    edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
    let mut result = source.to_owned();
    for edit in edits {
        result.replace_range(edit.range, &edit.new_text);
    }
    let parsed = parse_program_with_flavor(&result, flavor);
    assert!(parsed.errors.is_empty(), "{result}\n{:?}", parsed.errors);
    result
}

#[test]
fn annotation_edits_preserve_default_bodies_and_comments() {
    let source = "// 😀 heading\r\nfn smaller(a: int, b: int) -> int = a < b ? a : b\r\nlet closure: (int) -> int = fn(x: int) -> int => x\r\n";
    let edits = annotation_edits(source, Flavor::Default);
    assert_eq!(edits.len(), 6);
    assert_eq!(
        edits
            .iter()
            .map(|e| &source[e.highlight.clone()])
            .collect::<Vec<_>>(),
        [
            ": int",
            ": int",
            "-> int",
            ": (int) -> int",
            ": int",
            "-> int"
        ]
    );
    assert_eq!(
        erased(source, Flavor::Default, |_| true),
        "// 😀 heading\r\nfn smaller(a, b) = a < b ? a : b\r\nlet closure = fn(x) => x\r\n"
    );
}

#[test]
fn annotation_edits_remove_ml_headers_and_preserve_exports() {
    let source = "module Math\n    // 😀 documentation\n    export smaller : int -> int -> int\n    // definition comment\n    smaller a b = if a < b then a else b\n";
    let candidates = annotation_edits(source, Flavor::Ml);
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        &source[candidates
            .first()
            .map(|e| e.highlight.clone())
            .unwrap_or(0..0)],
        "smaller : int -> int -> int"
    );
    assert_eq!(erased(source, Flavor::Ml, |_| true), "module Math\n    // 😀 documentation\n    // definition comment\n    export smaller a b = if a < b then a else b\n");
}

#[test]
fn annotation_edits_ml_nested_comments_multiline_crlf_and_inline_types() {
    let source = "// 😀\r\nidentity : (int\r\n    (* outer (* nested *) comment *)\r\n    -> int)\r\nidentity (x : int) = x\r\n";
    let result = erased(source, Flavor::Ml, |_| true);
    assert!(result.contains("(* outer (* nested *) comment *)"));
    assert!(result.ends_with("identity (x) = x\r\n"), "{result:?}");
    assert!(!result.contains("->"));
    assert!(!result.replace("\r\n", "").contains('\n'));
    assert!(result.starts_with("// 😀\r\n"));
}

#[test]
fn annotation_edits_retain_generic_effect_and_contract_headers() {
    for source in ["identity<T> : T -> T\nidentity x = x\n", "effect State\n    get : Unit => int\nread : Unit -> int ! State\nread () = perform State.get ()\n", "signature S\n    identity : int -> int\n"] {
        assert!(parse_program_with_flavor(source, Flavor::Ml).errors.is_empty(), "invalid protected header: {source}");
        assert!(annotation_edits(source, Flavor::Ml).is_empty(), "{source}");
    }
    for flavor in [Flavor::Default, Flavor::Ml] {
        assert!(annotation_edits("fn broken(: int", flavor).is_empty());
    }
}

#[test]
fn annotation_edits_keep_trailing_comments_and_module_visibility() {
    use osprey_ast::{Stmt, Visibility};
    let source = "module Public\r\n    export greet : string -> string // preserve 😀\r\n    // between\r\n    greet name = \"hi \" + name\r\n";
    let result = erased(source, Flavor::Ml, |_| true);
    assert_eq!(result, "module Public\r\n    // preserve 😀\r\n    // between\r\n    export greet name = \"hi \" + name\r\n");
    for text in [source, result.as_str()] {
        let parsed = parse_program_with_flavor(text, Flavor::Ml);
        let Some(Stmt::Module { body, .. }) = parsed.program.statements.first() else {
            panic!("missing module");
        };
        assert_eq!(body.len(), 1);
        let Some(item) = body.first() else {
            panic!("missing export");
        };
        assert_eq!(item.visibility, Visibility::Exported);
        assert!(matches!(&*item.declaration, Stmt::Function { name, .. } if name == "greet"));
    }
}

#[test]
fn annotation_edits_default_comments_inside_types_are_retained() {
    let source = "fn apply(f: // callback\n(int) -> int) -> int = f(1)\nlet lambda = fn(x: int) -> // result\nint => x\n";
    let edits = annotation_edits(source, Flavor::Default);
    assert_eq!(edits.len(), 4);
    let result = erased(source, Flavor::Default, |_| true);
    assert!(result.contains("// callback\n"));
    assert!(result.contains("// result\n"));
    assert!(result.contains("= f(1)\n"));
    assert!(result.ends_with(" => x\n"));
    assert!(!result.contains("->"));
    assert!(!result.contains(':'));
}

#[test]
fn nested_fragment_annotations_use_raw_byte_ranges_after_escapes_and_unicode() {
    for (flavor, source, expected) in [
        (
            Flavor::Default,
            "fn greet() = \"\\n🦅 ${(fn(value: int, ignored) => (value + 1) ?: 0)(7, 9)}\"\n",
            "fn greet() = \"\\n🦅 ${(fn(value, ignored) => (value + 1) ?: 0)(7, 9)}\"\n",
        ),
        (
            Flavor::Ml,
            "greet () = \"\\n🦅 ${(\\(value : int, ignored) => (value + 1) ?: 0) (7, 9)}\"\n",
            "greet () = \"\\n🦅 ${(\\(value, ignored) => (value + 1) ?: 0) (7, 9)}\"\n",
        ),
    ] {
        let annotations = annotation_edits(source, flavor);
        let [annotation] = annotations.as_slice() else {
            panic!("missing annotation: {source}\n{annotations:?}");
        };
        assert_eq!(source.get(annotation.highlight.clone()), Some(": int"));
        assert_eq!(annotation.position.line, 1);
        assert_eq!(erased(source, flavor, |_| true), expected);
        let bindings = crate::binding_ranges(source, flavor);
        let ignored: Vec<_> = bindings
            .iter()
            .filter(|binding| binding.name == "ignored")
            .collect();
        let [ignored] = ignored.as_slice() else {
            panic!("missing actual ignored binding: {bindings:?}");
        };
        assert_eq!(source.get(ignored.range.clone()), Some("ignored"));
        assert!(ignored.range.start > annotation.highlight.end);
        assert_eq!(ignored.owner_position, Some(annotation.owner_position));
    }
}
