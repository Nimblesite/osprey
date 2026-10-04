//! Project ownership metadata; no private cell names, values or helper APIs.
//! Implements [MODULES-STATE-INVENTORY], [DOC-STATE-BOUNDARIES].

use super::model::Page;
use crate::project::CompilationInput;

pub(super) fn page(input: &CompilationInput) -> Option<Page> {
    let boundaries = input.state_boundaries();
    if boundaries.is_empty() {
        return None;
    }
    let listing = boundaries
        .iter()
        .map(|owner| format!("- {}", owner.summary()))
        .collect::<Vec<_>>()
        .join("\n");
    Some(Page {
        slug: "api/project/state-boundaries".into(), title: "State boundaries".into(),
        group: "Overview".into(),
        summary: "Every state owner in this project, including private module boundaries.".into(),
        signature: String::new(),
        markdown: format!("# State boundaries\n\nEvery state owner is listed, including private modules. Cell names and initializers remain private. Each handler installation creates fresh cells; imports never create a shared instance.\n\n{listing}\n"),
    })
}
