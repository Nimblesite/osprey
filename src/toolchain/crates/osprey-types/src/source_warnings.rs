//! The advice read from one source file as written, before project assembly
//! flattens its modules away: undocumented and unexampled exports
//! ([DOC-EXPORT-REQUIRED], [DOC-EXPORT-EXAMPLE-REQUIRED]), repeated literals
//! ([TYPE-WARNINGS-REPEATED-LITERAL]), consecutive prints
//! ([TYPE-WARNINGS-CONSECUTIVE-PRINT]) and unread private module members
//! ([TYPE-WARNINGS-UNUSED-PRIVATE]). The compiler and the editor both ask
//! here, so a file gets the same advice in each.

use osprey_ast::Program;

use crate::TypeWarning;

/// Every per-file warning of `program`, documentation first.
#[must_use]
pub fn source_warnings(program: &Program) -> Vec<TypeWarning> {
    let mut warnings = crate::documentation_warnings(program);
    warnings.extend(crate::repeated_literals(program));
    warnings.extend(crate::consecutive_prints(program));
    warnings.extend(crate::unused_privates(program));
    warnings
}
