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

use crate::analysis::{collect_all_symbols, collect_inferred_symbols, SymbolInfo, SymbolKind};
use crate::context::{self, Cursor};
use crate::features::best_match;
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
pub(crate) fn completion(
    text: &str,
    path: &str,
    line: u32,
    character: u32,
    encoding: PositionEncoding,
    project: &workspace::View,
) -> Vec<CompletionItem> {
    let siblings = &project.siblings;
    let flavor = project.flavor(path, text);
    let program = project.program(path, text);
    let cursor = context::at(text, line, character, encoding);
    // A fresh binder is the author inventing a name; every suggestion is noise.
    if cursor == Cursor::Binder {
        return Vec::new();
    }
    if let Cursor::Member(receiver) = &cursor {
        return member_items(receiver, &program, line);
    }
    let symbols = visible_symbols(&program, siblings);
    keyword_items(flavor, &cursor)
        .into_iter()
        .chain(symbol_items(&cursor, &symbols, &program, flavor))
        .collect()
}

/// Every symbol the open document can name: its own declarations plus those of
/// every sibling file the project links with it. Implements [LSP-WORKSPACE].
fn visible_symbols(program: &Program, siblings: &[workspace::Sibling]) -> Vec<SymbolInfo> {
    // Inferred, because a completion item SHOWS the symbol's type
    // ([`collect_inferred_symbols`]) — a deleted annotation must not empty it.
    let mut symbols = collect_inferred_symbols(program);
    for sibling in siblings {
        symbols.extend(collect_inferred_symbols(&sibling.program));
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
            documentation: None,
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
            documentation: None,
            insert_text: None,
        })
        .collect();
    items.sort_by(|left, right| left.label.cmp(&right.label));
    items.push(CompletionItem {
        label: WILDCARD.to_owned(),
        kind: CompletionKind::Keyword,
        detail: Some(String::from("Wildcard pattern")),
        documentation: None,
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
    // The RECEIVER's own type first: `let b = Box { value: 1 }` infers
    // `Box<int>`, so `b.value` is an `int` and saying anything else is a lie.
    // The constructor layout is the generic DECLARATION (`value: T`) and is
    // only the fallback, for a receiver whose fields inference did not reach.
    inferred_fields(symbol, &types)
        .or_else(|| receiver_type_name(symbol, &types).map(|owner| fields_of(&types, &owner)))
        .unwrap_or_default()
}

/// The fields of the receiver's own inferred record, at the types this binding
/// actually instantiated them to.
///
/// Reading the constructor layout instead reported the DECLARATION's field
/// types, so every instantiation of a generic record shared one answer:
/// `b.value` on a `Box<int>` completed as the layout's type variable. Rendering
/// that variable politely as `_` still withheld a type the checker had proved —
/// the same defect as the old `-> Unit` fallback, one surface along
/// ([TYPE-RENDER-HOLES]).
fn inferred_fields(symbol: &SymbolInfo, types: &ProgramTypes) -> Option<Vec<CompletionItem>> {
    let Type::Record { name, fields } = types.let_type(symbol.position)? else {
        return None;
    };
    if fields.is_empty() {
        return None;
    }
    // ORDER comes from the declaration, TYPES from this receiver. The inferred
    // record stores its fields in a `BTreeMap`, so reading it directly listed
    // them alphabetically while an annotated binding — which takes the layout
    // fallback — listed them as declared. The same record then completed in two
    // different orders depending on whether its binding carried a type nobody
    // needed to write, which is precisely the "deleting an annotation changes
    // nothing a reader sees" promise this branch exists to keep.
    let declared = types.ctors.values().find(|layout| layout.owner == *name);
    let ordered: Vec<CompletionItem> = declared.map_or_else(
        || fields.iter().map(|(f, ty)| field_item(f, ty)).collect(),
        |layout| {
            layout
                .fields
                .iter()
                .map(|(f, declared_ty)| field_item(f, fields.get(f).unwrap_or(declared_ty)))
                .collect()
        },
    );
    Some(ordered)
}

/// One field as a completion item, rendered for a reader.
fn field_item(name: &str, ty: &Type) -> CompletionItem {
    CompletionItem {
        label: name.to_owned(),
        kind: CompletionKind::Variable,
        // A field whose type inference genuinely left open is shown as a hole;
        // `t0` is the checker's private name and means nothing to a reader
        // ([TYPE-RENDER-HOLES]).
        detail: Some(osprey_types::render_with_holes(ty)),
        documentation: None,
        insert_text: None,
    }
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
                .map(|(name, ty)| field_item(name, ty))
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
        documentation: crate::analysis::requirements::description(s.effect_requirements.as_ref()),
        kind,
        detail: Some(mlrender::signature(flavor, &s.ty)),
        insert_text: None,
    }
}

#[cfg(test)]
mod tests;
