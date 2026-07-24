//! AST-driven program analysis: the document outline, built-in hover text, and
//! identifier lookups that power go-to-definition / find-references.
//!
//! This is the single source of truth for turning an [`osprey_ast::Program`]
//! into editor symbols — both the language server and the `osprey --symbols` /
//! `osprey --hover` CLI modes render from here.

use osprey_ast::{
    walk_each, Expr, ExternParameter, InterpolatedPart, NamedArgument, Parameter, Position,
    Program, Stmt, TypeExpr,
};
use std::fmt::Write as _;

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
    pub const fn as_str(self) -> &'static str {
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
    pub source_name: String,
    /// What sort of declaration this is.
    pub kind: SymbolKind,
    /// Rendered type/category text (signature for functions, annotation for
    /// `let`, `"type"`/`"effect"` for declarations).
    pub ty: String,
    /// Source position, when the parser recorded one (1-based line, 0-based col).
    pub position: Option<Position>,
    /// Full rendered signature for functions.
    pub signature: Option<String>,
    /// `(name, rendered type)` parameter pairs for functions.
    pub parameters: Vec<(String, String)>,
    /// Rendered return type for functions.
    pub return_type: Option<String>,
    /// The declaration's documentation rendered to hover Markdown, when it
    /// carries a doc comment (either flavor). Implements [LSP-HOVER-DOCS].
    pub doc: Option<String>,
}

/// Collect every top-level declaration (recursing into modules) into outline
/// entries, in source order.
#[must_use]
pub fn collect_symbols(program: &Program) -> Vec<SymbolInfo> {
    let mut out = Vec::new();
    collect(&program.statements, &[], &mut out);
    out
}

fn collect(stmts: &[Stmt], prefix: &[String], out: &mut Vec<SymbolInfo>) {
    for stmt in stmts {
        match stmt {
            Stmt::Namespace {
                name,
                body,
                position,
                ..
            } => {
                let label = name.label();
                out.push(container_sym(
                    prefix,
                    label,
                    SymbolKind::Namespace,
                    *position,
                ));
                let child_prefix = extended(prefix, std::slice::from_ref(&label.to_owned()));
                collect(body, &child_prefix, out);
            }
            Stmt::Module {
                path,
                body,
                position,
                ..
            } => {
                out.push(container_sym(
                    prefix,
                    &path.to_string(),
                    SymbolKind::Module,
                    *position,
                ));
                let child_prefix = extended(prefix, &path.segments);
                for item in body {
                    collect(
                        std::slice::from_ref(item.declaration.as_ref()),
                        &child_prefix,
                        out,
                    );
                }
            }
            Stmt::Signature { name, position, .. } => out.push(container_sym(
                prefix,
                name,
                SymbolKind::Signature,
                *position,
            )),
            other => {
                if let Some(mut symbol) = sym_of(other) {
                    qualify_symbol(&mut symbol, prefix);
                    out.push(symbol);
                }
            }
        }
    }
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
) -> SymbolInfo {
    SymbolInfo {
        name: qualified(prefix, source_name),
        source_name: source_name.to_owned(),
        kind,
        ty: kind.as_str().to_owned(),
        position,
        signature: None,
        parameters: Vec::new(),
        return_type: None,
        doc: None,
    }
}

/// Collect every binding in the program — top-level declarations *and* `let`s
/// nested in expression bodies (function/handler/match/block bodies) — so hover
/// resolves local variables, not only top-level names. Source order.
/// Implements [LSP-HOVER-VARIABLES]
#[must_use]
pub fn collect_all_symbols(program: &Program) -> Vec<SymbolInfo> {
    let mut out = Vec::new();
    walk_stmts(&program.statements, &[], &mut out);
    out
}

fn walk_stmts(stmts: &[Stmt], prefix: &[String], out: &mut Vec<SymbolInfo>) {
    for stmt in stmts {
        match stmt {
            Stmt::Namespace {
                name,
                body,
                position,
                ..
            } => {
                let label = name.label().to_owned();
                out.push(container_sym(
                    prefix,
                    &label,
                    SymbolKind::Namespace,
                    *position,
                ));
                walk_stmts(body, &extended(prefix, &[label]), out);
            }
            Stmt::Module {
                path,
                body,
                position,
                ..
            } => {
                out.push(container_sym(
                    prefix,
                    &path.to_string(),
                    SymbolKind::Module,
                    *position,
                ));
                let child_prefix = extended(prefix, &path.segments);
                for item in body {
                    walk_stmts(
                        std::slice::from_ref(item.declaration.as_ref()),
                        &child_prefix,
                        out,
                    );
                }
            }
            Stmt::Signature { name, position, .. } => out.push(container_sym(
                prefix,
                name,
                SymbolKind::Signature,
                *position,
            )),
            other => {
                if let Some(mut symbol) = sym_of(other) {
                    qualify_symbol(&mut symbol, prefix);
                    out.push(symbol);
                }
                walk_stmt_body(other, prefix, out);
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

/// Descend an expression collecting nested `let` bindings (first third).
fn walk_expr(e: &Expr, prefix: &[String], out: &mut Vec<SymbolInfo>) {
    match e {
        Expr::InterpolatedStr(parts) => parts.iter().for_each(|p| {
            if let InterpolatedPart::Expr(x) = p {
                walk_expr(x, prefix, out);
            }
        }),
        Expr::List(xs) => walk_each(xs, out, |x| x, |x, out| walk_expr(x, prefix, out)),
        Expr::Map(entries) => entries.iter().for_each(|en| {
            walk_expr(&en.key, prefix, out);
            walk_expr(&en.value, prefix, out);
        }),
        Expr::Object(fields) => walk_each(
            fields,
            out,
            |f| &f.value,
            |x, out| {
                walk_expr(x, prefix, out);
            },
        ),
        Expr::Binary { left, right, .. } | Expr::Pipe { left, right } => {
            walk_expr(left, prefix, out);
            walk_expr(right, prefix, out);
        }
        Expr::Unary { operand, .. } => walk_expr(operand, prefix, out),
        other => walk_expr_rest(other, prefix, out),
    }
}

/// Continuation of [`walk_expr`] — call/navigation/block forms (second third).
fn walk_expr_rest(e: &Expr, prefix: &[String], out: &mut Vec<SymbolInfo>) {
    match e {
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => {
            walk_expr(function, prefix, out);
            walk_arguments(arguments, named_arguments, prefix, out);
        }
        Expr::MethodCall {
            target,
            arguments,
            named_arguments,
            ..
        } => {
            walk_expr(target, prefix, out);
            walk_arguments(arguments, named_arguments, prefix, out);
        }
        Expr::FieldAccess { target, .. } => walk_expr(target, prefix, out),
        Expr::Index { target, index } => {
            walk_expr(target, prefix, out);
            walk_expr(index, prefix, out);
        }
        Expr::Lambda { body, .. } => walk_expr(body, prefix, out),
        Expr::Match { value, arms } => {
            walk_expr(value, prefix, out);
            walk_each(
                arms,
                out,
                |arm| &arm.body,
                |x, out| {
                    walk_expr(x, prefix, out);
                },
            );
        }
        Expr::Block { statements, value } => {
            walk_stmts(statements, prefix, out);
            if let Some(v) = value {
                walk_expr(v, prefix, out);
            }
        }
        Expr::TypeConstructor { fields, .. } | Expr::Update { fields, .. } => {
            walk_each(
                fields,
                out,
                |f| &f.value,
                |x, out| {
                    walk_expr(x, prefix, out);
                },
            );
        }
        other => walk_expr_fiber(other, prefix, out),
    }
}

/// Final third of [`walk_expr`]: fiber/effect forms; leaves fall through.
fn walk_expr_fiber(e: &Expr, prefix: &[String], out: &mut Vec<SymbolInfo>) {
    match e {
        Expr::Spawn(i) | Expr::Await(i) | Expr::Recv(i) | Expr::Yield(Some(i)) => {
            walk_expr(i, prefix, out);
        }
        Expr::Send { channel, value } => {
            walk_expr(channel, prefix, out);
            walk_expr(value, prefix, out);
        }
        Expr::Select { arms } => walk_each(
            arms,
            out,
            |arm| &arm.body,
            |x, out| {
                walk_expr(x, prefix, out);
            },
        ),
        Expr::Perform {
            arguments,
            named_arguments,
            ..
        } => {
            walk_arguments(arguments, named_arguments, prefix, out);
        }
        Expr::Handler { arms, body, .. } => {
            for arm in arms {
                walk_expr(&arm.body, prefix, out);
            }
            walk_expr(body, prefix, out);
        }
        _ => {}
    }
}

fn walk_arguments(
    arguments: &[Expr],
    named_arguments: &[NamedArgument],
    prefix: &[String],
    out: &mut Vec<SymbolInfo>,
) {
    for argument in arguments {
        walk_expr(argument, prefix, out);
    }
    for argument in named_arguments {
        walk_expr(&argument.value, prefix, out);
    }
}

fn sym_of(stmt: &Stmt) -> Option<SymbolInfo> {
    match stmt {
        Stmt::Function {
            name,
            type_params,
            parameters,
            return_type,
            doc,
            position,
            ..
        } => {
            let mut sym = fn_sym(
                name,
                param_pairs(parameters),
                return_type.as_ref(),
                render_doc(doc.as_ref()),
                *position,
            );
            let binder = render_type_params(type_params);
            if !binder.is_empty() {
                let with_binder =
                    sym.ty
                        .replacen(&format!("fn {name}("), &format!("fn {name}{binder}("), 1);
                sym.ty.clone_from(&with_binder);
                sym.signature = Some(with_binder);
            }
            Some(sym)
        }
        Stmt::Extern {
            name,
            parameters,
            return_type,
            doc,
            position,
        } => Some(fn_sym(
            name,
            extern_pairs(parameters),
            return_type.as_ref(),
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

/// Render a declaration's structured doc comment to the Markdown a hover shows,
/// or `None` when it has none ([DOC-EXPORT], hover half).
fn render_doc(doc: Option<&osprey_ast::DocComment>) -> Option<String> {
    doc.map(osprey_ast::DocComment::render_markdown)
}

/// A type/effect declaration symbol whose signature shows the binder
/// (`type Option<T>`, `effect State<T>`) while the name stays bare for
/// lookups. Implements [TYPE-GENERICS-DECL].
fn generic_decl_sym(
    name: &str,
    type_params: &[osprey_ast::TypeParam],
    kind: &str,
    doc: Option<String>,
    position: Option<Position>,
) -> SymbolInfo {
    let mut sym = decl_sym(name, kind, position);
    sym.doc = doc;
    let binder = render_type_params(type_params);
    if !binder.is_empty() {
        sym.signature = Some(format!("{kind} {name}{binder}"));
    }
    sym
}

/// Render a declaration's type-parameter binder (`<T, out U>`), empty when it
/// has none. Implements [TYPE-GENERICS-DECL].
fn render_type_params(params: &[osprey_ast::TypeParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    let shown: Vec<String> = params
        .iter()
        .map(|p| match p.variance {
            osprey_ast::Variance::Covariant => format!("out {}", p.name),
            osprey_ast::Variance::Contravariant => format!("in {}", p.name),
            osprey_ast::Variance::Invariant => p.name.clone(),
        })
        .collect();
    format!("<{}>", shown.join(", "))
}

fn fn_sym(
    name: &str,
    parameters: Vec<(String, String)>,
    return_type: Option<&TypeExpr>,
    doc: Option<String>,
    position: Option<Position>,
) -> SymbolInfo {
    let ret = return_type.map_or_else(|| String::from("Unit"), render_type);
    let shown: Vec<String> = parameters.iter().map(render_param).collect();
    let signature = format!("fn {name}({}) -> {ret}", shown.join(", "));
    SymbolInfo {
        name: name.into(),
        source_name: name.into(),
        kind: SymbolKind::Function,
        ty: signature.clone(),
        position,
        signature: Some(signature),
        parameters,
        return_type: Some(ret),
        doc,
    }
}

fn render_param((n, t): &(String, String)) -> String {
    if t.is_empty() {
        n.clone()
    } else {
        format!("{n}: {t}")
    }
}

fn let_sym(
    name: &str,
    ty: Option<&TypeExpr>,
    doc: Option<String>,
    position: Option<Position>,
) -> SymbolInfo {
    SymbolInfo {
        name: name.into(),
        source_name: name.into(),
        kind: SymbolKind::Variable,
        ty: ty.map(render_type).unwrap_or_default(),
        position,
        signature: None,
        parameters: Vec::new(),
        return_type: None,
        doc,
    }
}

fn decl_sym(name: &str, ty: &str, position: Option<Position>) -> SymbolInfo {
    SymbolInfo {
        name: name.into(),
        source_name: name.into(),
        kind: SymbolKind::Type,
        ty: ty.into(),
        position,
        signature: None,
        parameters: Vec::new(),
        return_type: None,
        doc: None,
    }
}

fn param_pairs(params: &[Parameter]) -> Vec<(String, String)> {
    params
        .iter()
        .map(|p| {
            (
                p.name.clone(),
                p.ty.as_ref().map(render_type).unwrap_or_default(),
            )
        })
        .collect()
}

fn extern_pairs(params: &[ExternParameter]) -> Vec<(String, String)> {
    params
        .iter()
        .map(|p| (p.name.clone(), render_type(&p.ty)))
        .collect()
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

/// The whole document outline as the `--symbols` JSON array.
#[must_use]
pub fn symbols_json(program: &Program) -> String {
    let rendered: Vec<String> = collect_symbols(program).iter().map(sym_json).collect();
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
mod tests {
    use super::*;

    #[test]
    fn outline_covers_every_declaration_form() {
        let parsed = osprey_syntax::parse_program(
            "type Shade = Light | Dark\n\
             effect Log { info: fn(string) -> Unit }\n\
             extern fn puts(s: string) -> int\n\
             let limit: int = 10\n\
             fn multiply(a: int, b: int) -> int = a * b\n\
             type Box<T> = { item: T }\n\
             effect Feed<out T> { next: fn() -> T }\n\
             fn pick<T, out U>(a: T, b: U) -> T = a\n\
             fn main() -> Unit = print(multiply(a: limit, b: 2))\n",
        );
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let json = symbols_json(&parsed.program);
        for frag in [
            "\"name\":\"Shade\",\"kind\":\"type\",\"type\":\"type\",\"line\":1,\"column\":1",
            "\"name\":\"Log\",\"kind\":\"type\",\"type\":\"effect\",\"line\":2",
            "\"name\":\"puts\",\"kind\":\"function\"",
            "\"signature\":\"fn puts(s: string) -> int\"",
            "\"name\":\"limit\",\"kind\":\"variable\",\"type\":\"int\",\"line\":4",
            "\"name\":\"multiply\",\"kind\":\"function\"",
            "\"signature\":\"fn multiply(a: int, b: int) -> int\"",
            "\"parameters\":[{\"name\":\"a\",\"type\":\"int\"},{\"name\":\"b\",\"type\":\"int\"}]",
            "\"returnType\":\"int\"",
            // A declared type-parameter binder reaches the outline, variance
            // markers included ([TYPE-GENERICS-DECL]).
            "\"signature\":\"type Box<T>\"",
            "\"signature\":\"effect Feed<out T>\"",
            "\"signature\":\"fn pick<T, out U>(a: T, b: U) -> T\"",
            "\"name\":\"main\",\"kind\":\"function\",\"type\":\"fn main() -> Unit\",\"line\":9",
        ] {
            assert!(json.contains(frag), "missing {frag} in {json}");
        }
    }

    /// The rendered hover markdown for `name` in `src`, via the real symbol
    /// path (`collect_all_symbols` → `doc`).
    fn doc_for(src: &str, name: &str) -> Option<String> {
        let parsed = osprey_syntax::parse_program(src);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        collect_all_symbols(&parsed.program)
            .into_iter()
            .find(|s| s.name == name)
            .and_then(|s| s.doc)
    }

    #[test]
    fn doc_comments_reach_hover_for_every_declaration_kind() {
        // Default flavor: fn, type, effect each carry their /// doc into hover.
        let src = "/// Doubles the input.\n\
                   fn double(x) = x * 2\n\
                   /// A performance tier.\n\
                   type Tier = Epic | Solid\n\
                   /// Emits a line.\n\
                   effect Console { emit: fn(string) -> Unit }\n";
        assert!(doc_for(src, "double").is_some_and(|d| d.contains("Doubles the input.")));
        assert!(doc_for(src, "Tier").is_some_and(|d| d.contains("A performance tier.")));
        assert!(doc_for(src, "Console").is_some_and(|d| d.contains("Emits a line.")));
    }

    #[test]
    fn structured_sections_render_in_hover() {
        let src = "/// Divides two numbers.\n\
                   ///\n\
                   /// # Parameters\n\
                   /// - a: the numerator\n\
                   ///\n\
                   /// # Returns\n\
                   /// the quotient\n\
                   fn div(a, b) = intDiv(a, b)\n";
        let d = doc_for(src, "div").expect("div doc");
        assert!(d.contains("Divides two numbers."), "{d}");
        assert!(d.contains("**Parameters**") && d.contains("`a`"), "{d}");
        assert!(
            d.contains("**Returns**") && d.contains("the quotient"),
            "{d}"
        );
    }

    #[test]
    fn ml_flavor_doc_comments_reach_hover() {
        // The ML (** … *) doc form lowers to the same DocComment and renders
        // identically ([DOC-SIGIL-ML]).
        let src = "(** Doubles the input. *)\n\
                   double x = x * 2\n\
                   (** A performance tier. *)\n\
                   type Tier =\n    Epic\n    Solid\n";
        let parsed = osprey_syntax::parse_program_with_flavor(src, osprey_syntax::Flavor::Ml);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let syms = collect_all_symbols(&parsed.program);
        let doc = syms
            .iter()
            .find(|s| s.name == "double")
            .and_then(|s| s.doc.clone());
        assert!(
            doc.is_some_and(|d| d.contains("Doubles the input.")),
            "ml fn doc"
        );
        let tdoc = syms
            .iter()
            .find(|s| s.name == "Tier")
            .and_then(|s| s.doc.clone());
        assert!(
            tdoc.is_some_and(|d| d.contains("A performance tier.")),
            "ml type doc"
        );
    }

    #[test]
    fn hover_renders_builtin_signature_and_rejects_unknowns() {
        let md = builtin_hover("print");
        // The rich hover carries the call signature and the description, not just
        // a bare `name : type` line.
        assert!(
            md.as_deref()
                .is_some_and(|m| m.contains("print(value: any) -> Unit") && m.contains("Prints")),
            "{md:?}"
        );
        assert!(builtin_hover("notARealBuiltin").is_none());
    }

    #[test]
    fn json_strings_escape_quotes_and_control_chars() {
        assert_eq!(json_str("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
        assert_eq!(json_str("\u{1}"), "\"\\u0001\"");
        // Carriage return and tab get their own short escapes.
        assert_eq!(json_str("a\rb\tc"), "\"a\\rb\\tc\"");
    }

    #[test]
    fn render_type_covers_named_array_generic_and_function_forms() {
        use osprey_ast::TypeExpr;
        // A bare named type renders as its name.
        assert_eq!(render_type(&TypeExpr::named("int")), "int");

        // An array type renders as `[element]`, and the empty array as `[]`.
        let mut array = TypeExpr::named("");
        array.is_array = true;
        array.array_element = Some(Box::new(TypeExpr::named("string")));
        assert_eq!(render_type(&array), "[string]");
        let mut bare_array = TypeExpr::named("");
        bare_array.is_array = true;
        assert_eq!(render_type(&bare_array), "[]");

        // A generic type renders as `Name<args>`.
        let mut generic = TypeExpr::named("Result");
        generic.generic_params = vec![TypeExpr::named("int"), TypeExpr::named("string")];
        assert_eq!(render_type(&generic), "Result<int, string>");

        // A function type renders parameter and return types; a missing return
        // type defaults to `Unit`.
        let mut func = TypeExpr::named("");
        func.is_function = true;
        func.parameter_types = vec![TypeExpr::named("int")];
        func.return_type = Some(Box::new(TypeExpr::named("bool")));
        assert_eq!(render_type(&func), "fn(int) -> bool");
        let mut func_unit = TypeExpr::named("");
        func_unit.is_function = true;
        assert_eq!(render_type(&func_unit), "fn() -> Unit");
    }

    #[test]
    fn collect_symbols_qualifies_module_members_and_skips_non_declarations() {
        // [MODULES-ABI] A flat wire outline keeps collision-safe source names:
        // the module itself plus qualified members, never flattened leaves.
        let parsed = osprey_syntax::parse_program(
            "module Inner {\n  fn helper() -> int = 1\n  let seed = 2\n}\n",
        );
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let syms = collect_symbols(&parsed.program);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["Inner", "Inner::helper", "Inner::seed"]);
        // The `let` carries no annotation, so its rendered type is empty and its
        // kind is `Variable`.
        let seed = syms
            .iter()
            .find(|s| s.name == "Inner::seed")
            .expect("seed symbol");
        assert_eq!(seed.kind, SymbolKind::Variable);
        assert_eq!(seed.ty, "");
    }

    #[test]
    fn namespace_module_and_signature_symbols_never_flatten_collisions() {
        // [MODULES-NAMESPACE] Two modules may export the same leaf name. The
        // outline preserves both ownership paths and the container kinds.
        let parsed = osprey_syntax::parse_program(
            "namespace sales { module Tax { export fn rate() = 10 } }\n\
             namespace payroll { module Tax { export fn rate() = 20 } }\n\
             signature TaxSig { fn rate() -> int }\n",
        );
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
        let symbols = collect_symbols(&parsed.program);
        let named = |name: &str| symbols.iter().find(|symbol| symbol.name == name);
        assert_eq!(named("sales").map(|s| s.kind), Some(SymbolKind::Namespace));
        assert_eq!(
            named("sales::Tax").map(|s| s.kind),
            Some(SymbolKind::Module)
        );
        assert!(named("sales::Tax::rate").is_some());
        assert!(named("payroll::Tax::rate").is_some());
        assert_eq!(named("TaxSig").map(|s| s.kind), Some(SymbolKind::Signature));
    }

    /// One of every container `Expr` variant, each holding a block whose single
    /// `let` is named for the slot it sits in — the fixture for the deep-walker
    /// test below. Implements [LSP-HOVER-VARIABLES]
    #[expect(
        clippy::too_many_lines,
        reason = "exhaustive fixture: one arm per AST container variant is the point"
    )]
    fn every_container_with_a_nested_let() -> Vec<osprey_ast::Expr> {
        use osprey_ast::{
            Expr, FieldAssignment, HandlerArm, MapEntry, MatchArm, NamedArgument, Pattern,
        };
        let blk = |name: &str| Expr::Block {
            statements: vec![Stmt::Let {
                name: name.into(),
                mutable: false,
                ty: None,
                value: Expr::Integer(0),
                doc: None,
                position: Some(Position { line: 1, column: 0 }),
            }],
            value: None,
        };
        let b = |name: &str| Box::new(blk(name));
        let narg = |name: &str| NamedArgument {
            name: "n".into(),
            value: blk(name),
        };
        let field = |name: &str| FieldAssignment {
            name: "f".into(),
            value: blk(name),
        };
        let arm = |name: &str| MatchArm {
            pattern: Pattern::Wildcard,
            body: blk(name),
        };
        vec![
            Expr::List(vec![blk("list")]),
            Expr::Map(vec![MapEntry {
                key: blk("mapk"),
                value: blk("mapv"),
            }]),
            Expr::Object(vec![field("obj")]),
            Expr::Binary {
                op: "+".into(),
                left: b("binl"),
                right: b("binr"),
            },
            Expr::Pipe {
                left: b("pipel"),
                right: b("piper"),
            },
            Expr::Unary {
                op: "-".into(),
                operand: b("unary"),
            },
            Expr::InterpolatedStr(vec![InterpolatedPart::Expr(blk("interp"))]),
            Expr::Call {
                function: b("callfn"),
                arguments: vec![blk("callarg")],
                named_arguments: vec![narg("callnamed")],
            },
            Expr::MethodCall {
                target: b("mtarget"),
                method: "m".into(),
                arguments: vec![blk("marg")],
                named_arguments: vec![narg("mnamed")],
            },
            Expr::FieldAccess {
                target: b("fatarget"),
                field: "f".into(),
            },
            Expr::Index {
                target: b("idxt"),
                index: b("idxi"),
            },
            Expr::Lambda {
                parameters: Vec::new(),
                return_type: None,
                body: b("lambda"),
                position: None,
            },
            Expr::Match {
                value: b("matchval"),
                arms: vec![arm("matcharm")],
            },
            Expr::TypeConstructor {
                name: "T".into(),
                type_args: Vec::new(),
                fields: vec![field("tc")],
            },
            Expr::Update {
                record: "r".into(),
                fields: vec![field("update")],
            },
            Expr::Spawn(b("spawn")),
            Expr::Await(b("await")),
            Expr::Recv(b("recv")),
            Expr::Yield(Some(b("yield"))),
            Expr::Send {
                channel: b("sendc"),
                value: b("sendv"),
            },
            Expr::Select {
                arms: vec![arm("select")],
            },
            Expr::Perform {
                effect: "E".into(),
                operation: "op".into(),
                arguments: vec![blk("perform")],
                named_arguments: vec![narg("performnamed")],
                position: None,
            },
            Expr::Handler {
                effect: "E".into(),
                arms: vec![HandlerArm {
                    operation: "op".into(),
                    params: Vec::new(),
                    body: blk("handlerarm"),
                }],
                body: b("handlerbody"),
                position: None,
            },
        ]
    }

    #[test]
    fn collect_all_symbols_descends_into_every_expression_form() {
        // A `let` is buried inside each container expression variant; the deep
        // collector must surface every one — this exercises all walker arms.
        // Implements [LSP-HOVER-VARIABLES]
        let program = Program {
            statements: every_container_with_a_nested_let()
                .into_iter()
                .map(|value| Stmt::Expr {
                    value,
                    position: None,
                })
                .collect(),
        };
        let found: Vec<String> = collect_all_symbols(&program)
            .into_iter()
            .map(|s| s.name)
            .collect();
        for expected in [
            "list",
            "mapk",
            "mapv",
            "obj",
            "binl",
            "binr",
            "pipel",
            "piper",
            "unary",
            "interp",
            "callfn",
            "callarg",
            "callnamed",
            "mtarget",
            "marg",
            "mnamed",
            "fatarget",
            "idxt",
            "idxi",
            "lambda",
            "matchval",
            "matcharm",
            "tc",
            "update",
            "spawn",
            "await",
            "recv",
            "yield",
            "sendc",
            "sendv",
            "select",
            "perform",
            "performnamed",
            "handlerarm",
            "handlerbody",
        ] {
            assert!(found.iter().any(|n| n == expected), "missing `{expected}`");
        }
    }

    #[test]
    fn symbol_kind_as_str_round_trips_each_variant() {
        assert_eq!(SymbolKind::Namespace.as_str(), "namespace");
        assert_eq!(SymbolKind::Module.as_str(), "module");
        assert_eq!(SymbolKind::Signature.as_str(), "signature");
        assert_eq!(SymbolKind::Function.as_str(), "function");
        assert_eq!(SymbolKind::Variable.as_str(), "variable");
        assert_eq!(SymbolKind::Type.as_str(), "type");
    }
}
