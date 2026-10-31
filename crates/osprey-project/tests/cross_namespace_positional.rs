//! A positional constructor declared in one namespace is applied in another
//! through its qualified name [MODULES-RESOLUTION]. One field resolves; two
//! fields must too (found by apps/workbench, which works around it).

mod support;

use osprey_project::assemble;
use osprey_syntax::Flavor;
use support::{config, parsed};

const ML_LIBRARY: &str = "namespace shapes\n\n(** A formula with a one-field and a two-field constructor. *)\ntype F = V string | Both F F\n";
const ML_ENTRY: &str = "namespace app\n\none = shapes::V \"p\"\n\ntwo = shapes::Both (shapes::V \"p\") (shapes::V \"q\")\n\nprint \"built\"\n";

#[test]
fn ml_applies_a_two_field_constructor_from_another_namespace() {
    let entry_path = "src/main.ospml";
    let sources = [
        parsed("src/shapes.ospml", Flavor::Ml, ML_LIBRARY),
        parsed(entry_path, Flavor::Ml, ML_ENTRY),
    ];
    let project = assemble(&config(entry_path), &sources);
    assert!(project.is_ok(), "{project:?}");
    if let Ok(project) = project {
        let errors = osprey_types::check_program(&project.program);
        assert!(errors.is_empty(), "{errors:?}");
    }
}
