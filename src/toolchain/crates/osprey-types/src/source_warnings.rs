//! The advice read from one source file as written, before project assembly
//! flattens its modules away: undocumented and unexampled exports
//! ([DOC-EXPORT-REQUIRED], [DOC-EXPORT-EXAMPLE-REQUIRED]), repeated literals
//! ([TYPE-WARNINGS-REPEATED-LITERAL]), consecutive prints
//! ([TYPE-WARNINGS-CONSECUTIVE-PRINT]), callback pyramids ([LINT-NESTING-LAMBDA])
//! and unread private module members ([TYPE-WARNINGS-UNUSED-PRIVATE]). The
//! compiler and the editor both ask here, so a file gets the same advice in each.

use std::path::Path;

use osprey_ast::Program;
use osprey_syntax::Flavor;

use crate::TypeWarning;

/// Folders whose sources serve a suite, not readers of an API.
const TEST_FOLDERS: [&str; 2] = ["tests", "test"];

/// A source under a `tests` or `test` folder is test support: its exports
/// are documented but need no examples ([DOC-EXPORT-EXAMPLE-TEST-SUPPORT]).
fn is_test_support(path: &Path) -> bool {
    path.parent().is_some_and(|folder| {
        folder
            .components()
            .any(|part| TEST_FOLDERS.iter().any(|name| part.as_os_str() == *name))
    })
}

/// The rule a run of closing brackets raises.
pub(crate) const CLOSING_RUN: &str = "closing-run";

/// Every per-file warning of `program`, parsed from `source` in `flavor` and
/// read from `path`, documentation first.
#[must_use]
pub fn source_warnings(
    program: &Program,
    source: &str,
    flavor: Flavor,
    path: &Path,
) -> Vec<TypeWarning> {
    let mut warnings = crate::documentation_warnings(program);
    if is_test_support(path) {
        warnings.retain(|warning| warning.rule != crate::UNEXAMPLED_EXPORT);
    }
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
