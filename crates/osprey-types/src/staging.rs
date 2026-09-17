//! Validate staged data contracts before any operation or unused arm disappears.

use crate::TypeError;
use osprey_ast::{walk_program, AstVisitor, Expr, Program, Stmt};

/// Type-check the original staged program, then discharge its static handlers.
/// Call after module assembly so imported declarations and bindings are known.
/// The residual program still requires the ordinary type/effect check.
/// Implements [STAGE-LOWER-ORDER-PHASE].
///
/// # Errors
/// Returns source type errors or violations of the staging rules.
pub fn lower_static_checked(program: &Program) -> Result<Program, Vec<TypeError>> {
    if has_staging(program) {
        let errors = crate::check::check_data_contracts(program);
        if !errors.is_empty() {
            return Err(errors);
        }
    }
    osprey_ast::stage::discharge(program).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| TypeError::new(error.message).with_pos(error.position))
            .collect()
    })
}

pub(crate) fn has_staging(program: &Program) -> bool {
    struct Staging(bool);
    impl AstVisitor for Staging {
        fn statement(&mut self, statement: &Stmt) {
            if let Stmt::Effect { stage, .. } = statement {
                self.0 |= stage.is_compile_time();
            }
        }
        fn expression(&mut self, expression: &Expr) {
            if let Expr::Handler { stage, .. } = expression {
                self.0 |= stage.is_compile_time();
            }
        }
    }
    let mut staging = Staging(false);
    walk_program(program, &mut staging);
    staging.0
}
