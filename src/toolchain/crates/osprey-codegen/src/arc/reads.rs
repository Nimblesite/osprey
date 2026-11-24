//! How a statement reads the names it mentions [GC-ARC-PERCEUS]: the most
//! times any one path reads each, and whether a read may not run. A binding
//! may give its value away only where no path reads it twice
//! ([`super::moves`]).
use osprey_ast::{Expr, InterpolatedPart, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// Operators that always evaluate both operands, each once.
const STRICT: [&str; 11] = ["+", "-", "*", "/", "%", "==", "!=", "<", "<=", ">", ">="];
/// "More than once": every larger count means the same.
const MANY: u8 = 2;

/// The reads one expression makes.
#[derive(Default)]
pub(super) struct Reads {
    /// The most reads of each name along any one path, capped at [`MANY`].
    most: BTreeMap<String, u8>,
    /// Names with a read that runs on only some paths.
    sometimes: BTreeSet<String>,
    /// Whether what is being visited runs on only some paths.
    branching: bool,
}

/// The expression an interpolated part embeds, if it embeds one.
fn embedded(part: &InterpolatedPart) -> Option<&Expr> {
    match part {
        InterpolatedPart::Expr(e) => Some(e),
        InterpolatedPart::Text(_) => None,
    }
}

impl Reads {
    pub(super) fn of(value: &Expr) -> Reads {
        let mut reads = Reads::default();
        reads.visit(value);
        reads
    }

    /// Names read, with whether some path reads them more than once.
    pub(super) fn counted(&self) -> impl Iterator<Item = (&String, bool)> {
        self.most.iter().map(|(name, most)| (name, *most >= MANY))
    }

    /// Whether a read of `name` runs on only some paths.
    pub(super) fn sometimes(&self, name: &str) -> bool {
        self.sometimes.contains(name)
    }

    /// `name` is read `count` more times on the path being visited.
    pub(super) fn add(&mut self, name: &str, count: u8) {
        let most = self.most.entry(name.to_owned()).or_default();
        *most = most.saturating_add(count).min(MANY);
        if self.branching {
            let _ = self.sometimes.insert(name.to_owned());
        }
    }

    /// Every name `e` mentions is read `count` times: once each for what a
    /// closure captures, [`MANY`] for a form that may repeat or defer.
    fn mention(&mut self, e: &Expr, count: u8) {
        let mut free = BTreeSet::new();
        osprey_ast::freevars::free_idents(e, &mut free);
        for name in &free {
            self.add(name, count);
        }
    }

    fn visit(&mut self, e: &Expr) {
        match e {
            Expr::Integer(_) | Expr::Float(_) | Expr::Str(_) | Expr::Bool(_) => {}
            Expr::Identifier(name) => self.add(name, 1),
            // A closure cell is built once and takes each capture once.
            Expr::Lambda { .. } => self.mention(e, 1),
            Expr::List(items, _) => self.visit_all(items),
            Expr::InterpolatedStr(parts) => self.visit_all(parts.iter().filter_map(embedded)),
            Expr::FieldAccess { target, .. } => self.visit(target),
            Expr::Unary { operand, .. } => self.visit(operand),
            Expr::TypeConstructor { name, fields, .. } => {
                self.add(name, MANY);
                self.visit_all(fields.iter().map(|field| &field.value));
            }
            // An updated copy reads its base once, then each new field.
            Expr::Update { record, fields } => {
                self.add(record, 1);
                self.visit_all(fields.iter().map(|field| &field.value));
            }
            _ => self.visit_control(e),
        }
    }

    /// Calls, and the forms that choose between paths.
    fn visit_control(&mut self, e: &Expr) {
        match e {
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => {
                self.visit(function);
                self.visit_all(arguments);
                self.visit_all(named_arguments.iter().map(|argument| &argument.value));
            }
            Expr::Binary {
                op, left, right, ..
            } => self.binary(op, left, right),
            Expr::Match { value, arms } => {
                self.visit(value);
                self.branches(arms.iter().map(|arm| &arm.body));
            }
            Expr::Block {
                statements, value, ..
            } => self.block(statements, value.as_deref()),
            _ => self.mention(e, MANY),
        }
    }

    fn visit_all<'a>(&mut self, operands: impl IntoIterator<Item = &'a Expr>) {
        for operand in operands {
            self.visit(operand);
        }
    }

    /// A strict operator runs both operands; any other may skip its right one.
    fn binary(&mut self, op: &str, left: &Expr, right: &Expr) {
        self.visit(left);
        if STRICT.contains(&op) {
            self.visit(right);
        } else {
            self.branches([right]);
        }
    }

    /// Exactly one of `bodies` runs, or none: a path reads each name as often
    /// as the body that reads it most.
    fn branches<'a>(&mut self, bodies: impl IntoIterator<Item = &'a Expr>) {
        let mut joined: BTreeMap<String, u8> = BTreeMap::new();
        for body in bodies {
            for (name, most) in Reads::of(body).most {
                let widest = joined.entry(name).or_default();
                *widest = (*widest).max(most);
            }
        }
        let outer = std::mem::replace(&mut self.branching, true);
        for (name, most) in joined {
            self.add(&name, most);
        }
        self.branching = outer;
    }

    /// A block runs its statements in order, then its tail.
    fn block(&mut self, statements: &[Stmt], tail: Option<&Expr>) {
        for statement in statements {
            match statement {
                Stmt::Let { value, .. } | Stmt::Expr { value, .. } => self.visit(value),
                other => self.mention_statement(other),
            }
        }
        self.visit_all(tail);
    }

    /// Any other statement may read what it mentions any number of times.
    fn mention_statement(&mut self, statement: &Stmt) {
        let mut free = BTreeSet::new();
        osprey_ast::freevars::free_idents_of_stmts(
            std::slice::from_ref(statement),
            None,
            &mut free,
        );
        for name in &free {
            self.add(name, MANY);
        }
    }
}
