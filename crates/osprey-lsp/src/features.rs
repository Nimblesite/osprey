//! Feature computations over a document's source text.
//!
//! Each entry point parses with [`osprey_syntax`] and answers one editor
//! feature, returning the neutral [`crate::model`] types the server maps to the
//! wire protocol. Navigation is AST-driven (declarations carry real positions);
//! find-references falls back to whole-word scanning for occurrences.

use lspkit_vfs::PositionEncoding;

use osprey_ast::Program;

use crate::analysis::{
    builtin_hover, collect_all_symbols, collect_symbols, SymbolInfo, SymbolKind,
};
use crate::model::{CompletionItem, CompletionKind, Location, SignatureInfo, Span};
use crate::text::{occurrences, path_at, prefix_to, Occurrence};

/// Hover markdown for the identifier at `(line, character)`: the symbol's
/// signature, or `name: type` for a binding — inferring an unannotated `let`'s
/// type from the checker — followed by its `///` documentation. Built-ins fall
/// back to their reference docs. Implements [LSP-HOVER], [LSP-HOVER-VARIABLES],
/// [LSP-HOVER-DOCS]
#[must_use]
pub fn hover(
    text: &str,
    path: &str,
    line: u32,
    character: u32,
    enc: PositionEncoding,
) -> Option<String> {
    let word = word_under(text, line, character, enc)?;
    let parsed = osprey_syntax::parse_program_for_path(path, text);
    let symbols = collect_all_symbols(&parsed.program);
    // A `[Symbol]` intra-doc link under the cursor resolves to the referenced
    // element's own hover — the whole dotted target (`Effect.op`), not just the
    // sub-word the cursor happens to sit on ([DOC-LINK]).
    if let Some(target) = doc_link_target(text, line, character) {
        if let Some(hov) = resolve_link(&symbols, &target, &parsed.program) {
            return Some(hov);
        }
    }
    match best_match(&symbols, &word, line) {
        Some(sym) => Some(symbol_hover(sym, &parsed.program)),
        None => builtin_hover(word.rsplit("::").next().unwrap_or(&word)),
    }
}

/// The `[Symbol]` link the cursor sits inside on `line`, if any: the bracketed
/// content when the cursor is between a `[` and its matching `]` and the
/// content is a dotted identifier (not a `[text](url)` markdown link).
/// Implements [DOC-LINK].
fn doc_link_target(text: &str, line: u32, character: u32) -> Option<String> {
    let src = nth_line(text, line)?;
    let col = usize::try_from(character).ok()?;
    let open = src.get(..col)?.rfind('[')?;
    let close_rel = src.get(open + 1..)?.find(']')?;
    let close = open + 1 + close_rel;
    if col > close {
        return None;
    }
    let inner = src.get(open + 1..close)?;
    let followed_by_paren = src.get(close + 1..).and_then(|s| s.chars().next()) == Some('(');
    let dotted = !inner.is_empty()
        && !inner.contains(char::is_whitespace)
        && inner
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':')
        && inner.chars().next().is_some_and(char::is_alphabetic);
    (dotted && !followed_by_paren).then(|| inner.to_string())
}

/// Resolve a `[Symbol]` link target to its hover: a bare name resolves to its
/// declaration or a builtin; a dotted `Effect.op` / `Type.variant` resolves to
/// the owner declaration's hover. Implements [DOC-LINK].
fn resolve_link(symbols: &[SymbolInfo], target: &str, program: &Program) -> Option<String> {
    let head = target
        .split(['.', ':'])
        .find(|segment| !segment.is_empty())
        .unwrap_or(target);
    symbols
        .iter()
        .find(|symbol| symbol_matches(symbol, head))
        .map(|s| symbol_hover(s, program))
        .or_else(|| builtin_hover(head))
}

/// The declaration of `word` in scope at `line` (0-based): the binding declared
/// at or before the cursor and nearest to it (innermost/most recent), else the
/// first match — resolving local shadowing without a full scope walk.
fn best_match<'a>(symbols: &'a [SymbolInfo], word: &str, line: u32) -> Option<&'a SymbolInfo> {
    let cursor = line.saturating_add(1); // AST positions are 1-based lines.
    let matches = || symbols.iter().filter(|symbol| symbol_matches(symbol, word));
    matches()
        .filter(|s| s.position.is_some_and(|p| p.line <= cursor))
        .max_by_key(|s| s.position.map_or(0, |p| p.line))
        .or_else(|| matches().next())
}

fn symbol_matches(symbol: &SymbolInfo, query: &str) -> bool {
    symbol.name == query
        || symbol.source_name == query
        || (query.contains("::")
            && symbol
                .name
                .strip_suffix(query)
                .is_some_and(|prefix| prefix.is_empty() || prefix.ends_with("::")))
}

/// Render `s` as hover markdown: a code-fenced signature/type, then its docs.
fn symbol_hover(s: &SymbolInfo, program: &Program) -> String {
    let code = match (s.kind, &s.signature) {
        (_, Some(sig)) => sig.clone(),
        (SymbolKind::Namespace | SymbolKind::Module | SymbolKind::Signature, None) => {
            format!("{} {}", s.kind.as_str(), s.name)
        }
        (_, None) => format!("{}: {}", s.name, displayed_type(s, program)),
    };
    let mut out = format!("```osprey\n{code}\n```");
    if let Some(doc) = &s.doc {
        out.push_str("\n\n");
        out.push_str(doc);
    }
    out
}

/// The type shown for a non-function symbol: its declared/category type, or —
/// for an unannotated `let` — the type the checker inferred for that binding.
/// Implements [LSP-HOVER-VARIABLES]
fn displayed_type(s: &SymbolInfo, program: &Program) -> String {
    if !s.ty.is_empty() {
        return s.ty.clone();
    }
    osprey_types::infer_program(program)
        .let_type(s.position)
        .map_or_else(String::new, ToString::to_string)
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
    declarations(text, uri, &word, enc)
        .into_iter()
        .map(|o| located(uri, (o.line, o.start, o.line, o.end)))
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
    let line_str = nth_line(text, line)?;
    let (name, active) = enclosing_call(prefix_to(line_str, character, enc))?;
    let parsed = osprey_syntax::parse_program_for_path(path, text);
    let sym = collect_symbols(&parsed.program)
        .into_iter()
        .find(|s| symbol_matches(s, &name) && s.kind == SymbolKind::Function)?;
    let params: Vec<String> = sym.parameters.iter().map(param_label).collect();
    let last = u32::try_from(params.len().saturating_sub(1)).unwrap_or(0);
    Some(SignatureInfo {
        label: sym.signature.unwrap_or(sym.name),
        parameters: params,
        active_parameter: active.min(last),
    })
}

/// Completion items: keywords plus the document's own declarations.
#[must_use]
pub fn completion(text: &str, path: &str) -> Vec<CompletionItem> {
    let parsed = osprey_syntax::parse_program_for_path(path, text);
    keyword_items(path)
        .into_iter()
        .chain(collect_symbols(&parsed.program).iter().map(symbol_item))
        .collect()
}

fn word_under(text: &str, line: u32, character: u32, enc: PositionEncoding) -> Option<String> {
    path_at(nth_line(text, line)?, character, enc).map(|word| word.word)
}

fn nth_line(text: &str, line: u32) -> Option<&str> {
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

fn symbol_item(s: &SymbolInfo) -> CompletionItem {
    let kind = match s.kind {
        SymbolKind::Function => CompletionKind::Function,
        SymbolKind::Variable => CompletionKind::Variable,
        SymbolKind::Type | SymbolKind::Namespace | SymbolKind::Module | SymbolKind::Signature => {
            CompletionKind::Type
        }
    };
    CompletionItem {
        label: s.name.clone(),
        kind,
        detail: Some(s.ty.clone()),
        insert_text: None,
    }
}

/// The fixed keyword/snippet completions (superset of the old TS server's six).
fn keyword_items(path: &str) -> Vec<CompletionItem> {
    const BASE: [(&str, &str, &str); 7] = [
        (
            "if",
            "Conditional expression [GRAMMAR-IF-ELSE]",
            "if ${1:condition} { ${2:then} } else { ${3:else} }",
        ),
        (
            "fn",
            "Function declaration",
            "fn ${1:name}(${2:params}) = ${3:body}",
        ),
        ("let", "Variable declaration", "let ${1:name} = ${2:value}"),
        (
            "mut",
            "Mutable variable declaration",
            "mut ${1:name} = ${2:value}",
        ),
        (
            "match",
            "Pattern matching",
            "match ${1:expr} {\n\t${2:pattern} => ${3:result}\n}",
        ),
        (
            "type",
            "Type declaration",
            "type ${1:Name} = ${2:Variant} | ${3:Variant}",
        ),
        (
            "effect",
            "Effect declaration",
            "effect ${1:Name} {\n\t${2:op}: ${3:fn() -> Unit}\n}",
        ),
    ];
    let ml = std::path::Path::new(path)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ospml"));
    let modules = module_keyword_specs(ml);
    BASE.iter()
        .chain(modules.iter())
        .map(|(label, detail, snippet)| CompletionItem {
            label: (*label).to_owned(),
            kind: CompletionKind::Keyword,
            detail: Some((*detail).to_owned()),
            insert_text: Some((*snippet).to_owned()),
        })
        .collect()
}

fn module_keyword_specs(ml: bool) -> [(&'static str, &'static str, &'static str); 8] {
    [
        (
            "namespace",
            "Logical namespace [MODULES-NAMESPACE]",
            if ml {
                "namespace ${1:name}"
            } else {
                "namespace ${1:name};"
            },
        ),
        (
            "import",
            "Namespace/module import [MODULES-IMPORT]",
            "import ${1:namespace}::${2:Module}",
        ),
        (
            "module",
            "Closed module boundary [MODULES-MODULE]",
            if ml {
                "module ${1:Name}\n\t${0}"
            } else {
                "module ${1:Name} {\n\t${0}\n}"
            },
        ),
        (
            "signature",
            "Explicit module interface [MODULES-SIGNATURE]",
            if ml {
                "signature ${1:Name}\n\t${0}"
            } else {
                "signature ${1:Name} {\n\t${0}\n}"
            },
        ),
        (
            "export",
            "Export a module declaration [MODULES-EXPORTS]",
            if ml {
                "export ${1:name} = ${2:value}"
            } else {
                "export fn ${1:name}(${2:params}) = ${3:body}"
            },
        ),
        (
            "state",
            "Durable state owner [MODULES-STATE-MODULE]",
            if ml {
                "state ${1:Name}\n\t${0}"
            } else {
                "state module ${1:Name} {\n\t${0}\n}"
            },
        ),
        (
            "opaque",
            "Opaque exported type [MODULES-OPAQUE-TYPES]",
            "opaque type ${1:Name}",
        ),
        ("as", "Import alias [MODULES-IMPORT]", "as ${1:Alias}"),
    ]
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
    const U16: PositionEncoding = PositionEncoding::Utf16;
    const SRC: &str = "fn add(a: int, b: int) -> int = a + b\nlet total = add(1, 2)\n";

    #[test]
    fn hover_uses_signature_for_functions_and_builtins() {
        assert!(hover(SRC, "file:///a.osp", 1, 12, U16)
            .is_some_and(|m| m.contains("fn add(a: int, b: int) -> int")));
        assert!(hover("fn main() = print(1)\n", "file:///a.osp", 0, 13, U16)
            .is_some_and(|m| m.contains("print")));
    }

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
    fn completion_includes_keywords_and_declarations() {
        let items = completion(SRC, "file:///a.osp");
        assert!(items
            .iter()
            .any(|i| i.label == "fn" && i.kind == CompletionKind::Keyword));
        assert!(items
            .iter()
            .any(|i| i.label == "add" && i.kind == CompletionKind::Function));
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
    fn completion_includes_qualified_symbols_and_flavor_specific_module_snippets() {
        // [MODULES-FLAVOR-PROJECTION] Both flavors expose the same concepts,
        // while insertion text stays idiomatic for the active surface.
        let src = "namespace billing { module Tax { export fn addTax(x) = x } }\n";
        let default = completion(src, "file:///billing.osp");
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

        let ml = completion("module Tax\n    x = 1\n", "file:///billing.ospml");
        assert!(ml.iter().any(|item| {
            item.label == "state"
                && item.insert_text.as_deref().is_some_and(|text| {
                    text.starts_with("state ") && !text.starts_with("state module")
                })
        }));
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
        assert!(md.contains("greeting: string"), "inferred type: {md}");
        assert!(md.contains("The greeting text."), "docs: {md}");
    }

    #[test]
    fn hover_on_a_documented_function_renders_its_docs() {
        // A `///` block above a function surfaces under its signature.
        // Implements [LSP-HOVER-DOCS]
        let src = "/// Doubles `x`.\nfn dbl(x: int) -> int = x * 2\n";
        let md = hover(src, "file:///a.osp", 1, 4, U16).expect("hover over `dbl`");
        assert!(md.contains("fn dbl(x: int) -> int"), "signature: {md}");
        assert!(md.contains("Doubles `x`."), "docs: {md}");
    }

    /// The 0-based column just inside the first occurrence of `needle` on
    /// 0-based `line` of `src` — a cursor position over that word.
    fn col_of(src: &str, line: usize, needle: &str) -> u32 {
        let text = src.lines().nth(line).expect("line exists");
        let at = text.find(needle).expect("needle on line");
        u32::try_from(at).expect("column fits") + 1
    }

    #[test]
    fn hover_on_a_doc_link_resolves_to_the_referenced_element() {
        // A `[Symbol]` intra-doc link in a comment hovers to that symbol's own
        // docs ([DOC-LINK]) — here `[helper]` on the doc line of `main`.
        let src = "/// A helper.\n\
                   fn helper(n) = n + 1\n\
                   /// Calls [helper] to do the work.\n\
                   fn main() = helper(1)\n";
        let col = col_of(src, 2, "helper");
        let md = hover(src, "file:///a.osp", 2, col, U16).expect("hover over [helper]");
        assert!(
            md.contains("fn helper(n)"),
            "resolves to helper's signature: {md}"
        );
        assert!(md.contains("A helper."), "shows helper's docs: {md}");
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
    fn completion_maps_a_type_declaration_to_the_type_kind() {
        let src = "type Shade = Light | Dark\nlet c: int = 1\n";
        let items = completion(src, "file:///a.osp");
        assert!(items
            .iter()
            .any(|i| i.label == "Shade" && i.kind == CompletionKind::Type));
        // The variable `c` is a Variable-kind completion with its detail.
        let c = items.iter().find(|i| i.label == "c").expect("c completion");
        assert_eq!(c.kind, CompletionKind::Variable);
        assert_eq!(c.detail.as_deref(), Some("int"));
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
    fn signature_help_skips_commas_in_strings_and_line_comments() {
        // The escaped quote and the `//` comment must not corrupt the comma/call
        // tracking, so the active parameter stays at the first argument.
        let src = "fn f(a: int, b: int) -> int = a\nlet x = f(\"a\\\"b\" // c, d\n";
        let sig = signature_help(src, "file:///a.osp", 1, 23, U16).expect("sig");
        assert_eq!(sig.active_parameter, 0, "{sig:?}");
    }
}
