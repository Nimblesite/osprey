//! Feature computations over a document's source text.
//!
//! Each entry point parses with [`osprey_syntax`] and answers one editor
//! feature, returning the neutral [`crate::model`] types the server maps to the
//! wire protocol. Navigation is AST-driven (declarations carry real positions);
//! find-references falls back to whole-word scanning for occurrences.

use lspkit_vfs::PositionEncoding;

use osprey_ast::Program;

use crate::analysis::{collect_inferred_symbols, collect_symbols, SymbolInfo, SymbolKind};
use crate::mlrender;
use crate::model::{Location, SignatureInfo, Span};
use crate::text::{occurrences, path_at, prefix_to, Occurrence};
use crate::workspace;

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
pub(crate) fn definition(
    text: &str,
    uri: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
    project: &workspace::View,
) -> Vec<Location> {
    let siblings = &project.siblings;
    let Some(word) = word_under(text, line, character, enc) else {
        return Vec::new();
    };
    let local: Vec<Location> = declarations(text, &project.program(uri, text), &word, enc)
        .into_iter()
        .map(|o| located(uri, (o.line, o.start, o.line, o.end)))
        .collect();
    if !local.is_empty() {
        return local;
    }
    // Nothing in this buffer declares it, so the declaration is either in a
    // sibling file or nowhere. Implements [LSP-WORKSPACE].
    let project = project_locations(siblings, &word, enc, Scan::Declarations);
    if !project.is_empty() {
        return project;
    }
    // A documented built-in has no source declaration to navigate to. Anchor it
    // on the identifier under the cursor — a graceful self-definition — so the
    // editor keeps a real, hoverable function navigable instead of reporting
    // "No definition found". Implements [LSP-DEFINITION-BUILTIN].
    builtin_definition(text, uri, line, character, enc, &word)
}

/// The identifier under the cursor as its own definition, when `word` names a
/// documented built-in. Built-ins live in the runtime, not in any `.osp` file,
/// so there is nowhere else to send the editor. Implements
/// [LSP-DEFINITION-BUILTIN].
fn builtin_definition(
    text: &str,
    uri: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
    word: &str,
) -> Vec<Location> {
    let leaf = word.rsplit("::").next().unwrap_or(word);
    if crate::analysis::builtin_hover(leaf).is_none() {
        return Vec::new();
    }
    match path_at(nth_line(text, line).unwrap_or_default(), character, enc) {
        Some(span) => vec![located(uri, (line, span.start, line, span.end))],
        None => Vec::new(),
    }
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
fn project_locations(
    siblings: &[workspace::Sibling],
    word: &str,
    enc: PositionEncoding,
    scan: Scan,
) -> Vec<Location> {
    siblings
        .iter()
        .flat_map(|sibling| sibling_locations(sibling, word, enc, scan))
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
pub(crate) fn references(
    text: &str,
    uri: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
    include_declaration: bool,
    project: &workspace::View,
) -> Vec<Location> {
    let siblings = &project.siblings;
    let Some(word) = word_under(text, line, character, enc) else {
        return Vec::new();
    };
    let declarations = declarations(text, &project.program(uri, text), &word, enc);
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
    found.extend(project_locations(siblings, &word, enc, scan));
    found
}

/// Signature help for the call enclosing `(line, character)`.
#[must_use]
pub(crate) fn signature_help(
    text: &str,
    path: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
    project: &workspace::View,
) -> Option<SignatureInfo> {
    let siblings = &project.siblings;
    let flavor = project.flavor(path, text);
    let program = project.program(path, text);
    let lookup =
        |name: &str| function_named(&program, name).or_else(|| project_function(siblings, name));
    let pointed = word_under(text, line, character, enc)
        .and_then(|name| lookup(&name))
        .map(|symbol| (symbol, 0));
    let (sym, active) = pointed.or_else(|| {
        let (name, active) = call_target(text, line, character, enc)?;
        Some((lookup(&name)?, active))
    })?;
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

/// The function `name` refers to, carrying the types the checker proved — this
/// answers signature help, which SHOWS those types ([`collect_inferred_symbols`]).
fn function_named(program: &Program, name: &str) -> Option<SymbolInfo> {
    collect_inferred_symbols(program)
        .into_iter()
        .find(|s| symbol_matches(s, name) && s.kind == SymbolKind::Function)
}

/// A function declared in a sibling file. Implements [LSP-WORKSPACE].
fn project_function(siblings: &[workspace::Sibling], name: &str) -> Option<SymbolInfo> {
    siblings
        .iter()
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
fn declarations(
    text: &str,
    program: &Program,
    name: &str,
    enc: PositionEncoding,
) -> Vec<Occurrence> {
    collect_symbols(program)
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
mod tests;
