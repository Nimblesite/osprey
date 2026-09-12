//! Read-based lexical diagnostics. Implements [TYPE-WARNINGS-UNUSED].
//!
//! Bindings carry identities, so a shadowing declaration cannot accidentally
//! count as a read of its namesake. Only expression-local bindings are
//! candidates: a top-level name can be an exported or native entry point.

use std::collections::{HashMap, HashSet};

use osprey_ast::{AstNode, AstVisitor, Expr, Parameter, Position, Program, Stmt};

use crate::{check::infer_checked, methods, pattern::pattern_binders_where, TypeWarning};

/// The source binding form behind an unused-symbol diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnusedKind {
    /// A lexical `let` or `mut` binding.
    Variable,
    /// A function or lambda parameter.
    Parameter,
    /// A match/select pattern binder.
    PatternBinding,
    /// An operation parameter in a handler arm.
    HandlerParameter,
}

impl UnusedKind {
    /// Stable diagnostic code used by compiler and editor clients.
    #[must_use]
    pub const fn rule(self) -> &'static str {
        self.description().0
    }

    fn label(self) -> &'static str {
        self.description().1
    }

    const fn description(self) -> (&'static str, &'static str) {
        match self {
            Self::Variable => ("unused-variable", "variable"),
            Self::Parameter => ("unused-parameter", "parameter"),
            Self::PatternBinding => ("unused-pattern-binding", "pattern binding"),
            Self::HandlerParameter => ("unused-handler-parameter", "handler parameter"),
        }
    }
}

/// An unused binding and the source identity needed to locate its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnusedSymbol {
    /// Compiler diagnostic, anchored to the owning source declaration.
    pub warning: TypeWarning,
    /// The source name, independent of the rendered message.
    pub name: String,
    /// The declaration form clients should locate.
    pub kind: UnusedKind,
    /// `let`, function/lambda, handler-arm, or top-level expression position;
    /// patterns inherit the nearest owner because they carry no AST position.
    pub owner_position: Option<Position>,
    /// Earlier binders with this owner, kind, and name, including used ones.
    /// This disambiguates repeated names in separate arms of the same owner.
    pub occurrence: usize,
}

struct Binding {
    symbol: UnusedSymbol,
    used: bool,
}

#[derive(Clone, Copy)]
struct Owner<'a> {
    name: &'a str,
    position: Option<Position>,
}

struct Analysis {
    bindings: Vec<Binding>,
    scope: Vec<usize>,
    method_reads: HashMap<usize, String>,
    constructors: HashSet<String>,
}

/// Report unread lexical symbols in declaration order in a valid program.
/// Names beginning with `_`, compiler-generated names, and declarations
/// outside expression scopes are intentionally excluded. Assigning a mutable
/// local does not read it; closure, handler, and fiber captures do.
#[must_use]
pub fn unused_symbols(program: &Program) -> Vec<UnusedSymbol> {
    let Ok(types) = infer_checked(program) else {
        return Vec::new();
    };
    let mut analysis = Analysis {
        bindings: Vec::new(),
        scope: Vec::new(),
        method_reads: method_reads(program, &types.methods),
        constructors: types.ctors.into_keys().collect(),
    };
    let owner = Owner {
        name: "",
        position: None,
    };
    for statement in &program.statements {
        analysis.statement(statement, owner, false);
    }
    analysis.bindings.into_iter().filter_map(unused).collect()
}

fn unused(binding: Binding) -> Option<UnusedSymbol> {
    let name = &binding.symbol.name;
    (!binding.used && !name.is_empty() && !name.starts_with(['_', '$'])).then_some(binding.symbol)
}

impl Analysis {
    fn bind(&mut self, name: &str, kind: UnusedKind, owner: Owner<'_>) {
        let index = self.declare(name, kind, owner);
        self.scope.push(index);
    }

    fn declare(&mut self, name: &str, kind: UnusedKind, owner: Owner<'_>) -> usize {
        let occurrence = self
            .bindings
            .iter()
            .filter(|binding| {
                let symbol = &binding.symbol;
                symbol.owner_position == owner.position
                    && symbol.kind == kind
                    && symbol.name == name
            })
            .count();
        let index = self.bindings.len();
        self.bindings.push(Binding {
            symbol: symbol(name, kind, owner, occurrence),
            used: false,
        });
        index
    }

    fn read(&mut self, name: &str) {
        for index in self.scope.iter().rev() {
            if let Some(binding) = self.bindings.get_mut(*index) {
                if binding.symbol.name == name {
                    binding.used = true;
                    break;
                }
            }
        }
    }

    fn statement(&mut self, statement: &Stmt, parent: Owner<'_>, local: bool) {
        match statement {
            Stmt::Function {
                name,
                parameters,
                body,
                position,
                ..
            } => {
                let name = osprey_ast::symbol::demangle(name).unwrap_or_else(|| name.clone());
                self.parameters(
                    parameters,
                    body,
                    Owner {
                        name: &name,
                        position: *position,
                    },
                );
            }
            Stmt::Let {
                name,
                value,
                position,
                ..
            } => {
                let owner = Owner {
                    name,
                    position: *position,
                };
                let binding = local.then(|| self.declare(name, UnusedKind::Variable, owner));
                self.expression(value, owner);
                self.scope.extend(binding);
            }
            Stmt::Expr {
                value, position, ..
            } => self.expression(
                value,
                Owner {
                    position: parent.position.or(*position),
                    ..parent
                },
            ),
            Stmt::Assignment { value, .. } => {
                self.expression(value, parent);
            }
            _ => AstNode::Statement(statement).for_each_child(|node| self.node(node, parent)),
        }
    }

    fn node(&mut self, node: AstNode<'_>, owner: Owner<'_>) {
        match node {
            AstNode::Statement(statement) => self.statement(statement, owner, false),
            AstNode::Expression(expression) => self.expression(expression, owner),
        }
    }

    fn parameters(&mut self, parameters: &[Parameter], body: &Expr, owner: Owner<'_>) {
        let depth = self.scope.len();
        for parameter in parameters {
            self.bind(&parameter.name, UnusedKind::Parameter, owner);
        }
        self.expression(source_body(parameters, body), owner);
        self.scope.truncate(depth);
    }

    fn expression(&mut self, expression: &Expr, owner: Owner<'_>) {
        if let Some(name) = self
            .method_reads
            .get(&std::ptr::from_ref(expression).addr())
            .cloned()
        {
            self.read(&name);
        }
        match expression {
            Expr::Identifier(name) => self.read(name),
            Expr::Path(path) => self.read(&path.to_string()),
            Expr::Lambda {
                parameters,
                body,
                position,
                ..
            } => self.parameters(
                parameters,
                body,
                Owner {
                    name: "<lambda>",
                    position: *position,
                },
            ),
            Expr::Block { statements, value } => self.block(statements, value.as_deref(), owner),
            Expr::Match { value, arms } => {
                self.expression(value, owner);
                self.arms(arms, owner);
            }
            Expr::Select { arms } => self.arms(arms, owner),
            Expr::Handler {
                effect, arms, body, ..
            } => self.handler(effect, arms, body, owner),
            _ => self.children(expression, owner),
        }
    }

    fn children(&mut self, expression: &Expr, owner: Owner<'_>) {
        match expression {
            Expr::Update { record, .. } => self.read(record),
            Expr::TypeConstructor { name, .. } if !self.constructors.contains(name) => {
                self.read(name);
            }
            _ => {}
        }
        AstNode::Expression(expression).for_each_child(|node| self.node(node, owner));
    }

    fn block(&mut self, statements: &[Stmt], value: Option<&Expr>, owner: Owner<'_>) {
        let depth = self.scope.len();
        for statement in statements {
            self.statement(statement, owner, true);
        }
        if let Some(value) = value {
            self.expression(value, owner);
        }
        self.scope.truncate(depth);
    }

    fn arms(&mut self, arms: &[osprey_ast::MatchArm], owner: Owner<'_>) {
        for arm in arms {
            let depth = self.scope.len();
            for name in
                pattern_binders_where(&arm.pattern, &|name| !self.constructors.contains(name))
            {
                self.bind(&name, UnusedKind::PatternBinding, owner);
            }
            self.expression(&arm.body, owner);
            self.scope.truncate(depth);
        }
    }

    fn handler(
        &mut self,
        effect: &str,
        arms: &[osprey_ast::HandlerArm],
        body: &Expr,
        owner: Owner<'_>,
    ) {
        let effect = osprey_ast::symbol::demangle_message(effect);
        for arm in arms {
            let depth = self.scope.len();
            let name = format!("{effect}.{}", arm.operation);
            let arm_owner = Owner {
                name: &name,
                position: arm.position,
            };
            for parameter in &arm.params {
                self.bind(parameter, UnusedKind::HandlerParameter, arm_owner);
            }
            self.expression(&arm.body, arm_owner);
            self.scope.truncate(depth);
        }
        self.expression(body, owner);
    }
}

/// Only explicit lowering provenance authorizes skipping constraint aliases.
/// A user-written `let x = x` remains an independent local declaration.
fn source_body<'a>(parameters: &[Parameter], mut body: &'a Expr) -> &'a Expr {
    for _ in parameters
        .iter()
        .filter(|parameter| parameter.inline_constraint)
    {
        if let Expr::Block {
            statements,
            value: Some(value),
        } = body
        {
            if matches!(statements.as_slice(), [Stmt::Let { .. }]) {
                body = value;
            }
        }
    }
    body
}

fn symbol(name: &str, kind: UnusedKind, owner: Owner<'_>, occurrence: usize) -> UnusedSymbol {
    let suffix = if matches!(kind, UnusedKind::HandlerParameter) {
        format!(" of `{}`", owner.name)
    } else {
        String::new()
    };
    UnusedSymbol {
        warning: TypeWarning {
            message: format!("unused {} `{name}`{suffix}", kind.label()),
            position: owner.position,
            rule: kind.rule(),
        },
        name: name.to_owned(),
        kind,
        owner_position: owner.position,
        occurrence,
    }
}

/// Only a resolved UFCS fallback reads the method's value name. A same-named
/// callable record field must not keep an unrelated local binding alive.
fn method_reads(program: &Program, targets: &methods::Targets) -> HashMap<usize, String> {
    struct Collector<'a> {
        targets: &'a methods::Targets,
        index: usize,
        reads: HashMap<usize, String>,
    }
    impl AstVisitor for Collector<'_> {
        fn expression(&mut self, expression: &Expr) {
            if matches!(
                self.targets.get(&self.index),
                Some(methods::Target::Function | methods::Target::Deferred(_))
            ) {
                if let Some(parts) = methods::parts(expression) {
                    let _ = self.reads.insert(
                        std::ptr::from_ref(expression).addr(),
                        parts.method.to_owned(),
                    );
                }
            }
            self.index += 1;
        }
    }
    let mut collector = Collector {
        targets,
        index: 0,
        reads: HashMap::new(),
    };
    osprey_ast::walk_program(program, &mut collector);
    collector.reads
}
