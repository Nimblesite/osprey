//! Spec-driven project advice; warnings must never alter program acceptance.
mod support;

use osprey_project::{assemble, AssembledProject, ProjectConfig, SourceFile};
use osprey_syntax::Flavor;
use support::{config, parsed};

fn checked(config: &ProjectConfig, sources: &[SourceFile]) -> Result<AssembledProject, String> {
    let project = assemble(config, sources).map_err(|errors| format!("{errors:?}"))?;
    let errors = osprey_types::check_program(&project.program);
    assert!(errors.is_empty(), "{errors:?}");
    Ok(project)
}

#[test]
fn folder_advice_is_deterministic_and_does_not_impose_a_name_convention() -> Result<(), String> {
    // [MODULES-STYLE], [MODULES-PATH-INDEPENDENCE].
    let first = parsed(
        "src/main.osp",
        Flavor::Default,
        "namespace billing;\nfn main() = 0\n",
    );
    let second = parsed(
        "src/deep/helper.ospml",
        Flavor::Ml,
        "namespace billing\nhelper x = x\n",
    );
    let config = config("src/main.osp");
    let project = checked(&config, &[first.clone(), second.clone()])?;
    assert_eq!(project.warnings.len(), 2);
    assert!(project
        .warnings
        .iter()
        .all(|warning| warning.rule == "namespace-folder-drift"));
    let reversed = checked(&config, &[second, first.clone()])?;
    assert_eq!(project.warnings, reversed.warnings);
    assert_eq!(checked(&config, &[first])?.warnings, Vec::new());
    Ok(())
}

#[test]
fn application_advice_obeys_the_published_library_policy_in_both_flavors() -> Result<(), String> {
    // [MODULES-STYLE]: hierarchy depth counts modules, never label punctuation.
    for (flavor, source) in [
        (Flavor::Default, "namespace \"com.example.app\";\nmodule A { module B { module C { module D {} } } }\n"),
        (Flavor::Ml, "namespace \"com.example.app\"\nmodule A\n    module B\n        module C\n            module D\n                value = 0\n"),
    ] {
        let source = parsed("main.osp", flavor, source);
        let app = checked(&config("main.osp"), std::slice::from_ref(&source))?;
        assert_eq!(app.warnings.iter().map(|warning| warning.rule).collect::<Vec<_>>(), ["namespace-reverse-domain", "module-deep-hierarchy"]);
        let library = ProjectConfig { published_library: true, ..config("main.osp") };
        assert_eq!(checked(&library, &[source])?.warnings, Vec::new());
    }
    Ok(())
}

#[test]
fn ordinary_opaque_labels_do_not_acquire_domain_or_hierarchy_semantics() -> Result<(), String> {
    for label in [
        "billing/api/v1/resources",
        "app.team.feature",
        "org.example",
        "com..example",
        "example.com.library",
    ] {
        let source = parsed(
            "main.osp",
            Flavor::Default,
            &format!("namespace \"{label}\";\nfn main() = 0\n"),
        );
        assert!(
            checked(&config("main.osp"), &[source])?.warnings.is_empty(),
            "{label}"
        );
    }
    Ok(())
}

#[test]
fn state_inventory_counts_cells_but_never_exposes_their_representation() -> Result<(), String> {
    // [MODULES-STATE-INVENTORY]: reuse the real ownership validator's fixtures.
    let state = support::state_module(
        "Store",
        osprey_ast::Expr::Integer(42),
        Vec::new(),
        support::handler("CounterFx", osprey_ast::Expr::Identifier("count".into())),
    );
    let source = support::ast("main.osp", vec![state]);
    let project = checked(&config("main.osp"), &[source])?;
    let owner = project
        .state_boundaries
        .first()
        .ok_or("missing state owner")?;
    assert_eq!(project.state_boundaries.len(), 1);
    assert_eq!(owner.name, "app::Store");
    assert_eq!(owner.private_cells, 1);
    assert_eq!(owner.effects, ["app::Store::CounterFx"]);
    assert_eq!(
        project
            .warnings
            .first()
            .map(|warning| warning.message.as_str()),
        Some(owner.summary().as_str())
    );
    assert!(!owner.summary().contains("count"));
    assert!(!owner.summary().contains("42"));
    Ok(())
}

#[test]
fn private_and_empty_owners_are_listed_without_plain_modules() -> Result<(), String> {
    for (flavor, text) in [
        (Flavor::Default, "namespace z;\nmodule Outer { state module Hidden {} }\nnamespace a;\nstate module Empty {}\n"),
        (Flavor::Ml, "namespace z\nmodule Outer\n    state Hidden\n        value = 0\nnamespace a\nstate Empty\n    value = 0\n"),
    ] {
        let source = parsed("main.osp", flavor, text);
        let project = checked(&config("main.osp"), &[source])?;
        let names = project.state_boundaries.iter().map(|owner| owner.name.as_str()).collect::<Vec<_>>();
        assert_eq!(names, ["a::Empty", "z::Outer::Hidden"]);
        assert!(project.state_boundaries.iter().all(|owner| owner.private_cells == 0 && owner.effects.is_empty()));
    }
    Ok(())
}

#[test]
fn published_library_is_a_validated_boolean_not_a_warning_escape_hatch() -> Result<(), String> {
    let path = std::path::Path::new("app/osprey.toml");
    for (value, expected) in [("true", true), ("false", false)] {
        let text = format!("[modules]\npublished_library = {value}\n");
        let config = ProjectConfig::parse(&text, path).map_err(|errors| format!("{errors:?}"))?;
        assert_eq!(config.published_library, expected);
    }
    let errors = ProjectConfig::parse("[modules]\npublished_library = \"true\"", path)
        .err()
        .ok_or("invalid boolean accepted")?;
    assert_eq!(errors.len(), 1);
    assert_eq!(errors.first().and_then(|error| error.line), Some(2));
    assert!(errors
        .iter()
        .all(|error| error.message == "expected `true` or `false`"));
    Ok(())
}
