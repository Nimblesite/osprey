use super::*;
use crate::test_support::col_of;
use crate::test_support::definition;
use crate::test_support::hover;
use crate::test_support::references;
use crate::test_support::signature_help;
const U16: PositionEncoding = PositionEncoding::Utf16;

#[test]
fn nested_bindings_end_at_their_lexical_scope() {
    let src = "fn outer(x) = {\n    let inner = {\n        let x = 2\n        x\n    }\n    x\n}\n";
    let uri = "file:///nested.osp";
    assert_scoped_definitions(src, uri, &[((3, "x"), (2, "x =")), ((5, "x"), (0, "x)"))]);
    let found = reference_spans(src, uri, 0, "x)", true);
    assert_eq!(
        found,
        vec![word_span(src, (0, "x)")), word_span(src, (5, "x"))]
    );
    let inner = reference_spans(src, uri, 3, "x", true);
    assert_eq!(
        inner,
        vec![word_span(src, (2, "x =")), word_span(src, (3, "x"))]
    );
}

#[test]
fn layout_bindings_end_at_their_lexical_scope() {
    for (src, uri, last) in [
        (
            "outer x =\n    inner =\n        x = 2\n        x\n    x\n",
            "file:///nested.ospml",
            4,
        ),
        (
            "def outer(x):\n    if True:\n        x = 2\n        print(x)\n    return x\n",
            "file:///nested.py",
            4,
        ),
    ] {
        assert_scoped_definitions(src, uri, &[((3, "x"), (2, "x =")), ((last, "x"), (0, "x"))]);
        let found = reference_spans(src, uri, 0, "x", true);
        assert_eq!(
            found,
            vec![word_span(src, (0, "x")), word_span(src, (last, "x"))]
        );
    }
}

#[test]
fn lambda_and_handler_parameters_do_not_capture_later_outer_references() {
    for src in [
        "fn outer(x) = {\n    let inner = fn(x) => x\n    x\n}\n",
        "effect Ask { value: fn(int) -> int }\nfn outer(x) = {\n    handle Ask { value x => x }\n    x\n}\n",
    ] {
        let outer = usize::from(src.starts_with("effect"));
        assert_scoped_definitions(src, "file:///nested.osp", &[((outer + 2, "x"), (outer, "x)"))]);
        assert_eq!(reference_spans(src, "file:///nested.osp", outer, "x)", true).len(), 2);
    }
}

#[test]
fn local_function_definitions_remain_navigable() {
    let src = "fn outer() = {\n    fn inner(x) = x\n    inner(1)\n}\n";
    assert_scoped_definitions(src, "file:///nested.osp", &[((2, "inner"), (1, "inner"))]);
}

#[test]
fn ml_lambda_parameters_end_at_the_closing_parenthesis() {
    let src = "outer x =\n    inner = (\\x => x)\n    x\n";
    assert_scoped_definitions(src, "file:///nested.ospml", &[((2, "x"), (0, "x"))]);
    assert_eq!(
        reference_spans(src, "file:///nested.ospml", 0, "x", true).len(),
        2
    );
}

#[test]
fn ml_handler_parameters_do_not_leak_into_sibling_arms() {
    let src = "effect Ask\n    first : int => int\n    second : Unit => int\nouter x =\n    handle Ask\n        first x => x\n        second => x\n    x\n";
    assert_scoped_definitions(src, "file:///nested.ospml", &[((6, "x"), (3, "x"))]);
}
const SRC: &str = "fn add(a: int, b: int) -> int = wrapAdd(a, b)\nlet total = add(1, 2)\n";

#[test]
fn definition_points_at_the_declaration() {
    let first = first_definition(SRC, "file:///a.osp", 1, 12);
    assert_eq!(first.span.0, 0, "{first:?}");
}

#[test]
fn definition_of_a_builtin_anchors_on_the_identifier() {
    // A documented built-in has no source declaration, so go-to-definition
    // returned nothing and the editor reported "No definition found for
    // 'listAppend'" over a real, hoverable function. Resolve a built-in to
    // the identifier under the cursor instead. Implements
    // [LSP-DEFINITION-BUILTIN].
    let src = "let batch = listAppend(List(), 6)\n";
    let col = col_of(src, 0, "listAppend");
    let first = first_definition(src, "file:///a.osp", 0, col);
    assert_eq!(first.span.0, 0, "on the same line: {first:?}");
    assert_eq!(
        first.span.1,
        col - 1,
        "anchored at the identifier: {first:?}"
    );
}

#[test]
fn references_can_exclude_the_declaration() {
    let with = references(SRC, "file:///a.osp", 0, 3, U16, true);
    let without = references(SRC, "file:///a.osp", 0, 3, U16, false);
    assert_eq!(with.len(), 2);
    assert_eq!(without.len(), 1);
}

#[test]
fn signature_help_tracks_the_active_parameter_through_strings_and_comments() {
    // Line 1 of `SRC` is `let total = add(1, 2)`; char 19 is over the second
    // argument, and the call exposes both parameters.
    let sig = sig_at(SRC, "file:///a.osp", 1, 19);
    assert_eq!(sig.active_parameter, 1, "{sig:?}");
    assert_eq!(sig.parameters.len(), 2);
    // Commas inside string literals — and the escaped quote and `//` comment
    // that must not corrupt the comma/call tracking — leave the active
    // parameter where the still-open call puts it.
    for (src, column, active) in [
        (
            "fn f(a: int, b: int) -> int = a\nlet x = f(\"a, b, c\", 2)\n",
            21,
            1,
        ),
        (
            "fn f(a: int, b: int) -> int = a\nlet x = f(\"a\\\"b\" // c, d\n",
            23,
            0,
        ),
    ] {
        let sig = sig_at(src, "file:///a.osp", 1, column);
        assert_eq!(sig.active_parameter, active, "{sig:?}");
    }
}

#[test]
fn qualified_hover_definition_and_references_resolve_one_module_member() {
    // [MODULES-ABI] `::` is scanned as one symbol. Two colliding leaf names
    // remain independently navigable through their qualified paths.
    let src = "namespace sales {\n\
                 module Tax { export fn rate() -> int = 10 }\n\
               }\n\
               namespace payroll {\n\
                 module Tax { export fn rate() -> int = 20 }\n\
               }\n\
               let chosen = sales::Tax::rate()\n";
    let column = col_of(src, 6, "sales::Tax::rate");
    let hover = hover(src, "file:///modules.osp", 6, column, U16).expect("hover");
    crate::testkit::shows(&hover, &["fn rate() -> int"]);

    let definitions = definitions_at(src, "file:///modules.osp", (6, "sales::Tax::rate"));
    assert_eq!(definitions.len(), 1, "{definitions:?}");
    assert_eq!(
        definitions.first().map(|span| span.0),
        Some(1),
        "sales declaration line"
    );

    let found = reference_spans(src, "file:///modules.osp", 6, "sales::Tax::rate", true);
    assert_eq!(found.len(), 2, "use plus declaration: {found:?}");
    assert!(!found.iter().any(|span| span.0 == 4));
}

/// A word by `(line, needle)`: the identifier that starts the needle's first
/// occurrence on that 0-based line.
type Word<'a> = (usize, &'a str);

fn word_span(src: &str, (line, needle): Word<'_>) -> (u32, u32, u32, u32) {
    let word = needle
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .unwrap_or(needle);
    let start = col_of(src, line, needle) - 1;
    let row = u32::try_from(line).expect("line fits");
    (
        row,
        start,
        row,
        start + u32::try_from(word.len()).expect("word fits"),
    )
}

fn definitions_at(src: &str, uri: &str, cursor: Word<'_>) -> Vec<(u32, u32, u32, u32)> {
    let row = u32::try_from(cursor.0).expect("line fits");
    definition(src, uri, row, col_of(src, cursor.0, cursor.1), U16)
        .into_iter()
        .map(|location| location.span)
        .collect()
}

/// Every `(use, binding)` pair resolves the use to exactly that binding.
fn assert_scoped_definitions(src: &str, uri: &str, cases: &[(Word<'_>, Word<'_>)]) {
    for &(cursor, binding) in cases {
        let found = definitions_at(src, uri, cursor);
        assert_eq!(found, vec![word_span(src, binding)], "{cursor:?} in {uri}");
    }
}

/// The spans `references` reports from the cursor over `needle` on 0-based `line`.
fn reference_spans(
    src: &str,
    uri: &str,
    line: usize,
    needle: &str,
    include_declaration: bool,
) -> Vec<(u32, u32, u32, u32)> {
    let row = u32::try_from(line).expect("line fits");
    references(
        src,
        uri,
        row,
        col_of(src, line, needle),
        U16,
        include_declaration,
    )
    .into_iter()
    .map(|location| location.span)
    .collect()
}

/// The signature help at `(line, column)`, failing the test when there is none.
fn sig_at(src: &str, uri: &str, line: u32, column: u32) -> SignatureInfo {
    signature_help(src, uri, line, column, U16).expect("sig")
}

/// The first definition at the cursor, failing the test when none resolves.
fn first_definition(src: &str, uri: &str, line: u32, column: u32) -> Location {
    let defs = definition(src, uri, line, column, U16);
    defs.first().expect("definition").clone()
}

const SCOPED_DEFAULT: &str = "effect Log { write: fn(string) -> Unit }\n\
fn twice(work) = {\n    let first = work()\n    handle Log {\n        write message => print(message)\n    }\n    first + work()\n}\n\
fn other(work) = work()\n";

const SCOPED_PYTHON: &str =
    "# osprey: flavor=python\n@effect\nclass Log:\n    def write(message: str) -> None: ...\n\n\n\
@handler(Log)\nclass console:\n    def write(message):\n        print(message)\n\n\n\
def twice(work):\n    first = work()\n    return first + work()\n\n\n\
def other(work):\n    return work()\n";

// [LSP-DEFINITION-LOCAL]: `work()` in gpu.osp's `onGpu(work)` found no
// definition, because definition read only top-level declarations. A
// parameter, a local binding and a handler-arm parameter resolve to the
// binding in scope, never to a same-named binding in another function.
#[test]
fn definition_resolves_parameters_locals_and_arm_parameters_in_scope() {
    assert_scoped_definitions(
        SCOPED_DEFAULT,
        "file:///scoped.osp",
        &[
            ((2, "work()"), (1, "work)")),
            ((6, "work()"), (1, "work)")),
            ((1, "work)"), (1, "work)")),
            ((6, "first"), (2, "first")),
            ((4, "message)"), (4, "message =>")),
            ((8, "work()"), (8, "work)")),
        ],
    );
    assert_scoped_definitions(
        SCOPED_PYTHON,
        "file:///scoped.py",
        &[
            ((13, "work()"), (12, "work)")),
            ((14, "work()"), (12, "work)")),
            ((14, "first"), (13, "first")),
            ((9, "message)"), (8, "message")),
            ((18, "work()"), (17, "work)")),
        ],
    );
}

// [LSP-REFERENCES-LOCAL]: references to a parameter stay inside its function;
// `other`'s `work` is a different binding.
#[test]
fn references_to_a_parameter_stay_inside_its_function() {
    let uri = "file:///scoped.osp";
    let mut with = reference_spans(SCOPED_DEFAULT, uri, 2, "work()", true);
    with.sort_unstable();
    let expected: Vec<_> = [(1, "work)"), (2, "work()"), (6, "work()")]
        .into_iter()
        .map(|word| word_span(SCOPED_DEFAULT, word))
        .collect();
    assert_eq!(with, expected);
    assert_eq!(
        reference_spans(SCOPED_DEFAULT, uri, 2, "work()", false).len(),
        2
    );
}

#[test]
fn definition_and_references_return_empty_off_any_identifier() {
    // A two-space gap guarantees a column that is over neither word.
    let src = "let a  =  b\n";
    // Column 6 sits in the double space between `a` and `=`.
    assert_eq!(
        definition(src, "file:///a.osp", 0, 6, U16),
        Vec::<Location>::new()
    );
    assert_eq!(
        references(src, "file:///a.osp", 0, 6, U16, true),
        Vec::<Location>::new()
    );
    // A line past the end of the file yields no word either.
    assert!(hover(src, "file:///a.osp", 99, 0, U16).is_none());
}

#[test]
fn signature_help_labels_a_parameter_with_the_type_the_checker_proved() {
    // `id` is generic — nothing constrains `x`, so there is genuinely no
    // type to show and the label stays the bare name.
    let src = "fn id(x) = x\nlet y = id(7)\n";
    let sig = sig_at(src, "file:///a.osp", 1, 11);
    assert_eq!(sig.parameters, vec!["x".to_owned()]);
    assert_eq!(sig.active_parameter, 0);

    // But a parameter the checker DID prove must carry its type, exactly as
    // the outline and hover do. Signature help read raw `collect_symbols`,
    // so it never consulted inference: deleting the inferable `: int` that
    // CLAUDE.md requires deleting silently downgraded the help to `n`.
    // Implements [LSP-HOVER-INFERRED-SIGNATURE].
    let proved = "fn twice(n) = n * 2\nlet y = twice(7)\n";
    let sig = sig_at(proved, "file:///b.osp", 1, 14);
    assert_eq!(sig.parameters, vec!["n: int".to_owned()]);
    // The annotated spelling is the control: tooling must not be able to
    // tell the two apart.
    let annotated = "fn twice(n: int) = n * 2\nlet y = twice(7)\n";
    let control = sig_at(annotated, "file:///c.osp", 1, 14);
    assert_eq!(sig.parameters, control.parameters);
    assert_eq!(sig.label, control.label);
}

#[test]
fn signature_help_opens_on_the_callee_and_unwinds_closed_inner_calls() {
    // The inner `add(1, 2)` call is closed before the cursor, so the active
    // call is the still-open outer one — the `)` arm pops the call/comma
    // stacks. Editors also ask the moment the callee is typed: answering only
    // between the parentheses stops helping right after the name.
    let trigger = "fn add(a: int, b: int) -> int = (a + b) ?: 0\nlet total = add\n";
    for (src, column, active) in [
        (
            "fn add(a: int, b: int) -> int = (a + b) ?: 0\nlet r = add(add(1, 2), 3)\n",
            24,
            1,
        ),
        (trigger, 13, 0),
    ] {
        let sig = sig_at(src, "file:///a.osp", 1, column);
        assert_eq!(sig.label, "fn add(a: int, b: int) -> int");
        assert_eq!(sig.active_parameter, active, "{sig:?}");
    }
    // A word that names no function still yields nothing — `total` is a
    // binding, and offering it a parameter list would be a lie.
    assert!(signature_help(trigger, "file:///a.osp", 1, 5, U16).is_none());
}

#[cfg(unix)]
#[test]
fn a_symbol_declared_in_a_sibling_file_resolves_across_the_project() {
    // Single-file analysis made every imported symbol invisible: hovering
    // `Ledger::sqlite` in the composition root said nothing and
    // go-to-definition went nowhere, even though the compiler links both
    // files into one program. Implements [LSP-WORKSPACE].
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../osprey/examples/projects/modules");
    let entry = root.join("src/main.ospml");
    let text = std::fs::read_to_string(&entry).expect("read the composition root");
    let uri = format!("file://{}", entry.display());
    let (line, column) = position_of(&text, "Ledger::sqlite");

    let hovered = hover(&text, &uri, line, column, U16).expect("cross-file hover");
    crate::testkit::shows(&hovered, &["sqlite"]);

    let first = first_definition(&text, &uri, line, column);
    assert!(first.uri.ends_with("store/ledger.ospml"), "{first:?}");

    // References reach into the declaring file as well as this one.
    let references = references(&text, &uri, line, column, U16, true);
    assert!(
        references.iter().any(|l| l.uri.ends_with("ledger.ospml")),
        "{references:?}"
    );
    assert!(references.iter().any(|l| l.uri == uri), "{references:?}");

    // Completion after the module prefix lists the sibling file's export.
    let after_prefix = column.saturating_add(u32::try_from("Ledger::".len()).unwrap_or(0));
    let labels: Vec<String> = crate::test_support::completion(&text, &uri, line, after_prefix, U16)
        .into_iter()
        .map(|item| item.label)
        .collect();
    assert!(
        labels.iter().any(|label| label == "bank::Ledger::sqlite"),
        "{labels:?}"
    );
}

/// The 0-based `(line, column)` of the first occurrence of `needle`.
fn position_of(text: &str, needle: &str) -> (u32, u32) {
    text.lines()
        .enumerate()
        .find_map(|(row, line)| {
            let column = line.find(needle)?;
            Some((
                u32::try_from(row).unwrap_or(0),
                u32::try_from(column).unwrap_or(0),
            ))
        })
        .unwrap_or_else(|| panic!("`{needle}` is not in the document"))
}

mod union_variants;

#[test]
fn hover_on_pattern_binders_and_lambda_parameters_shows_their_inferred_types() {
    // A match-arm binder and a lambda parameter are bindings without a `let`,
    // so neither reached the binding table and hovering one returned nothing —
    // in the pattern that binds it and in the body that reads it alike.
    // Implements [LSP-HOVER-BINDERS].
    let ml = "segment row = match row\n    [x, y, ctrlAX, ..._rest] => x + y + ctrlAX\n    [x, ...tail] => x\n    _ => 0.0\n\nlet s = segment [1.0, 2.0, 3.0]\n";
    for (line, needle, expected) in [
        (1, "x, y", "x : float"),
        (1, "y, ctrlAX", "y : float"),
        (1, "ctrlAX, ...", "ctrlAX : float"),
        (1, "ctrlAX\n", "ctrlAX : float"),
        (1, "_rest", "_rest : List<float>"),
        (2, "tail", "tail : List<float>"),
        (2, "x\n", "x : float"),
    ] {
        let md = binder_hover(ml, "file:///logo.ospml", line, needle);
        crate::testkit::shows(&md, &[expected, "```osprey-ml"]);
    }

    // A tuple-style ML parameter list is a pattern over one argument, so its
    // names are binders too (#372).
    let tupled = "total (left, right) = satAdd (left, right)\n\nlet t = total (1, 2)\n";
    for (needle, expected) in [
        ("left, right) =", "left : int"),
        ("right)\n", "right : int"),
    ] {
        let md = binder_hover(tupled, "file:///total.ospml", 0, needle);
        crate::testkit::shows(&md, &[expected, "```osprey-ml"]);
    }

    // Constructor fields, structural rows and tuple slots bind the same way,
    // and a name bound in two arms answers with the arm under the cursor.
    let default = "type Shape = Circle { radius: float } | Named { label: string }\ntype Pair = Pair(int, int)\nfn describe(shape, pair: Pair) = match shape {\n  Circle { radius } => toString(radius)\n  Named { label } => match pair {\n    (label, count) => \"${label}${count}\"\n  }\n}\nfn apply(f, v) = f(v)\nlet d = describe(Circle { radius: 1.0 }, Pair(2, 3))\nlet kept = apply(|item| => [item], 2)\nlet joined = |left, right| => left + right + \"!\"\nlet j = joined(\"a\", \"b\")\n";
    for (line, needle, expected) in [
        (3, "radius }", "radius: float"),
        (3, "radius)", "radius: float"),
        (4, "label }", "label: string"),
        (5, "label, count", "label: int"),
        (5, "count)", "count: int"),
        (5, "label}", "label: int"),
        (5, "count}", "count: int"),
        (10, "item|", "item: int"),
        (10, "item]", "item: int"),
        (11, "left,", "left: string"),
        (11, "right|", "right: string"),
        (11, "left +", "left: string"),
        (11, "right +", "right: string"),
    ] {
        let md = binder_hover(default, "file:///shapes.osp", line, needle);
        crate::testkit::shows(&md, &[expected, "```osprey"]);
    }

    // A binder is in scope only inside its own arm: `head` must not answer
    // for the same word written after the match.
    let stray = "fn pick(xs) = match xs {\n  [head, ...tail] => head\n  _ => 0\n}\nlet n = pick([1])\n// head\n";
    assert!(hover(stray, "file:///a.osp", 5, 4, U16).is_none());
}

/// Hover over the LAST `needle` on 0-based `line`; a needle ending in a
/// newline anchors on the end of the line.
fn binder_hover(src: &str, uri: &str, line: usize, needle: &str) -> String {
    let needle = needle.trim_end_matches('\n');
    let at = src.lines().nth(line).and_then(|text| text.rfind(needle));
    let col = u32::try_from(at.expect("needle on line")).expect("column fits") + 1;
    hover(src, uri, u32::try_from(line).expect("line fits"), col, U16)
        .unwrap_or_else(|| panic!("no hover for `{needle}` on line {line} of {uri}"))
}
