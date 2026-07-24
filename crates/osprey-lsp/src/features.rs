//! Feature computations over a document's source text.
//!
//! Each entry point parses with [`osprey_syntax`] and answers one editor
//! feature, returning the neutral [`crate::model`] types the server maps to the
//! wire protocol. Navigation is AST-driven (declarations carry real positions);
//! find-references falls back to whole-word scanning for occurrences.

use lspkit_vfs::PositionEncoding;

use osprey_ast::Program;

use crate::analysis::{collect_symbols, SymbolInfo, SymbolKind};
use crate::mlrender;
use crate::model::{Location, SignatureInfo, Span};
use crate::text::{occurrences, path_at, prefix_to, Occurrence};
use crate::workspace;
use osprey_syntax::Flavor;

/// The flavor a document is authored in, by the full [FLAVOR-SELECT] precedence
/// (marker > extension > Default) — the same chain the CLI and the diagnostics
/// bus use, so one file is never read two ways. A conflict is reported as a
/// diagnostic, so the features here fall back rather than refusing to answer.
pub(crate) fn flavor_of(path: &str, text: &str) -> Flavor {
    osprey_syntax::resolve_flavor(None, path, text).unwrap_or(Flavor::Default)
}

/// The declaration of `word` in scope at `line` (0-based): the binding declared
/// at or before the cursor and nearest to it (innermost/most recent), else the
/// first match — resolving local shadowing without a full scope walk.
pub(crate) fn best_match<'a>(
    symbols: &'a [SymbolInfo],
    word: &str,
    line: u32,
) -> Option<&'a SymbolInfo> {
    let cursor = line.saturating_add(1); // AST positions are 1-based lines.
    let matches = || symbols.iter().filter(|symbol| symbol_matches(symbol, word));
    matches()
        .filter(|s| s.position.is_some_and(|p| p.line <= cursor))
        .max_by_key(|s| s.position.map_or(0, |p| p.line))
        .or_else(|| matches().next())
}

pub(crate) fn symbol_matches(symbol: &SymbolInfo, query: &str) -> bool {
    symbol.name == query
        || symbol.source_name == query
        || (query.contains("::")
            && symbol
                .name
                .strip_suffix(query)
                .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with("::")))
}

/// Definition location(s) for the identifier at `(line, character)`.
#[must_use]
pub fn definition(
    text: &str,
    uri: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
) -> Vec<Location> {
    let Some(word) = word_under(text, line, character, enc) else {
        return Vec::new();
    };
    let local: Vec<Location> = declarations(text, uri, &word, enc)
        .into_iter()
        .map(|o| located(uri, (o.line, o.start, o.line, o.end)))
        .collect();
    if local.is_empty() {
        // Nothing in this buffer declares it, so the declaration is either in a
        // sibling file or nowhere. Implements [LSP-WORKSPACE].
        return project_locations(uri, &word, enc, Scan::Declarations);
    }
    local
}

/// Which occurrences of a name a cross-file scan reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scan {
    /// Only the lines that declare it — go-to-definition.
    Declarations,
    /// Only textual uses — find-references with the declaration excluded.
    Uses,
    /// Both. A declaring file spells the name **unqualified** (`openSql`)
    /// while its callers write the qualified path (`Ledger::openSql`), so a
    /// whole-word scan alone never reaches the declaration.
    UsesAndDeclarations,
}

/// Locations of `word` in every sibling file of the project that claims `uri`.
/// Implements [LSP-WORKSPACE].
fn project_locations(uri: &str, word: &str, enc: PositionEncoding, scan: Scan) -> Vec<Location> {
    workspace::siblings(uri)
        .into_iter()
        .flat_map(|sibling| sibling_locations(&sibling, word, enc, scan))
        .collect()
}

fn sibling_locations(
    sibling: &workspace::Sibling,
    word: &str,
    enc: PositionEncoding,
    scan: Scan,
) -> Vec<Location> {
    let mut found: Vec<Occurrence> = match scan {
        Scan::Declarations => Vec::new(),
        Scan::Uses | Scan::UsesAndDeclarations => occurrences(&sibling.source, word, enc),
    };
    if scan != Scan::Uses {
        for declaration in sibling_declarations(sibling, word, enc) {
            if !found.iter().any(|o| o.line == declaration.line) {
                found.push(declaration);
            }
        }
    }
    found
        .into_iter()
        .map(|o| located(&sibling.uri, (o.line, o.start, o.line, o.end)))
        .collect()
}

fn sibling_declarations(
    sibling: &workspace::Sibling,
    word: &str,
    enc: PositionEncoding,
) -> Vec<Occurrence> {
    collect_symbols(&sibling.program)
        .iter()
        .filter(|symbol| symbol_matches(symbol, word))
        .filter_map(|symbol| declaration_occurrence(&sibling.source, symbol, enc))
        .collect()
}

/// All references to the identifier at `(line, character)`.
#[must_use]
pub fn references(
    text: &str,
    uri: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
    include_declaration: bool,
) -> Vec<Location> {
    let Some(word) = word_under(text, line, character, enc) else {
        return Vec::new();
    };
    let declarations = declarations(text, uri, &word, enc);
    let decls: Vec<(u32, u32)> = declarations.iter().map(|o| (o.line, o.start)).collect();
    let mut found: Vec<Location> = occurrences(text, &word, enc)
        .into_iter()
        .filter(|o| include_declaration || !decls.contains(&(o.line, o.start)))
        .map(|o| located(uri, (o.line, o.start, o.line, o.end)))
        .collect();
    if include_declaration {
        for declaration in declarations {
            let location = located(
                uri,
                (
                    declaration.line,
                    declaration.start,
                    declaration.line,
                    declaration.end,
                ),
            );
            if !found.contains(&location) {
                found.push(location);
            }
        }
    }
    // A symbol used across a project is referenced across it too, so the scan
    // does not stop at the open buffer. Implements [LSP-WORKSPACE].
    let scan = if include_declaration {
        Scan::UsesAndDeclarations
    } else {
        Scan::Uses
    };
    found.extend(project_locations(uri, &word, enc, scan));
    found
}

/// Signature help for the call enclosing `(line, character)`.
#[must_use]
pub fn signature_help(
    text: &str,
    path: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
) -> Option<SignatureInfo> {
    let (name, active) = call_target(text, line, character, enc)?;
    let flavor = flavor_of(path, text);
    let parsed = osprey_syntax::parse_program_with_flavor(text, flavor);
    let sym = function_named(&parsed.program, &name).or_else(|| project_function(path, &name))?;
    let params: Vec<String> = sym.parameters.iter().map(param_label).collect();
    let last = u32::try_from(params.len().saturating_sub(1)).unwrap_or(0);
    Some(SignatureInfo {
        label: mlrender::signature(flavor, &sym.signature.unwrap_or(sym.name)),
        parameters: params,
        active_parameter: active.min(last),
    })
}

/// The call signature help is about: the innermost still-open call, or — when
/// none is open — the function name the cursor sits on. Editors ask for help
/// the moment the callee is typed, before its `(` exists, and answering only
/// inside the parentheses means the signature appears just after it stopped
/// being useful. A non-function word simply finds no signature.
fn call_target(
    text: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
) -> Option<(String, u32)> {
    let line_str = nth_line(text, line)?;
    enclosing_call(prefix_to(line_str, character, enc))
        .or_else(|| word_under(text, line, character, enc).map(|word| (word, 0)))
}

fn function_named(program: &Program, name: &str) -> Option<SymbolInfo> {
    collect_symbols(program)
        .into_iter()
        .find(|s| symbol_matches(s, name) && s.kind == SymbolKind::Function)
}

/// A function declared in a sibling file. Implements [LSP-WORKSPACE].
fn project_function(path: &str, name: &str) -> Option<SymbolInfo> {
    workspace::siblings(path)
        .into_iter()
        .find_map(|sibling| function_named(&sibling.program, name))
}

pub(crate) fn word_under(
    text: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
) -> Option<String> {
    path_at(nth_line(text, line)?, character, enc).map(|word| word.word)
}

pub(crate) fn nth_line(text: &str, line: u32) -> Option<&str> {
    usize::try_from(line).ok().and_then(|i| text.lines().nth(i))
}

fn located(uri: &str, span: Span) -> Location {
    Location {
        uri: uri.to_owned(),
        span,
    }
}

/// The identifier occurrence of each declaration of `name`.
///
/// A declaration's recorded position points at its keyword (`fn`/`type`/`let`),
/// not the name, so this finds the first whole-word occurrence of `name` on each
/// declaration line — the location editors expect for go-to-definition.
fn declarations(text: &str, path: &str, name: &str, enc: PositionEncoding) -> Vec<Occurrence> {
    let parsed = osprey_syntax::parse_program_for_path(path, text);
    collect_symbols(&parsed.program)
        .iter()
        .filter(|symbol| symbol_matches(symbol, name))
        .filter_map(|symbol| declaration_occurrence(text, symbol, enc))
        .collect()
}

fn declaration_occurrence(
    text: &str,
    symbol: &SymbolInfo,
    enc: PositionEncoding,
) -> Option<Occurrence> {
    let position = symbol.position?;
    let line = position.line.saturating_sub(1);
    occurrences(text, &symbol.source_name, enc)
        .into_iter()
        .find(|occurrence| occurrence.line == line)
        .or_else(|| {
            let end = position
                .column
                .saturating_add(crate::text::measure(&symbol.source_name, enc));
            Some(Occurrence {
                line,
                start: position.column,
                end,
            })
        })
}

fn param_label((name, ty): &(String, String)) -> String {
    if ty.is_empty() {
        name.clone()
    } else {
        format!("{name}: {ty}")
    }
}

/// Parse `before` (the line text up to the cursor) and return the name of the
/// innermost still-open call and the active (comma-separated) argument index.
///
/// String literals and `//` line comments are skipped so their `(`, `)` and `,`
/// do not corrupt the call/comma stacks.
fn enclosing_call(before: &str) -> Option<(String, u32)> {
    let mut names: Vec<String> = Vec::new();
    let mut commas: Vec<u32> = Vec::new();
    let mut current = String::new();
    let mut last = String::new();
    let mut in_string = false;
    let mut escaped = false;
    let mut chars = before.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        if c == '/' && chars.peek() == Some(&'/') {
            break;
        }
        if c.is_alphanumeric() || c == '_' {
            current.push(c);
            continue;
        }
        if !current.is_empty() {
            last = std::mem::take(&mut current);
        }
        if c == '"' {
            in_string = true;
        } else {
            step_call(c, &mut names, &mut commas, &mut last);
        }
    }
    let name = names.last().filter(|n| !n.is_empty())?;
    Some((name.clone(), commas.last().copied().unwrap_or(0)))
}

fn step_call(c: char, names: &mut Vec<String>, commas: &mut Vec<u32>, last: &mut String) {
    match c {
        '(' => {
            names.push(std::mem::take(last));
            commas.push(0);
        }
        ')' => {
            let _ = names.pop();
            let _ = commas.pop();
        }
        ',' => {
            if let Some(top) = commas.last_mut() {
                *top = top.saturating_add(1);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hover::hover;
    const U16: PositionEncoding = PositionEncoding::Utf16;
    const SRC: &str = "fn add(a: int, b: int) -> int = a + b\nlet total = add(1, 2)\n";

    #[test]
    fn definition_points_at_the_declaration() {
        let defs = definition(SRC, "file:///a.osp", 1, 12, U16);
        let first = defs.first().expect("definition");
        assert_eq!(first.span.0, 0, "{defs:?}");
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

    /// The 0-based column just inside the first occurrence of `needle` on
    /// 0-based `line` of `src` — a cursor position over that word.
    fn col_of(src: &str, line: usize, needle: &str) -> u32 {
        let text = src.lines().nth(line).expect("line exists");
        let at = text.find(needle).expect("needle on line");
        u32::try_from(at).expect("column fits") + 1
    }

    #[test]
    fn definition_and_references_return_empty_off_any_identifier() {
        // A two-space gap guarantees a column that is over neither word.
        let src = "let a  =  b\n";
        // Column 6 sits in the double space between `a` and `=`.
        assert!(definition(src, "file:///a.osp", 0, 6, U16).is_empty());
        assert!(references(src, "file:///a.osp", 0, 6, U16, true).is_empty());
        // A line past the end of the file yields no word either.
        assert!(hover(src, "file:///a.osp", 99, 0, U16).is_none());
    }

    #[test]
    fn signature_help_labels_unannotated_parameters_by_name_only() {
        // The parameter has no type annotation, so its label is the bare name.
        let src = "fn id(x) = x\nlet y = id(7)\n";
        let sig = signature_help(src, "file:///a.osp", 1, 11, U16).expect("sig");
        assert_eq!(sig.parameters, vec!["x".to_owned()]);
        assert_eq!(sig.active_parameter, 0);
    }

    #[test]
    fn signature_help_unwinds_a_closed_inner_call() {
        // The inner `add(1, 2)` call is closed before the cursor, so the active
        // call is the still-open outer `print(...)`. This exercises the `)` arm
        // that pops the call/comma stacks.
        let src = "fn add(a: int, b: int) -> int = a + b\nlet r = add(add(1, 2), 3)\n";
        let sig = signature_help(src, "file:///a.osp", 1, 24, U16).expect("sig");
        assert_eq!(sig.label, "fn add(a: int, b: int) -> int");
        // After the inner call closed, the cursor is over the outer second arg.
        assert_eq!(sig.active_parameter, 1, "{sig:?}");
    }

    #[test]
    fn signature_help_triggers_on_the_function_name_not_only_inside_the_parens() {
        // Editors ask the moment the callee is typed. Answering only between
        // the parentheses shows the signature just after it stopped helping.
        let src = "fn add(a: int, b: int) -> int = a + b\nlet total = add\n";
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
        // `Ledger::openSql` in the composition root said nothing and
        // go-to-definition went nowhere, even though the compiler links both
        // files into one program. Implements [LSP-WORKSPACE].
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/projects/modules");
        let entry = root.join("src/main.ospml");
        let text = std::fs::read_to_string(&entry).expect("read the composition root");
        let uri = format!("file://{}", entry.display());
        let (line, column) = position_of(&text, "Ledger::openSql");

        let hovered = hover(&text, &uri, line, column, U16).expect("cross-file hover");
        assert!(hovered.contains("openSql"), "{hovered}");

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
}
