//! A union declared in one namespace is matched in another through its
//! qualified constructors, on both surfaces [MODULES-RESOLUTION] (issue #282).

mod support;

use osprey_project::assemble;
use osprey_syntax::Flavor;
use support::{config, parsed};

const ML_LIBRARY: &str =
    "namespace lib\n\ntype Outcome =\n    Done\n        result : string\n    Failed\n        reason : string\n";
const ML_ENTRY: &str = "namespace app\n\nshow outcome = match outcome\n    lib::Done result => result\n    lib::Failed reason => reason\n\nprint (show (lib::Done(result = \"yes\")))\n";
const DEFAULT_LIBRARY: &str =
    "namespace lib;\n\ntype Outcome = Done { result: string } | Failed { reason: string }\n";
const DEFAULT_ENTRY: &str = "namespace app;\n\nfn show(outcome) = match outcome {\n    lib::Done { result } => result\n    lib::Failed { reason } => reason\n}\n\nfn main() = print(show(lib::Done { result: \"yes\" }))\n";

fn checks(flavor: Flavor, extension: &str, library: &str, entry: &str) {
    let entry_path = format!("src/main.{extension}");
    let sources = [
        parsed(&format!("src/lib.{extension}"), flavor, library),
        parsed(&entry_path, flavor, entry),
    ];
    let project = assemble(&config(&entry_path), &sources);
    assert!(project.is_ok(), "{project:?}");
    if let Ok(project) = project {
        let errors = osprey_types::check_program(&project.program);
        assert!(errors.is_empty(), "{errors:?}");
    }
}

#[test]
fn ml_matches_a_union_declared_in_another_namespace() {
    checks(Flavor::Ml, "ospml", ML_LIBRARY, ML_ENTRY);
}

#[test]
fn default_matches_a_union_declared_in_another_namespace() {
    checks(Flavor::Default, "osp", DEFAULT_LIBRARY, DEFAULT_ENTRY);
}
