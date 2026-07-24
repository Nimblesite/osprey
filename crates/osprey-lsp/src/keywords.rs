//! Keyword and snippet completions, partitioned by source flavor.
//!
//! Osprey's two surfaces do not share a keyword set: ML has no `fn`, `let` or
//! `if` at all (`osprey_syntax::ml::token::keyword_or_ident`), and every
//! brace-bodied snippet is rejected outright by the layout parser. Completing a
//! Default spelling into a `.ospml` buffer therefore inserts a guaranteed parse
//! error, which is worse than offering nothing. Implements
//! [LSP-FLAVOR-RENDER], [FLAVOR-ML-FN], [FLAVOR-ML-MATCH], [FLAVOR-ML-RECORD],
//! [FLAVOR-ML-EFFECT], [FLAVOR-ML-HANDLER], [MODULES-FLAVOR-PROJECTION].

use crate::model::{CompletionItem, CompletionKind};
use osprey_syntax::Flavor;

/// Core keyword/snippet completions, **Default** flavor (superset of the old TS
/// server's six).
const DEFAULT_CORE: [(&str, &str, &str); 7] = [
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
const ML_CORE: [(&str, &str, &str); 5] = [
    (
        "mut",
        "Mutable binding (a plain binding needs no keyword)",
        "mut ${1:name} = ${2:value}",
    ),
    (
        "match",
        "Pattern matching — layout arms, no braces",
        "match ${1:expr}\n\t${2:pattern} => ${3:result}",
    ),
    (
        "type",
        "Type declaration — layout variants",
        "type ${1:Name} =\n\t${2:Variant}\n\t${3:Variant}",
    ),
    (
        "effect",
        "Effect declaration — operations use `=>`",
        "effect ${1:Name}\n\t${2:op} : Unit => Unit",
    ),
    (
        "handle",
        "Install a handler over a body [FLAVOR-ML-HANDLER]",
        "handle ${1:Effect}\n\t${2:op} ${3:arg} => ${4:result}\nin\n\t${0}",
    ),
];

/// The fixed keyword/snippet completions for a document's authoring flavor.
pub(crate) fn keyword_items(flavor: Flavor) -> Vec<CompletionItem> {
    let ml = flavor == Flavor::Ml;
    let core: &[(&str, &str, &str)] = if ml { &ML_CORE } else { &DEFAULT_CORE };
    let modules = module_keyword_specs(ml);
    core.iter()
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
