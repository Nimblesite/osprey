//! A library project has nothing to run, so it needs no entry source.
mod support;

use osprey_project::assemble;
use osprey_syntax::Flavor;
use support::{config, parsed};

/// `packages/native` is nine `.ospml` sources with helpers only: no `main`,
/// no executable statements and no configured entry. `osprey --check` and the
/// language server both rejected it with "cannot infer an entry source",
/// so every library package showed a project error on every one of its files.
#[test]
fn a_multi_source_library_without_main_assembles_without_an_entry() {
    let sources = [
        ("src/base.ospml", "namespace ui\nwidth x = x\n"),
        ("src/theme.ospml", "namespace ui\naccent x = x\n"),
    ]
    .map(|(name, text)| parsed(name, Flavor::Ml, text));
    let mut options = config("src/main.ospml");
    options.entry = None;
    let project = assemble(&options, &sources)
        .unwrap_or_else(|errors| panic!("a library must assemble without an entry: {errors:?}"));
    assert_eq!(project.sources.len(), 2);
    assert!(
        project.entry_prologue.is_empty(),
        "a library runs nothing at start-up: {:?}",
        project.entry_prologue
    );
}
