use super::*;
use crate::test_support::col_of;
use crate::test_support::definition;
use crate::test_support::hover;
use crate::test_support::references;
use crate::test_support::signature_help;
const U16: PositionEncoding = PositionEncoding::Utf16;
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
    assert!(hover.contains("fn rate() -> int"), "{hover}");

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
    assert!(hovered.contains("sqlite"), "{hovered}");

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
