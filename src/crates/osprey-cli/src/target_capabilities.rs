//! Reject target facilities before code generation or toolchain discovery.
//! Implements [IOS-TARGET-CAPABILITIES], [ANDROID-TARGET-CAPABILITIES] and [WASM-TARGET-CAPABILITIES].
use osprey_ast::{
    stmt_meta, walk_program, AstVisitor, Expr, OperationTable, Position, Program, Stmt,
};
use std::collections::BTreeSet;

const FIBER_FNS: &[&str] = &[
    "await",
    "fiberDone",
    "yield",
    "fiber_yield",
    "Channel",
    "send",
    "recv",
    "sleep",
];
const TERMINAL_FNS: &[&str] = &[
    "termRawMode",
    "termCols",
    "termRows",
    "termReadKey",
    "termClear",
    "termMoveCursor",
    "termHideCursor",
    "termShowCursor",
];
const WEB_IMPORTS: &[&str] = &["osprey_web_render", "osprey_web_command"];
const WASI_LIBC_IMPORTS: &[&str] = &["perror", "abort"];

pub(crate) fn validate(program: &Program, target: &str) -> Result<(), String> {
    if !matches!(target, "wasm32" | "python") && !crate::toolchain::host_archive_target(target) {
        return Ok(());
    }
    let mut visitor = Capabilities {
        target,
        position: None,
        error: None,
        required: required_builtins(program),
        operations: OperationTable::collect(program),
    };
    walk_program(program, &mut visitor);
    visitor.error.map_or(Ok(()), Err)?;
    if target == "wasm32" {
        validate_wasm_abi(program)?;
    }
    Ok(())
}

/// An app-library target cannot honour a link directive: the archive holds no
/// library and the host application links every import. Refuse it before any
/// toolchain work rather than defer its discovery to the host's linker.
pub(crate) fn validate_links(source: &str, target: &str) -> Result<(), String> {
    if !crate::toolchain::host_archive_target(target) {
        return Ok(());
    }
    crate::linking::unscoped_link_directive(source).map_or(Ok(()), |feature| {
        Err(format!(
            "target `{target}` does not support {feature}; use a supported target or link the library in the host application"
        ))
    })
}

struct Capabilities<'a> {
    target: &'a str,
    position: Option<Position>,
    error: Option<String>,
    required: BTreeSet<String>,
    operations: OperationTable,
}

impl Capabilities<'_> {
    fn reject(&mut self, feature: &str) {
        if self.error.is_some() {
            return;
        }
        let location = self.position.map_or_else(String::new, |position| {
            format!(" near line {}:{}", position.line, position.column + 1)
        });
        self.error = Some(format!(
            "target `{}` does not support {feature}{location}; use a supported target or implement the platform operation in the host application",
            self.target
        ));
    }

    fn builtin(&mut self, name: &str) {
        if !self.required.contains(name) {
            return;
        }
        if let Some(feature) = builtin_feature(self.target, name) {
            self.reject(&format!("{feature} builtin `{name}`"));
        }
    }
}

fn builtin_feature(target: &str, name: &str) -> Option<&'static str> {
    if target == "python" && FIBER_FNS.contains(&name) {
        return Some("fiber concurrency");
    }
    let groups = [
        (crate::sandbox::HTTP_FNS, "HTTP"),
        (crate::sandbox::WEBSOCKET_FNS, "WebSocket"),
        (crate::sandbox::PROCESS_FNS, "process spawning"),
    ];
    groups
        .iter()
        .find(|(names, _)| names.contains(&name))
        .map(|(_, label)| *label)
        .or_else(|| (target == "wasm32").then(|| wasm_builtin(name)).flatten())
}

/// Lexical free-variable analysis distinguishes platform builtins from local
/// values, parameters, pattern binders and explicitly declared host imports.
fn required_builtins(program: &Program) -> BTreeSet<String> {
    let mut normalized = program.clone();
    for statement in &mut normalized.statements {
        osprey_ast::mutate::statement_children_mut(statement, &mut normalize_methods);
    }
    let mut collector = FunctionUses {
        bound: global_bindings(&normalized.statements),
        required: statement_uses(&normalized.statements),
    };
    walk_program(&normalized, &mut collector);
    collector.required
}

fn statement_uses(statements: &[Stmt]) -> BTreeSet<String> {
    let mut required = BTreeSet::new();
    osprey_ast::freevars::free_idents_of_stmts(statements, None, &mut required);
    for statement in statements {
        if let Stmt::Function { name, .. } | Stmt::Extern { name, .. } = statement {
            let _ = required.remove(name);
        }
    }
    required
}

fn global_bindings(statements: &[Stmt]) -> BTreeSet<String> {
    statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Let { name, .. } | Stmt::Function { name, .. } | Stmt::Extern { name, .. } => {
                Some(name.clone())
            }
            _ => None,
        })
        .collect()
}

struct FunctionUses {
    bound: BTreeSet<String>,
    required: BTreeSet<String>,
}

impl AstVisitor for FunctionUses {
    fn statement(&mut self, statement: &Stmt) {
        if let Stmt::Function {
            parameters, body, ..
        } = statement
        {
            let mut names = BTreeSet::new();
            osprey_ast::freevars::free_idents(body, &mut names);
            names.retain(|name| {
                !self.bound.contains(name)
                    && !parameters.iter().any(|parameter| &parameter.name == name)
            });
            self.required.extend(names);
        }
    }
}

fn normalize_methods(expression: &mut Expr) {
    osprey_ast::mutate::children_mut(expression, &mut normalize_methods);
    if let Expr::MethodCall {
        target,
        method,
        arguments,
        named_arguments,
    } = expression
    {
        *expression = Expr::Call {
            function: Box::new(Expr::Identifier(method.clone())),
            arguments: std::iter::once((**target).clone())
                .chain(arguments.iter().cloned())
                .collect(),
            named_arguments: named_arguments.clone(),
        };
    }
}

fn validate_wasm_abi(program: &Program) -> Result<(), String> {
    let boundaries: Vec<_> = program
        .statements
        .iter()
        .filter_map(wasm_boundary)
        .collect();
    if boundaries.is_empty() {
        return Ok(());
    }
    let types = osprey_types::infer_program(program);
    for (name, position) in boundaries {
        validate_wasm_signature(&types, name, position)?;
    }
    validate_dispatcher_effects(program)
}

fn validate_dispatcher_effects(program: &Program) -> Result<(), String> {
    let entries = browser_functions(program);
    let names: Vec<_> = entries.iter().map(|(name, _)| *name).collect();
    let errors = osprey_types::check_program_exports(program, &names);
    if errors.is_empty() {
        return Ok(());
    }
    let messages = errors
        .iter()
        .map(|error| error.message.as_str())
        .collect::<Vec<_>>()
        .join("; ");
    let position = entries.first().and_then(|(_, position)| *position);
    Err(format!(
        "target `wasm32` does not support browser ABI with unresolved effects{}: {messages}",
        location(position)
    ))
}

fn browser_functions(program: &Program) -> Vec<(&str, Option<Position>)> {
    program
        .statements
        .iter()
        .filter(|statement| matches!(statement, Stmt::Function { .. }))
        .filter_map(browser_boundary)
        .collect()
}

fn location(position: Option<Position>) -> String {
    position.map_or_else(String::new, |p| {
        format!(" near line {}:{}", p.line, p.column + 1)
    })
}

fn validate_wasm_signature(
    types: &osprey_types::ProgramTypes,
    name: &str,
    position: Option<Position>,
) -> Result<(), String> {
    let (params, result, abi, signature) = wasm_signature(name);
    let expected = (
        params.iter().map(|name| wasm_scalar(name)).collect(),
        wasm_scalar(result),
    );
    if types.functions.get(name) == Some(&expected) {
        return Ok(());
    }
    Err(format!(
        "target `wasm32` does not support {abi} ABI for `{name}`{}: expected {signature}",
        location(position)
    ))
}

// Implements [WASI-LIBC-ABI]: these libc calls need no additional host imports,
// and their exact signatures prevent a source declaration from changing C ABI.
fn wasm_scalar(name: &str) -> osprey_types::Type {
    osprey_types::Type::Con {
        name: name.to_string(),
        args: Vec::new(),
    }
}

fn wasm_signature(
    name: &str,
) -> (
    &'static [&'static str],
    &'static str,
    &'static str,
    &'static str,
) {
    match name {
        "perror" => (&["string"], "Unit", "WASI libc", "(string) -> Unit"),
        "abort" => (&[], "Unit", "WASI libc", "() -> Unit"),
        _ => (&["string"], "int", "browser", "(string) -> int"),
    }
}

fn wasm_boundary(statement: &Stmt) -> Option<(&str, Option<Position>)> {
    match statement {
        Stmt::Extern { name, position, .. } if WASI_LIBC_IMPORTS.contains(&name.as_str()) => {
            Some((name, *position))
        }
        _ => browser_boundary(statement),
    }
}

fn browser_boundary(statement: &Stmt) -> Option<(&str, Option<Position>)> {
    match statement {
        Stmt::Extern { name, position, .. } if WEB_IMPORTS.contains(&name.as_str()) => {
            Some((name, *position))
        }
        Stmt::Function { name, position, .. } => {
            let source = osprey_ast::symbol::demangle(name).unwrap_or_else(|| name.clone());
            (source.rsplit("::").next() == Some("osprey_web_dispatch")).then_some((name, *position))
        }
        _ => None,
    }
}

/// Why THIS target cannot suspend a request. The two targets are blocked for
/// unrelated reasons, and a diagnostic that cites the other one sends the
/// reader to a roadmap that will never move their build.
/// [WASM-TARGET-CAPABILITIES] [IOS-TARGET-CAPABILITIES]
fn continuation_limit(target: &str) -> &'static str {
    if target == "wasm32" {
        "wasm32 acquires one-shot continuations when the stack-switching proposal lands"
    } else {
        "a suspended continuation cannot cross or outlive a synchronous host call"
    }
}

fn wasm_builtin(name: &str) -> Option<&'static str> {
    if FIBER_FNS.contains(&name) {
        Some("fiber concurrency")
    } else if TERMINAL_FNS.contains(&name) {
        Some("terminal control")
    } else {
        None
    }
}

impl AstVisitor for Capabilities<'_> {
    fn statement(&mut self, statement: &Stmt) {
        self.position = stmt_meta::statement_position(statement);
        if let Stmt::Extern { name, .. } = statement {
            if self.target == "wasm32"
                && !WEB_IMPORTS.contains(&name.as_str())
                && !WASI_LIBC_IMPORTS.contains(&name.as_str())
            {
                self.reject(&format!("foreign C import `{name}`"));
            }
        }
    }

    fn expression(&mut self, expression: &Expr) {
        match expression {
            // Declared control operations need continuation support even when
            // an arm abandons without resuming. Implements [MULTI-WASM].
            Expr::Handler { effect, arms, .. } => {
                for arm in arms {
                    if !self
                        .operations
                        .mode_of(osprey_ast::effect_name::base(effect), &arm.operation)
                        .is_control()
                    {
                        continue;
                    }
                    // A module-scoped effect reaches here under its encoded
                    // symbol; the author wrote `Clocks::Clock`, so that is what
                    // the diagnostic must say.
                    let effect = osprey_ast::symbol::demangle_message(effect);
                    let why = continuation_limit(self.target);
                    self.reject(&format!(
                        "a continuation for `{effect}.{}` ({why})",
                        arm.operation
                    ));
                }
            }
            Expr::Spawn(_)
            | Expr::Await(_)
            | Expr::Yield(_)
            | Expr::Send { .. }
            | Expr::Recv(_)
            | Expr::Select { .. }
                if matches!(self.target, "wasm32" | "python") =>
            {
                self.reject("fiber concurrency");
            }
            Expr::Identifier(name) => self.builtin(name),
            Expr::MethodCall { method, .. } => self.builtin(method),
            _ => {}
        }
    }
}
