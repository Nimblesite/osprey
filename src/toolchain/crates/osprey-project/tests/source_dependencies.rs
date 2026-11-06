//! Compiler-resolved source ownership, consumed by package usage analysis.
mod support;

use osprey_project::assemble;
use osprey_syntax::Flavor;
use support::{config, parsed};

fn wildcard_sources(imports: &str) -> [osprey_project::SourceFile; 3] {
    [
        parsed(
            "main.osp",
            Flavor::Default,
            &format!("namespace app;\n{imports}\nfn main() = answer()\n"),
        ),
        parsed(
            "one.osp",
            Flavor::Default,
            "namespace shared;\nmodule One { export fn answer() = 7\n fn hidden() = 0 }\n",
        ),
        parsed(
            "two.osp",
            Flavor::Default,
            "namespace shared;\nmodule Two { export fn answer() = 9 }\n",
        ),
    ]
}

fn wildcard_config() -> osprey_project::ProjectConfig {
    osprey_project::ProjectConfig {
        allow_wildcard_imports: true,
        ..config("main.osp")
    }
}

#[test]
fn allowed_wildcard_imports_track_only_the_selected_module() {
    let project = assemble(
        &wildcard_config(),
        &wildcard_sources("import shared::One::*"),
    )
    .expect("permitted wildcard import");
    let errors = osprey_types::check_program(&project.program);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(project
        .source_dependencies
        .iter()
        .any(|edge| edge.target.ends_with("one.osp")));
    assert!(project
        .source_dependencies
        .iter()
        .all(|edge| !edge.target.ends_with("two.osp")));
}

#[test]
fn wildcard_and_alias_member_collisions_never_choose_an_import() {
    for imports in [
        "import shared::One::*\nimport shared::Two::*",
        "import shared::One as answer\nimport shared::Two::{answer}",
    ] {
        let messages = support::error_messages(&wildcard_config(), &wildcard_sources(imports));
        assert!(
            support::contains(&messages, "ambiguous import binding"),
            "{messages:?}"
        );
        assert!(
            support::contains(&messages, "ambiguous imported name"),
            "{messages:?}"
        );
    }
}

#[test]
fn record_field_call_does_not_require_unused_ufcs_package() -> Result<(), String> {
    let sources = [
        parsed("src/main.osp", Flavor::Default,
            "namespace app;\ntype Action = { run: fn() -> int }\nfn main() = { let action = Action { run: fn() => 42 }\naction.run() }\n"),
        parsed("vendor/@unused/actions/fallback.osp", Flavor::Default,
            "namespace app;\nfn run(value) = 99\n"),
    ];
    let project =
        assemble(&config("src/main.osp"), &sources).map_err(|errors| format!("{errors:?}"))?;
    let without =
        assemble(&config("src/main.osp"), &sources[..1]).map_err(|errors| format!("{errors:?}"))?;
    for program in [&project.program, &without.program] {
        let errors = osprey_types::check_program(program);
        assert!(errors.is_empty(), "{errors:?}");
    }
    assert!(
        project
            .source_dependencies
            .iter()
            .all(|edge| edge.target != sources[1].path),
        "A record field wins over the unused UFCS fallback: {:?}",
        project.source_dependencies
    );
    Ok(())
}

#[test]
fn imported_module_remains_required_without_member_reads() -> Result<(), String> {
    let sources = [
        parsed(
            "src/main.osp",
            Flavor::Default,
            "namespace app;\nimport shared::Values as V\nfn main() = 42\n",
        ),
        parsed(
            "vendor/@one/shared/value.osp",
            Flavor::Default,
            "namespace shared;\nmodule Values { export let answer = 42 }\n",
        ),
    ];
    let project =
        assemble(&config("src/main.osp"), &sources).map_err(|errors| format!("{errors:?}"))?;
    assert!(
        project
            .source_dependencies
            .iter()
            .any(|edge| edge.symbol == "shared::Values" && edge.target == sources[1].path),
        "An import must still resolve even when no member is read: {:?}",
        project.source_dependencies
    );
    Ok(())
}

#[test]
fn imported_namespace_remains_required_without_member_reads() -> Result<(), String> {
    let sources = [
        parsed(
            "src/main.osp",
            Flavor::Default,
            "namespace app;\nimport shared as V\nfn main() = 42\n",
        ),
        parsed(
            "vendor/@one/shared/value.osp",
            Flavor::Default,
            "namespace shared;\nmodule Values { export let answer = 42 }\n",
        ),
    ];
    let project =
        assemble(&config("src/main.osp"), &sources).map_err(|errors| format!("{errors:?}"))?;
    assert!(
        project
            .source_dependencies
            .iter()
            .any(|edge| edge.target == sources[1].path),
        "A namespace import must resolve after pruning: {:?}",
        project.source_dependencies
    );
    Ok(())
}

const NAMESPACE_IMPORT_CASES: [(&str, &str); 4] = [
    (
        "src/main.osp",
        "namespace app;\nimport shared as V\nfn main() = 42\n",
    ),
    ("vendor/@one/shared/empty.osp", "namespace shared {}\n"),
    (
        "vendor/@two/shared/value.osp",
        "namespace shared;\nmodule Values { export let answer = 42 }\n",
    ),
    ("vendor/@other/unrelated.osp", "namespace unrelated {}\n"),
];

#[test]
fn namespace_imports_keep_all_contributors_including_empty_sources() -> Result<(), String> {
    let sources = NAMESPACE_IMPORT_CASES.map(|(path, text)| parsed(path, Flavor::Default, text));
    let project =
        assemble(&config("src/main.osp"), &sources).map_err(|errors| format!("{errors:?}"))?;
    let targets: std::collections::BTreeSet<_> = project
        .source_dependencies
        .iter()
        .map(|edge| &edge.target)
        .collect();
    assert_eq!(
        targets,
        [&sources[1].path, &sources[2].path].into_iter().collect()
    );
    Ok(())
}

#[test]
fn resolved_graph_keeps_type_and_inlined_constant_dependencies() -> Result<(), String> {
    let sources = [
        parsed("src/main.osp", Flavor::Default,
            "namespace app;\nimport shared::Values as V\nimport model::Types as T\nfn consume(value: T::Ticket) = value.id\nfn main() = V::answer\n"),
        parsed("vendor/@one/shared/value.osp", Flavor::Default,
            "namespace shared;\nmodule Values { export let answer = 42 }\n"),
        parsed("vendor/@one/model/types.osp", Flavor::Default,
            "namespace model;\nmodule Types { export type Ticket = { id: int } }\n"),
        parsed("vendor/@other/shared/value.osp", Flavor::Default,
            "namespace other;\nmodule Unused { export let answer = 99 }\n"),
    ];
    let project =
        assemble(&config("src/main.osp"), &sources).map_err(|errors| format!("{errors:?}"))?;
    let referenced: Vec<_> = project
        .source_dependencies
        .iter()
        .map(|edge| edge.symbol.as_str())
        .collect();
    assert!(
        referenced.contains(&"shared::Values::answer"),
        "{referenced:?}"
    );
    assert!(
        referenced.contains(&"model::Types::Ticket"),
        "{referenced:?}"
    );
    assert!(project
        .source_dependencies
        .iter()
        .all(|edge| !edge.target.to_string_lossy().contains("@other")));
    Ok(())
}
