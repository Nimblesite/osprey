//! Reject target facilities before code generation or toolchain discovery.
//! Implements [IOS-TARGET-CAPABILITIES], [ANDROID-TARGET-CAPABILITIES] and [WASM-TARGET-CAPABILITIES].
use osprey_ast::{contains_resume, walk_program, AstVisitor, Expr, Position, Program, Stmt};
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

pub(crate) fn validate(program: &Program, target: &str) -> Result<(), String> {
    if !matches!(
        target,
        "wasm32" | "ios" | "ios-sim" | "android-arm64" | "android-x64"
    ) {
        return Ok(());
    }
    let mut visitor = Capabilities {
        target,
        position: None,
        error: None,
        required: required_builtins(program),
    };
    walk_program(program, &mut visitor);
    visitor.error.map_or(Ok(()), Err)?;
    if target == "wasm32" {
        validate_browser_abi(program)?;
    }
    Ok(())
}

struct Capabilities<'a> {
    target: &'a str,
    position: Option<Position>,
    error: Option<String>,
    required: BTreeSet<String>,
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

fn validate_browser_abi(program: &Program) -> Result<(), String> {
    let boundaries: Vec<_> = program
        .statements
        .iter()
        .filter_map(browser_boundary)
        .collect();
    if boundaries.is_empty() {
        return Ok(());
    }
    let types = osprey_types::infer_program(program);
    for (name, position) in boundaries {
        validate_browser_signature(&types, name, position)?;
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

fn validate_browser_signature(
    types: &osprey_types::ProgramTypes,
    name: &str,
    position: Option<Position>,
) -> Result<(), String> {
    let scalar = |name: &str| osprey_types::Type::Con {
        name: name.to_string(),
        args: Vec::new(),
    };
    let expected = (vec![scalar("string")], scalar("int"));
    if types.functions.get(name) == Some(&expected) {
        return Ok(());
    }
    let location = position.map_or_else(String::new, |p| {
        format!(" near line {}:{}", p.line, p.column + 1)
    });
    Err(format!("target `wasm32` does not support browser ABI for `{name}`{location}: expected (string) -> int"))
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
        self.position = statement_position(statement);
        if let Stmt::Extern { name, .. } = statement {
            if self.target == "wasm32" && !WEB_IMPORTS.contains(&name.as_str()) {
                self.reject(&format!("foreign C import `{name}`"));
            }
        }
    }

    fn expression(&mut self, expression: &Expr) {
        match expression {
            // A resuming ARM is what needs a continuation, so the rejection
            // names the operation whose request cannot be suspended rather than
            // the `resume` keyword — a row that cannot say which effects will
            // start working is a row that cannot be planned against. A static or
            // substituting arm needs no continuation and compiles. The permanent
            // wording `many` deserves waits until `many` runs on any target: the
            // checker rejects it before this gate is reached (phase 5 of
            // docs/plans/0028-resumption-multiplicity.md). Implements [MULTI-WASM].
            Expr::Handler { effect, arms, .. } => {
                for arm in arms.iter().filter(|arm| contains_resume(&arm.body)) {
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
                if self.target == "wasm32" =>
            {
                self.reject("fiber concurrency");
            }
            Expr::Identifier(name) => self.builtin(name),
            Expr::MethodCall { method, .. } => self.builtin(method),
            _ => {}
        }
    }
}

fn statement_position(statement: &Stmt) -> Option<Position> {
    match statement {
        Stmt::Let { position, .. }
        | Stmt::Assignment { position, .. }
        | Stmt::Function { position, .. }
        | Stmt::Extern { position, .. }
        | Stmt::Type { position, .. }
        | Stmt::Effect { position, .. }
        | Stmt::Module { position, .. }
        | Stmt::Namespace { position, .. }
        | Stmt::Signature { position, .. }
        | Stmt::Expr { position, .. } => *position,
        Stmt::Import(import) => import.position,
    }
}
