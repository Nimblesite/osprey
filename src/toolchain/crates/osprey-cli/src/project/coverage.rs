//! Source initialization counters for bindings erased by project constant folding.
use super::{CompilationInput, CompilationUnit};
use osprey_ast::{Expr, Program, Stmt};
use std::borrow::Cow;

impl CompilationInput {
    /// Execute folded initializers' source counters once at entry, retaining
    /// callable identity and normal builds. Implements [TESTING-PROJECT-SUITE].
    pub(crate) fn codegen_program(&self, coverage: bool) -> Cow<'_, Program> {
        let CompilationUnit::Project(project) = &self.unit else {
            return Cow::Borrowed(self.backend_program());
        };
        if !coverage || project.documentation_bindings.is_empty() {
            return Cow::Borrowed(self.backend_program());
        }
        let mut program = self.backend_program().clone();
        let markers = project
            .documentation_bindings
            .iter()
            .filter_map(initializer)
            .collect();
        prepend(&mut program, markers);
        Cow::Owned(program)
    }
}

fn unit() -> Expr {
    Expr::Block {
        statements: Vec::new(),
        value: None,
        position: None,
    }
}

fn initializer(statement: &Stmt) -> Option<Stmt> {
    match statement {
        Stmt::Let {
            position: Some(position),
            ..
        } => Some(Stmt::Expr {
            value: unit(),
            doc: None,
            position: Some(*position),
        }),
        _ => None,
    }
}

fn main_body(statement: &mut Stmt) -> Option<&mut Expr> {
    match statement {
        Stmt::Function { name, body, .. } if name == "main" => Some(body),
        _ => None,
    }
}

fn prepend(program: &mut Program, statements: Vec<Stmt>) {
    if let Some(body) = program.statements.iter_mut().find_map(main_body) {
        let previous = std::mem::replace(body, unit());
        *body = Expr::Block {
            statements,
            value: Some(Box::new(previous)),
            position: None,
        };
    } else {
        let _ = program.statements.splice(0..0, statements);
    }
}
