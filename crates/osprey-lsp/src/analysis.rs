//! AST-driven program analysis: the document outline, built-in hover text, and
//! identifier lookups that power go-to-definition / find-references.
//!
//! This is the single source of truth for turning an [`osprey_ast::Program`]
//! into editor symbols — both the language server and the `osprey --symbols` /
//! `osprey --hover` CLI modes render from here.

#[cfg(test)]
use osprey_ast::InterpolatedPart;
use osprey_ast::{
    AstNode, DocComment, EffectRef, Expr, ExternParameter, Parameter, Position, Program, Stmt,
    TypeExpr,
};
use std::fmt::Write as _;
mod render;
pub(crate) use render::fill_inferred;
pub use render::render_type_params;
use render::{
    extern_pairs, fn_sym, generic_decl_sym, let_sym, param_pairs, render_doc, render_effect_row,
};

/// What kind of declaration a [`SymbolInfo`] describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// A logical namespace contribution.
    Namespace,
    /// A closed plain/state module boundary.
    Module,
    /// An explicit module interface.
    Signature,
    /// A function or `extern fn`.
    Function,
    /// A `let` binding.
    Variable,
    /// A `type` or `effect` declaration.
    Type,
}

impl SymbolKind {
    /// The wire string used in the `--symbols` JSON and LSP detail.
    #[must_use]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Namespace => "namespace",
            Self::Module => "module",
            Self::Signature => "signature",
            Self::Function => "function",
            Self::Variable => "variable",
            Self::Type => "type",
        }
    }
}

/// One outline entry derived from a top-level declaration.
#[derive(Debug, Clone)]
pub struct SymbolInfo {
    /// Collision-safe qualified source name (`billing::Tax::addTax`).
    pub name: String,
    /// Name as written on the declaration line (`addTax` / `Tax`).
    pub(crate) source_name: String,
    /// What sort of declaration this is.
    pub(crate) kind: SymbolKind,
    /// Rendered type/category text (signature for functions, annotation for
    /// `let`, `"type"`/`"effect"` for declarations).
    pub(crate) ty: String,
    /// Source position, when the parser recorded one (1-based line, 0-based col).
    pub position: Option<Position>,
    /// Full rendered signature for functions.
    pub(crate) signature: Option<String>,
    /// The rendered type-parameter binder (`<T, out U>`), empty when the
    /// declaration has none. Kept apart from the signature so a signature
    /// rebuilt from inferred slots cannot lose it.
    pub(crate) binder: String,
    /// `(name, rendered type)` parameter pairs for functions.
    pub(crate) parameters: Vec<(String, String)>,
    /// Rendered return type for functions.
    pub(crate) return_type: Option<String>,
    /// Written upper bound, never an inferred set of required operations.
    /// Kept separate so inference cannot drop it while rebuilding a signature.
    pub(crate) declared_effect_row: Option<String>,
    /// The declaration's documentation rendered to hover Markdown, when it
    /// carries a doc comment (either flavor). Implements [LSP-HOVER-DOCS].
    pub(crate) doc: Option<String>,
}

/// Collect every top-level declaration (recursing into modules) into outline
/// entries, in source order.
#[must_use]
pub fn collect_symbols(program: &Program) -> Vec<SymbolInfo> {
    let mut out = Vec::new();
    walk_stmts(&program.statements, &[], Bodies::Skip, &mut out);
    out
}

/// [`collect_symbols`] with every inferable slot filled in by the checker.
///
/// This is what anything that DISPLAYS a type must call. The house style
/// deletes every inferable annotation, so a consumer reading the raw AST sees
/// `fn twice(n)` where the checker knows `fn twice(n: int) -> int` — and the
/// outline, signature help and completion each degraded that way independently
/// while hover and `--symbols` reported the truth. One collector keeps them
/// from disagreeing again. Go-to-definition and reference search do NOT belong
/// here: they use positions only, and inference is not free.
/// Implements [LSP-HOVER-INFERRED-SIGNATURE].
#[must_use]
pub fn collect_inferred_symbols(program: &Program) -> Vec<SymbolInfo> {
    let types = osprey_types::infer_program(program);
    let mut symbols = collect_symbols(program);
    for sym in &mut symbols {
        fill_inferred(sym, &types);
    }
    symbols
}

/// Whether a statement walk descends into expression bodies to pick up nested
/// `let` bindings (hover) or stops at the declaration (outline).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Bodies {
    Skip,
    Descend,
}

fn extended(prefix: &[String], segments: &[String]) -> Vec<String> {
    prefix.iter().chain(segments).cloned().collect()
}

fn qualified(prefix: &[String], source_name: &str) -> String {
    if prefix.is_empty() {
        source_name.to_owned()
    } else {
        format!("{}::{source_name}", prefix.join("::"))
    }
}

fn qualify_symbol(symbol: &mut SymbolInfo, prefix: &[String]) {
    symbol.name = qualified(prefix, &symbol.source_name);
}

fn container_sym(
    prefix: &[String],
    source_name: &str,
    kind: SymbolKind,
    position: Option<Position>,
    docs: (Option<&DocComment>, Option<&DocComment>),
) -> SymbolInfo {
    SymbolInfo {
        name: qualified(prefix, source_name),
        source_name: source_name.to_owned(),
        kind,
        ty: kind.as_str().to_owned(),
        position,
        signature: None,
        binder: String::new(),
        parameters: Vec::new(),
        return_type: None,
        declared_effect_row: None,
        doc: render_scope_docs(docs),
    }
}

/// Render a scope's outer `///` and inner `//!` comments into one hover block.
/// A namespace or module can carry both — they describe the same scope from
/// opposite sides ([DOC-SIGIL-INNER]) — so the hover shows the outer text a
/// caller wrote first, then the inner text a maintainer wrote, separated by a
/// blank line. Either may be absent; both absent means no hover doc at all.
fn render_scope_docs(docs: (Option<&DocComment>, Option<&DocComment>)) -> Option<String> {
    let rendered: Vec<String> = [docs.0, docs.1]
        .into_iter()
        .flatten()
        .map(DocComment::render_markdown)
        .filter(|text| !text.is_empty())
        .collect();
    if rendered.is_empty() {
        None
    } else {
        Some(rendered.join("\n\n"))
    }
}

/// Collect every binding in the program — top-level declarations *and* `let`s
/// nested in expression bodies (function/handler/match/block bodies) — so hover
/// resolves local variables, not only top-level names. Source order.
/// Implements [LSP-HOVER-VARIABLES]
#[must_use]
pub(crate) fn collect_all_symbols(program: &Program) -> Vec<SymbolInfo> {
    let mut out = Vec::new();
    walk_stmts(&program.statements, &[], Bodies::Descend, &mut out);
    out
}

/// One walker for both surfaces: containers qualify their children identically,
/// and only `bodies` decides whether declaration bodies are descended.
fn walk_stmts(stmts: &[Stmt], prefix: &[String], bodies: Bodies, out: &mut Vec<SymbolInfo>) {
    for stmt in stmts {
        match stmt {
            Stmt::Namespace {
                name,
                body,
                doc,
                inner_doc,
                position,
                ..
            } => {
                let label = name.label().to_owned();
                out.push(container_sym(
                    prefix,
                    &label,
                    SymbolKind::Namespace,
                    *position,
                    (doc.as_ref(), inner_doc.as_ref()),
                ));
                walk_stmts(body, &extended(prefix, &[label]), bodies, out);
            }
            Stmt::Module {
                path,
                body,
                doc,
                inner_doc,
                position,
                ..
            } => {
                out.push(container_sym(
                    prefix,
                    &path.to_string(),
                    SymbolKind::Module,
                    *position,
                    (doc.as_ref(), inner_doc.as_ref()),
                ));
                let child_prefix = extended(prefix, &path.segments);
                for item in body {
                    walk_stmts(
                        std::slice::from_ref(item.declaration.as_ref()),
                        &child_prefix,
                        bodies,
                        out,
                    );
                }
            }
            // A signature is a documented declaration form too ([DOC-ATTACH]);
            // it has no body, so it carries only an outer doc.
            Stmt::Signature {
                name,
                doc,
                position,
                ..
            } => out.push(container_sym(
                prefix,
                name,
                SymbolKind::Signature,
                *position,
                (doc.as_ref(), None),
            )),
            other => {
                if let Some(mut symbol) = sym_of(other) {
                    qualify_symbol(&mut symbol, prefix);
                    out.push(symbol);
                }
                if bodies == Bodies::Descend {
                    walk_stmt_body(other, prefix, out);
                }
            }
        }
    }
}

fn walk_stmt_body(stmt: &Stmt, prefix: &[String], out: &mut Vec<SymbolInfo>) {
    match stmt {
        Stmt::Function { body, .. } => walk_expr(body, prefix, out),
        Stmt::Let { value, .. } | Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
            walk_expr(value, prefix, out);
        }
        _ => {}
    }
}

/// Descend expressions while preserving block-local symbol collection.
fn walk_expr(e: &Expr, prefix: &[String], out: &mut Vec<SymbolInfo>) {
    match e {
        Expr::Block { statements, value } => {
            walk_stmts(statements, prefix, Bodies::Descend, out);
            if let Some(value) = value {
                walk_expr(value, prefix, out);
            }
        }
        // Resume operands were not part of this symbol collector's traversal.
        Expr::Resume(_) => {}
        _ => AstNode::Expression(e).for_each_child(|child| {
            if let AstNode::Expression(expression) = child {
                walk_expr(expression, prefix, out);
            }
        }),
    }
}

fn sym_of(stmt: &Stmt) -> Option<SymbolInfo> {
    match stmt {
        Stmt::Function {
            name,
            type_params,
            parameters,
            return_type,
            effects,
            effect_tail,
            effect_row_present,
            doc,
            position,
            ..
        } => Some(fn_sym(
            name,
            &render_type_params(type_params),
            param_pairs(parameters),
            return_type.as_ref(),
            render_effect_row(effects, effect_tail.as_deref(), *effect_row_present),
            render_doc(doc.as_ref()),
            *position,
        )),
        Stmt::Extern {
            name,
            parameters,
            return_type,
            doc,
            position,
        } => Some(fn_sym(
            name,
            "",
            extern_pairs(parameters),
            return_type.as_ref(),
            None,
            render_doc(doc.as_ref()),
            *position,
        )),
        Stmt::Let {
            name,
            ty,
            doc,
            position,
            ..
        } => Some(let_sym(
            name,
            ty.as_ref(),
            render_doc(doc.as_ref()),
            *position,
        )),
        Stmt::Type {
            name,
            type_params,
            doc,
            position,
            ..
        } => Some(generic_decl_sym(
            name,
            type_params,
            "type",
            render_doc(doc.as_ref()),
            *position,
        )),
        Stmt::Effect {
            stage: osprey_ast::Stage::Dynamic,
            name,
            type_params,
            doc,
            position,
            ..
        } => Some(generic_decl_sym(
            name,
            type_params,
            "effect",
            render_doc(doc.as_ref()),
            *position,
        )),
        _ => None,
    }
}

/// Render a written type expression back to source-ish text.
#[must_use]
pub fn render_type(t: &TypeExpr) -> String {
    if t.is_function {
        let ps: Vec<String> = t.parameter_types.iter().map(render_type).collect();
        let ret = t
            .return_type
            .as_deref()
            .map_or_else(|| String::from("Unit"), render_type);
        return format!("fn({}) -> {ret}", ps.join(", "));
    }
    if t.is_array {
        return t
            .array_element
            .as_deref()
            .map_or_else(|| String::from("[]"), |e| format!("[{}]", render_type(e)));
    }
    if t.generic_params.is_empty() {
        return t.name.clone();
    }
    let gs: Vec<String> = t.generic_params.iter().map(render_type).collect();
    format!("{}<{}>", t.name, gs.join(", "))
}

/// Rich Markdown hover text for a built-in name, or `None` when not a built-in.
/// Renders the full metadata — signature, description, parameters, return type,
/// and example — from the single source in `osprey_types`, so a built-in hovers
/// with exactly the detail the reference docs carry.
#[must_use]
pub fn builtin_hover(name: &str) -> Option<String> {
    osprey_types::builtin_hover_markdown(name)
}

/// The whole document outline as the `--symbols` JSON array, every inferable
/// slot filled in by the checker ([`fill_inferred`]) so deleting an annotation
/// the house style calls redundant cannot change what tooling reports.
#[must_use]
pub fn symbols_json(program: &Program) -> String {
    let rendered: Vec<String> = collect_inferred_symbols(program)
        .iter()
        .map(sym_json)
        .collect();
    format!("[{}]", rendered.join(","))
}

/// Render one entry as a JSON object. The AST column is 0-based; the wire format
/// is 1-based, so it is shifted here.
fn sym_json(s: &SymbolInfo) -> String {
    let (line, column) = s
        .position
        .map_or((1, 1), |p| (p.line, p.column.saturating_add(1)));
    let mut o = format!(
        "{{\"name\":{},\"kind\":{},\"type\":{},\"line\":{line},\"column\":{column}",
        json_str(&s.name),
        json_str(s.kind.as_str()),
        json_str(&s.ty)
    );
    if let Some(sig) = &s.signature {
        let _ = write!(o, ",\"signature\":{}", json_str(sig));
    }
    if !s.parameters.is_empty() {
        let _ = write!(o, ",\"parameters\":{}", params_json(&s.parameters));
    }
    if let Some(ret) = &s.return_type {
        let _ = write!(o, ",\"returnType\":{}", json_str(ret));
    }
    if let Some(row) = &s.declared_effect_row {
        let _ = write!(o, ",\"declaredEffectRow\":{}", json_str(row));
    }
    o.push('}');
    o
}

fn params_json(params: &[(String, String)]) -> String {
    let items: Vec<String> = params
        .iter()
        .map(|(n, t)| format!("{{\"name\":{},\"type\":{}}}", json_str(n), json_str(t)))
        .collect();
    format!("[{}]", items.join(","))
}

pub(crate) fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len().saturating_add(2));
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests;
