//! Keyword and snippet completions, partitioned by source flavor.
//!
//! Osprey's two surfaces do not share a keyword set: ML has no `fn`, `let` or
//! `if` at all (`osprey_syntax::ml::token::keyword_or_ident`), and every
//! brace-bodied snippet is rejected outright by the layout parser. Completing a
//! Default spelling into a `.ospml` buffer therefore inserts a guaranteed parse
//! error, which is worse than offering nothing. Implements
//! [LSP-FLAVOR-RENDER], [FLAVOR-ML-FN], [FLAVOR-ML-MATCH], [FLAVOR-ML-RECORD],
//! [FLAVOR-ML-EFFECT], [FLAVOR-ML-HANDLER], [MODULES-FLAVOR-PROJECTION].

use crate::context::Cursor;
use crate::model::{CompletionItem, CompletionKind};
use osprey_syntax::Flavor;

/// Where a keyword may be written. A declaration keyword completed into an
/// argument slot expands to source the parser rejects, so the two sets are kept
/// apart rather than filtered by hand at each call site.
/// Implements [LSP-COMPLETION-CONTEXT].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// Declaration/statement position only (`fn`, `type`, `namespace`, …).
    Decl,
    /// Also legal where a value is expected (`match`, `if`, `handle`).
    Expr,
}

/// One completion's fixed spelling: label, detail, snippet, and where it is
/// legal.
type Spec = (&'static str, &'static str, &'static str, Scope);

/// Core keyword/snippet completions, **Default** flavor (superset of the old TS
/// server's six).
const DEFAULT_CORE: [Spec; 7] = [
    (
        "if",
        "Conditional expression [GRAMMAR-IF-ELSE]",
        "if ${1:condition} { ${2:then} } else { ${3:else} }",
        Scope::Expr,
    ),
    (
        "fn",
        "Function declaration",
        "fn ${1:name}(${2:params}) = ${3:body}",
        Scope::Decl,
    ),
    (
        "let",
        "Variable declaration",
        "let ${1:name} = ${2:value}",
        Scope::Decl,
    ),
    (
        "mut",
        "Mutable variable declaration",
        "mut ${1:name} = ${2:value}",
        Scope::Decl,
    ),
    (
        "match",
        "Pattern matching",
        "match ${1:expr} {\n\t${2:pattern} => ${3:result}\n}",
        Scope::Expr,
    ),
    (
        "type",
        "Type declaration",
        "type ${1:Name} = ${2:Variant} | ${3:Variant}",
        Scope::Decl,
    ),
    (
        "effect",
        "Effect declaration",
        "effect ${1:Name} {\n\t${2:op}: ${3:fn() -> Unit}\n}",
        Scope::Decl,
    ),
];

/// Core keyword/snippet completions, **ML** flavor.
///
/// This is deliberately not a transliteration of [`DEFAULT_CORE`]: the ML
/// keyword set (`ml::token::keyword_or_ident`) genuinely has **no `fn`, no
/// `let` and no `if`**. A definition is a bare clause (`inc x = x + 1`) under an
/// optional signature line, a binding needs no keyword, and a condition is a
/// `match` on `true`/`false`. Completing those three would insert identifiers
/// that are not keywords at all — and every brace-form snippet expands to source
/// the ML frontend rejects outright. Implements [FLAVOR-ML-FN],
/// [FLAVOR-ML-MATCH], [FLAVOR-ML-RECORD], [FLAVOR-ML-EFFECT],
/// [FLAVOR-ML-HANDLER], [LSP-FLAVOR-RENDER].
const ML_CORE: [Spec; 5] = [
    (
        "mut",
        "Mutable binding (a plain binding needs no keyword)",
        "mut ${1:name} = ${2:value}",
        Scope::Decl,
    ),
    (
        "match",
        "Pattern matching — layout arms, no braces",
        "match ${1:expr}\n\t${2:pattern} => ${3:result}",
        Scope::Expr,
    ),
    (
        "type",
        "Type declaration — layout variants",
        "type ${1:Name} =\n\t${2:Variant}\n\t${3:Variant}",
        Scope::Decl,
    ),
    (
        "effect",
        "Effect declaration — operations use `=>`",
        "effect ${1:Name}\n\t${2:op} : Unit => Unit",
        Scope::Decl,
    ),
    (
        "handle",
        "Install a handler over a body [FLAVOR-ML-HANDLER]",
        "handle ${1:Effect}\n\t${2:op} ${3:arg} => ${4:result}\nin\n\t${0}",
        Scope::Expr,
    ),
];

/// The keyword/snippet completions legal at `cursor` in a document of
/// `flavor`. A type annotation, a field access and a fresh binder take no
/// keyword at all; a value position takes only the expression forms.
/// Implements [LSP-COMPLETION-CONTEXT], [LSP-FLAVOR-RENDER].
pub(crate) fn keyword_items(flavor: Flavor, cursor: &Cursor) -> Vec<CompletionItem> {
    let Some(allowed) = scope_at(cursor) else {
        return Vec::new();
    };
    let ml = flavor == Flavor::Ml;
    let core: &[Spec] = if ml { &ML_CORE } else { &DEFAULT_CORE };
    let modules = module_keyword_specs(ml);
    core.iter()
        .chain(modules.iter())
        .filter(|(_, _, _, scope)| allowed == Scope::Decl || *scope == Scope::Expr)
        .map(|(label, detail, snippet, _)| CompletionItem {
            label: (*label).to_owned(),
            kind: CompletionKind::Keyword,
            detail: Some((*detail).to_owned()),
            insert_text: Some((*snippet).to_owned()),
        })
        .collect()
}

/// The widest keyword scope `cursor` admits, or `None` where no keyword is
/// legal — a written type, a `receiver.` field, a match pattern, or the name of
/// a parameter being invented.
const fn scope_at(cursor: &Cursor) -> Option<Scope> {
    match cursor {
        Cursor::Declaration => Some(Scope::Decl),
        Cursor::Value => Some(Scope::Expr),
        Cursor::Type | Cursor::Member(_) | Cursor::Pattern | Cursor::Binder => None,
    }
}

fn module_keyword_specs(ml: bool) -> [Spec; 8] {
    [
        (
            "namespace",
            "Logical namespace [MODULES-NAMESPACE]",
            if ml {
                "namespace ${1:name}"
            } else {
                "namespace ${1:name};"
            },
            Scope::Decl,
        ),
        (
            "import",
            "Namespace/module import [MODULES-IMPORT]",
            "import ${1:namespace}::${2:Module}",
            Scope::Decl,
        ),
        (
            "module",
            "Closed module boundary [MODULES-MODULE]",
            if ml {
                "module ${1:Name}\n\t${0}"
            } else {
                "module ${1:Name} {\n\t${0}\n}"
            },
            Scope::Decl,
        ),
        (
            "signature",
            "Explicit module interface [MODULES-SIGNATURE]",
            if ml {
                "signature ${1:Name}\n\t${0}"
            } else {
                "signature ${1:Name} {\n\t${0}\n}"
            },
            Scope::Decl,
        ),
        (
            "export",
            "Export a module declaration [MODULES-EXPORTS]",
            if ml {
                "export ${1:name} = ${2:value}"
            } else {
                "export fn ${1:name}(${2:params}) = ${3:body}"
            },
            Scope::Decl,
        ),
        (
            "state",
            "Durable state owner [MODULES-STATE-MODULE]",
            if ml {
                "state ${1:Name}\n\t${0}"
            } else {
                "state module ${1:Name} {\n\t${0}\n}"
            },
            Scope::Decl,
        ),
        (
            "opaque",
            "Opaque exported type [MODULES-OPAQUE-TYPES]",
            "opaque type ${1:Name}",
            Scope::Decl,
        ),
        (
            "as",
            "Import alias [MODULES-IMPORT]",
            "as ${1:Alias}",
            Scope::Decl,
        ),
    ]
}
