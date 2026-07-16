//! The canonical prose metadata for every built-in — descriptions, parameter
//! docs, and examples — joined at render time to the authoritative type scheme
//! in [`crate::builtins`]. This is the single source the editor hover
//! ([`builtin_hover_markdown`]) and the `osprey --docs` reference generator both
//! render from, so hover and the website docs can never disagree.
//!
//! Types live in `builtins.rs`; prose lives in `builtin_docs_lang.rs` /
//! `builtin_docs_sys.rs`. They are joined by name, and the parity test below
//! fails the build if a built-in is registered without docs (or vice versa) or
//! if a doc's parameter count drifts from the function's real arity.

use crate::builtins::base_env;
use crate::ty::Type;
use std::fmt::Write as _;

/// One parameter's prose: its name and a one-line description. The parameter's
/// *type* is deliberately absent — it is read from the function's scheme so the
/// documented and actual types can never drift.
pub(crate) struct ParamDoc {
    pub name: &'static str,
    pub description: &'static str,
}

/// All prose for one built-in. Pairs by name with a scheme in `builtins.rs`.
pub(crate) struct BuiltinDoc {
    pub name: &'static str,
    pub summary: &'static str,
    pub params: &'static [ParamDoc],
    pub example: &'static str,
}

/// Every built-in's documentation, in category order, as one flat iterator over
/// the generated static tables.
fn builtin_docs() -> impl Iterator<Item = &'static BuiltinDoc> {
    use crate::builtin_docs_lang as lang;
    use crate::builtin_docs_sys as sys;
    [
        lang::CORE,
        lang::TESTING,
        lang::STRINGS,
        lang::FUNCTIONAL,
        lang::LISTS,
        lang::MAPS,
        sys::FILES,
        sys::HTTP,
        sys::JSON,
        sys::CONCURRENCY,
        sys::WEBSOCKET,
        sys::TERMINAL,
    ]
    .into_iter()
    .flatten()
}

/// One parameter as rendered for an editor or doc page: name, the type read
/// from the scheme, and the prose description.
#[derive(Debug, Clone)]
pub struct BuiltinParam {
    /// The parameter name.
    pub name: String,
    /// The parameter's rendered type, from the authoritative scheme.
    pub ty: String,
    /// The one-line prose description.
    pub description: String,
}

/// A built-in's full, render-ready documentation: prose joined to its real type
/// signature. Both the LSP hover and `osprey --docs` build their output here.
#[derive(Debug, Clone)]
pub struct BuiltinDocView {
    /// The built-in's name.
    pub name: String,
    /// The rendered call signature, e.g. `sleep(milliseconds: int) -> Unit`.
    pub signature: String,
    /// The one-line summary.
    pub summary: String,
    /// The parameters, in declaration order.
    pub params: Vec<BuiltinParam>,
    /// The rendered return type.
    pub return_type: String,
    /// A runnable example, or empty when none is documented.
    pub example: String,
}

/// The full documentation for `name`, or `None` when it is not a built-in.
#[must_use]
pub fn builtin_doc_view(name: &str) -> Option<BuiltinDocView> {
    let doc = builtin_docs().find(|d| d.name == name)?;
    let env = base_env();
    let scheme = env.get(name)?;
    let (param_types, ret) = split_fun(&scheme.ty);
    let params = join_params(doc.params, &param_types);
    let signature = render_signature(name, &params, &ret);
    Some(BuiltinDocView {
        name: name.to_string(),
        signature,
        summary: doc.summary.to_string(),
        params,
        return_type: ret.to_string(),
        example: doc.example.to_string(),
    })
}

/// Every documented built-in name, in byte order (matching the website index).
#[must_use]
pub fn builtin_names() -> Vec<String> {
    let mut names: Vec<String> = builtin_docs().map(|d| d.name.to_string()).collect();
    names.sort();
    names
}

/// Rich Markdown hover for a built-in: signature, summary, parameters, return
/// type, and example. `None` when `name` is not a built-in.
#[must_use]
pub fn builtin_hover_markdown(name: &str) -> Option<String> {
    let view = builtin_doc_view(name)?;
    let mut out = format!("```osprey\n{}\n```\n\n{}", view.signature, view.summary);
    append_params(&mut out, &view.params);
    let _ = write!(out, "\n\n**Returns** `{}`", view.return_type);
    append_example(&mut out, &view.example);
    Some(out)
}

/// Split a function scheme into (param types, return type); a non-function is
/// treated as a nullary value that is its own return type.
fn split_fun(ty: &Type) -> (Vec<Type>, Type) {
    match ty {
        Type::Fun { params, ret } => (params.clone(), (**ret).clone()),
        other => (Vec::new(), other.clone()),
    }
}

fn join_params(docs: &[ParamDoc], types: &[Type]) -> Vec<BuiltinParam> {
    docs.iter()
        .enumerate()
        .map(|(i, p)| BuiltinParam {
            name: p.name.to_string(),
            ty: types.get(i).map(Type::to_string).unwrap_or_default(),
            description: p.description.to_string(),
        })
        .collect()
}

fn render_signature(name: &str, params: &[BuiltinParam], ret: &Type) -> String {
    let shown: Vec<String> = params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect();
    format!("{name}({}) -> {ret}", shown.join(", "))
}

fn append_params(out: &mut String, params: &[BuiltinParam]) {
    if params.is_empty() {
        return;
    }
    out.push_str("\n\n**Parameters**");
    for p in params {
        let _ = write!(out, "\n- `{}` `{}` — {}", p.name, p.ty, p.description);
    }
}

fn append_example(out: &mut String, example: &str) {
    if !example.is_empty() {
        let _ = write!(out, "\n\n**Example**\n```osprey\n{example}\n```");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn every_builtin_is_documented_with_matching_arity() {
        // The drift guard: the documented set and the registered scheme set must
        // be identical, and each doc's parameter count must equal its real arity.
        let env = base_env();
        let documented: BTreeSet<String> = builtin_docs().map(|d| d.name.to_string()).collect();
        let registered: BTreeSet<String> = env.bound_names().into_iter().collect();
        assert_eq!(
            documented, registered,
            "builtin docs and type schemes are out of sync"
        );
        for doc in builtin_docs() {
            let scheme = env.get(doc.name).expect("registered above");
            let (params, _) = split_fun(&scheme.ty);
            assert_eq!(
                doc.params.len(),
                params.len(),
                "{} documents {} params but its type has {}",
                doc.name,
                doc.params.len(),
                params.len()
            );
        }
    }

    #[test]
    fn hover_joins_prose_to_the_real_signature() {
        // `sleep`'s doc historically claimed `-> int`; the scheme says `-> Unit`,
        // and the rendered hover must follow the scheme, not the stale prose.
        let md = builtin_hover_markdown("sleep").expect("sleep is documented");
        assert!(md.contains("sleep(milliseconds: int) -> Unit"), "{md}");
        assert!(md.contains("Pauses execution"), "{md}");
        assert!(md.contains("**Parameters**"), "{md}");
        assert!(md.contains("**Returns** `Unit`"), "{md}");
        assert!(md.contains("**Example**"), "{md}");
        assert!(builtin_hover_markdown("notARealBuiltin").is_none());
    }

    #[test]
    fn zero_arg_builtin_omits_the_parameters_section() {
        let md = builtin_hover_markdown("input").expect("input is documented");
        assert!(md.contains("input() -> string"), "{md}");
        assert!(!md.contains("**Parameters**"), "{md}");
    }
}
