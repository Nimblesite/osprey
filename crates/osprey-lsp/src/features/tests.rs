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
    let found: Vec<_> = references(src, uri, 0, col_of(src, 0, "x)"), U16, true)
        .into_iter()
        .map(|location| location.span)
        .collect();
    assert_eq!(
        found,
        vec![word_span(src, (0, "x)")), word_span(src, (5, "x"))]
    );
    let inner: Vec<_> = references(src, uri, 3, col_of(src, 3, "x"), U16, true)
        .into_iter()
        .map(|location| location.span)
        .collect();
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
        let found = references(src, uri, 0, col_of(src, 0, "x"), U16, true);
        assert_eq!(
            found
                .into_iter()
                .map(|location| location.span)
                .collect::<Vec<_>>(),
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
        assert_eq!(references(src, "file:///nested.osp", u32::try_from(outer).expect("line fits"),
            col_of(src, outer, "x)"), U16, true).len(), 2);
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
        references(
            src,
            "file:///nested.ospml",
            0,
            col_of(src, 0, "x"),
            U16,
            true
        )
        .len(),
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
    let defs = definition(SRC, "file:///a.osp", 1, 12, U16);
    let first = defs.first().expect("definition");
    assert_eq!(first.span.0, 0, "{defs:?}");
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
    let defs = definition(src, "file:///a.osp", 0, col, U16);
    let first = defs
        .first()
        .expect("built-in resolves to its own identifier");
    assert_eq!(first.span.0, 0, "on the same line: {defs:?}");
    assert_eq!(
        first.span.1,
        col - 1,
        "anchored at the identifier: {defs:?}"
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
fn signature_help_tracks_the_active_parameter() {
    // Line 1 is `let total = add(1, 2)`; char 19 is over the second argument.
    let sig = signature_help(SRC, "file:///a.osp", 1, 19, U16).expect("sig");
    assert_eq!(sig.active_parameter, 1, "{sig:?}");
    assert_eq!(sig.parameters.len(), 2);
}

#[test]
fn signature_help_ignores_commas_inside_strings() {
    // The commas inside the string literal must not advance the active param.
    let src = "fn f(a: int, b: int) -> int = a\nlet x = f(\"a, b, c\", 2)\n";
    let sig = signature_help(src, "file:///a.osp", 1, 21, U16).expect("sig");
    assert_eq!(sig.active_parameter, 1, "{sig:?}");
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

    let definitions = definition(src, "file:///modules.osp", 6, column, U16);
    assert_eq!(definitions.len(), 1, "{definitions:?}");
    assert_eq!(
        definitions.first().map(|location| location.span.0),
        Some(1),
        "sales declaration line"
    );

    let references = references(src, "file:///modules.osp", 6, column, U16, true);
    assert_eq!(references.len(), 2, "use plus declaration: {references:?}");
    assert!(!references.iter().any(|location| location.span.0 == 4));
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
    let cursor = col_of(SCOPED_DEFAULT, 2, "work()");
    let mut with: Vec<_> = references(SCOPED_DEFAULT, uri, 2, cursor, U16, true)
        .into_iter()
        .map(|location| location.span)
        .collect();
    with.sort_unstable();
    let expected: Vec<_> = [(1, "work)"), (2, "work()"), (6, "work()")]
        .into_iter()
        .map(|word| word_span(SCOPED_DEFAULT, word))
        .collect();
    assert_eq!(with, expected);
    assert_eq!(
        references(SCOPED_DEFAULT, uri, 2, cursor, U16, false).len(),
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
    let sig = signature_help(src, "file:///a.osp", 1, 11, U16).expect("sig");
    assert_eq!(sig.parameters, vec!["x".to_owned()]);
    assert_eq!(sig.active_parameter, 0);

    // But a parameter the checker DID prove must carry its type, exactly as
    // the outline and hover do. Signature help read raw `collect_symbols`,
    // so it never consulted inference: deleting the inferable `: int` that
    // CLAUDE.md requires deleting silently downgraded the help to `n`.
    // Implements [LSP-HOVER-INFERRED-SIGNATURE].
    let proved = "fn twice(n) = n * 2\nlet y = twice(7)\n";
    let sig = signature_help(proved, "file:///b.osp", 1, 14, U16).expect("sig");
    assert_eq!(sig.parameters, vec!["n: int".to_owned()]);
    // The annotated spelling is the control: tooling must not be able to
    // tell the two apart.
    let annotated = "fn twice(n: int) = n * 2\nlet y = twice(7)\n";
    let control = signature_help(annotated, "file:///c.osp", 1, 14, U16).expect("sig");
    assert_eq!(sig.parameters, control.parameters);
    assert_eq!(sig.label, control.label);
}

#[test]
fn signature_help_unwinds_a_closed_inner_call() {
    // The inner `add(1, 2)` call is closed before the cursor, so the active
    // call is the still-open outer `print(...)`. This exercises the `)` arm
    // that pops the call/comma stacks.
    let src = "fn add(a: int, b: int) -> int = (a + b) ?: 0\nlet r = add(add(1, 2), 3)\n";
    let sig = signature_help(src, "file:///a.osp", 1, 24, U16).expect("sig");
    assert_eq!(sig.label, "fn add(a: int, b: int) -> int");
    // After the inner call closed, the cursor is over the outer second arg.
    assert_eq!(sig.active_parameter, 1, "{sig:?}");
}

#[test]
fn signature_help_triggers_on_the_function_name_not_only_inside_the_parens() {
    // Editors ask the moment the callee is typed. Answering only between
    // the parentheses shows the signature just after it stopped helping.
    let src = "fn add(a: int, b: int) -> int = (a + b) ?: 0\nlet total = add\n";
    let sig = signature_help(src, "file:///a.osp", 1, 13, U16).expect("sig on the name");
    assert_eq!(sig.label, "fn add(a: int, b: int) -> int");
    assert_eq!(sig.active_parameter, 0, "{sig:?}");
    // A word that names no function still yields nothing — `total` is a
    // binding, and offering it a parameter list would be a lie.
    assert!(signature_help(src, "file:///a.osp", 1, 5, U16).is_none());
}

#[cfg(unix)]
#[test]
fn a_symbol_declared_in_a_sibling_file_resolves_across_the_project() {
    // Single-file analysis made every imported symbol invisible: hovering
    // `Ledger::sqlite` in the composition root said nothing and
    // go-to-definition went nowhere, even though the compiler links both
    // files into one program. Implements [LSP-WORKSPACE].
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/projects/modules");
    let entry = root.join("src/main.ospml");
    let text = std::fs::read_to_string(&entry).expect("read the composition root");
    let uri = format!("file://{}", entry.display());
    let (line, column) = position_of(&text, "Ledger::sqlite");

    let hovered = hover(&text, &uri, line, column, U16).expect("cross-file hover");
    crate::testkit::shows(&hovered, &["sqlite"]);

    let definitions = definition(&text, &uri, line, column, U16);
    let first = definitions.first().expect("cross-file definition");
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

#[test]
fn signature_help_skips_commas_in_strings_and_line_comments() {
    // The escaped quote and the `//` comment must not corrupt the comma/call
    // tracking, so the active parameter stays at the first argument.
    let src = "fn f(a: int, b: int) -> int = a\nlet x = f(\"a\\\"b\" // c, d\n";
    let sig = signature_help(src, "file:///a.osp", 1, 23, U16).expect("sig");
    assert_eq!(sig.active_parameter, 0, "{sig:?}");
}
