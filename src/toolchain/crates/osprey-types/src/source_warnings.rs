//! The advice read from one source file as written, before project assembly
//! flattens its modules away: undocumented and unexampled exports
//! ([DOC-EXPORT-REQUIRED], [DOC-EXPORT-EXAMPLE-REQUIRED]), repeated literals
//! ([TYPE-WARNINGS-REPEATED-LITERAL]), consecutive prints
//! ([TYPE-WARNINGS-CONSECUTIVE-PRINT]), callback pyramids ([LINT-NESTING-LAMBDA])
//! and unread private module members ([TYPE-WARNINGS-UNUSED-PRIVATE]). The
//! compiler and the editor both ask here, so a file gets the same advice in each.

use osprey_ast::Program;
use osprey_syntax::Flavor;

use crate::TypeWarning;

/// The rule a run of closing brackets raises.
const CLOSING_RUN: &str = "closing-run";

/// Every per-file warning of `program`, parsed from `source` in `flavor`,
/// documentation first.
#[must_use]
pub fn source_warnings(program: &Program, source: &str, flavor: Flavor) -> Vec<TypeWarning> {
    let mut warnings = crate::documentation_warnings(program);
    warnings.extend(crate::repeated_literals(program));
    warnings.extend(crate::consecutive_prints(program));
    warnings.extend(closing_runs(source, flavor));
    warnings.extend(crate::unused_privates(program));
    warnings
}

/// One warning where each run of closing brackets starts: a run of `n`
/// brackets ends code nested at least `n` deep, callbacks or data alike.
fn closing_runs(source: &str, flavor: Flavor) -> Vec<TypeWarning> {
    osprey_syntax::closing_runs(source, flavor)
        .into_iter()
        .map(|run| TypeWarning {
            message: format!(
                "{} brackets close here at once: this code nests {} deep; flatten its callbacks into `use x <- f(...)` lines and name its inner values",
                run.length, run.length
            ),
            position: Some(run.position),
            rule: CLOSING_RUN,
        })
        .collect()
}
