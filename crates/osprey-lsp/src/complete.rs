//! `textDocument/completion`.
//!
//! The list is filtered twice before it reaches the editor: by **flavor**, so a
//! `.ospml` buffer is never offered a keyword the ML frontend does not have
//! ([`crate::keywords`], [LSP-FLAVOR-RENDER]), and by **position**, so a type
//! annotation is not offered the `fn` snippet and an argument slot is not
//! offered `namespace` ([`crate::context`], [LSP-COMPLETION-CONTEXT]). Symbols
//! come from the whole project, not just the open buffer
//! ([`crate::workspace`], [LSP-WORKSPACE]).

use lspkit_vfs::PositionEncoding;
use osprey_ast::Program;
use osprey_syntax::Flavor;
use osprey_types::{names, ProgramTypes, Type};

use crate::analysis::{collect_all_symbols, collect_symbols, SymbolInfo, SymbolKind};
use crate::context::{self, Cursor};
use crate::features::{best_match, flavor_of};
use crate::keywords::keyword_items;
use crate::mlrender;
use crate::model::{CompletionItem, CompletionKind};
use crate::workspace;

/// The type names every program can write without declaring them. Spelled from
/// the checker's own constants so the two can never drift.
const BUILTIN_TYPES: [&str; 8] = [
    names::INT,
    names::FLOAT,
    names::STRING,
    names::BOOL,
    names::UNIT,
    names::RESULT,
    names::LIST,
    names::MAP,
];

/// The wildcard pattern, which matches anything and binds nothing.
const WILDCARD: &str = "_";

/// Completion items for the cursor at `(line, character)`.
#[must_use]
pub fn completion(
    text: &str,
    path: &str,
    line: u32,
    character: u32,
    encoding: PositionEncoding,
) -> Vec<CompletionItem> {
    let flavor = flavor_of(path, text);
    let program = osprey_syntax::parse_program_with_flavor(text, flavor).program;
    let cursor = context::at(text, line, character, encoding);
    // A fresh binder is the author inventing a name; every suggestion is noise.
    if cursor == Cursor::Binder {
        return Vec::new();
    }
    if let Cursor::Member(receiver) = &cursor {
        return member_items(receiver, &program, line);
    }
    let symbols = visible_symbols(&program, path);
    keyword_items(flavor, &cursor)
        .into_iter()
        .chain(symbol_items(&cursor, &symbols, &program, flavor))
        .collect()
}

/// Every symbol the open document can name: its own declarations plus those of
/// every sibling file the project links with it. Implements [LSP-WORKSPACE].
fn visible_symbols(program: &Program, path: &str) -> Vec<SymbolInfo> {
    let mut symbols = collect_symbols(program);
    for sibling in workspace::siblings(path) {
        symbols.extend(collect_symbols(&sibling.program));
    }
    // First wins, so the open buffer's own declaration shadows an imported one
    // of the same qualified name. `dedup_by` would not do: duplicates land in
    // different files and are therefore never adjacent.
    let mut seen = std::collections::HashSet::new();
    symbols.retain(|symbol| seen.insert(symbol.name.clone()));
    symbols
}

/// The declared symbols legal at `cursor`.
fn symbol_items(
    cursor: &Cursor,
    symbols: &[SymbolInfo],
    program: &Program,
    flavor: Flavor,
) -> Vec<CompletionItem> {
    match cursor {
        // A written type takes type names only — a function or a binding there
        // is not merely unhelpful, it does not parse.
        Cursor::Type => type_items(symbols, flavor),
        Cursor::Pattern => pattern_items(program),
        Cursor::Value | Cursor::Declaration => {
            symbols.iter().map(|s| symbol_item(s, flavor)).collect()
        }
        Cursor::Member(_) | Cursor::Binder => Vec::new(),
    }
}

/// Declared types and effects, plus the built-in type names.
fn type_items(symbols: &[SymbolInfo], flavor: Flavor) -> Vec<CompletionItem> {
    symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Type)
        .map(|s| symbol_item(s, flavor))
        .chain(BUILTIN_TYPES.iter().map(|name| CompletionItem {
            label: (*name).to_owned(),
            kind: CompletionKind::Type,
            detail: Some(String::from("built-in type")),
            insert_text: None,
        }))
        .collect()
}

/// The constructors a `match` arm can destructure, plus the wildcard. Sorted:
/// the checker keys constructors by hash, and a completion list that reshuffles
/// between keystrokes is unusable.
fn pattern_items(program: &Program) -> Vec<CompletionItem> {
    let types = osprey_types::infer_program(program);
    let mut items: Vec<CompletionItem> = types
        .ctors
        .iter()
        .map(|(name, layout)| CompletionItem {
            label: name.clone(),
            kind: CompletionKind::Type,
            detail: Some(layout.owner.clone()),
            insert_text: None,
        })
        .collect();
    items.sort_by(|left, right| left.label.cmp(&right.label));
    items.push(CompletionItem {
        label: WILDCARD.to_owned(),
        kind: CompletionKind::Keyword,
        detail: Some(String::from("Wildcard pattern")),
        insert_text: None,
    });
    items
}

/// The fields of the record the `receiver.` binding holds.
///
/// The receiver's type comes from the same declared-or-inferred rule hover uses
/// ([LSP-HOVER-VARIABLES]), so a binding with no annotation still completes.
/// An unknown receiver yields nothing rather than the whole symbol table:
/// `origin.` is a promise that only `origin`'s fields follow.
/// Implements [LSP-COMPLETION-MEMBER].
fn member_items(receiver: &str, program: &Program, line: u32) -> Vec<CompletionItem> {
    let symbols = collect_all_symbols(program);
    let Some(symbol) = best_match(&symbols, receiver, line) else {
        return Vec::new();
    };
    let types = osprey_types::infer_program(program);
    receiver_type_name(symbol, &types)
        .map(|owner| fields_of(&types, &owner))
        .unwrap_or_default()
}

/// The name of the type a binding holds: its annotation when it has one, else
/// the name inside the type the checker inferred.
///
/// The inferred type is read *structurally* rather than through its rendering:
/// a record Displays as `{ x: int, y: int }`, which names no type at all, so
/// rendering first and parsing back would lose exactly the case that matters.
fn receiver_type_name(symbol: &SymbolInfo, types: &ProgramTypes) -> Option<String> {
    if !symbol.ty.is_empty() {
        return Some(bare_type_name(&symbol.ty));
    }
    match types.let_type(symbol.position)? {
        Type::Record { name, .. } | Type::Con { name, .. } | Type::Union { name, .. } => {
            Some(name.clone())
        }
        Type::Var(_) | Type::Fun { .. } => None,
    }
}

fn fields_of(types: &ProgramTypes, owner: &str) -> Vec<CompletionItem> {
    types
        .ctors
        .values()
        .find(|layout| layout.owner == owner)
        .map(|layout| {
            layout
                .fields
                .iter()
                .map(|(name, ty)| CompletionItem {
                    label: name.clone(),
                    kind: CompletionKind::Variable,
                    detail: Some(ty.to_string()),
                    insert_text: None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A type's name without its arguments: the layout table is keyed by the
/// declared name, so `Box<int>` must look up `Box`.
fn bare_type_name(rendered: &str) -> String {
    rendered
        .split(['<', ' '])
        .next()
        .unwrap_or(rendered)
        .trim()
        .to_owned()
}

fn symbol_item(s: &SymbolInfo, flavor: Flavor) -> CompletionItem {
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
        detail: Some(mlrender::signature(flavor, &s.ty)),
        insert_text: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const U16: PositionEncoding = PositionEncoding::Utf16;

    /// Complete at the end of `src` — where an author's cursor actually is.
    fn at_end(src: &str, path: &str) -> Vec<CompletionItem> {
        let rows: Vec<&str> = src.split('\n').collect();
        let line = u32::try_from(rows.len().saturating_sub(1)).unwrap_or(0);
        let column = u32::try_from(rows.last().unwrap_or(&"").chars().count()).unwrap_or(0);
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

    const SRC: &str = "fn add(a: int, b: int) -> int = a + b\nlet total = add(1, 2)\n";

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
        // Completing after `origin.` used to dump the entire symbol table.
        let src = "type Point = { x: int, y: int }\n\
                   let origin = Point { x: 1, y: 2 }\n\
                   print(origin.";
        let names = labels(&at_end(src, "file:///a.osp"));
        assert_eq!(names, vec!["x", "y"], "{names:?}");
        // An unknown receiver stays silent rather than falling back to noise.
        assert!(at_end("fn f() = mystery.", "file:///a.osp").is_empty());
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
        assert!(at_end("fn add(", "file:///a.osp").is_empty());
        assert!(at_end("fn add(a: int, ", "file:///a.osp").is_empty());
    }

    #[test]
    fn ml_completions_never_offer_a_keyword_the_ml_frontend_does_not_have() {
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
                && item.insert_text.as_deref().is_some_and(|text| {
                    text.starts_with("state ") && !text.starts_with("state module")
                })
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
}
