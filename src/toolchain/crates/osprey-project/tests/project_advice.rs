//! Spec-driven project advice; warnings must never alter program acceptance.
mod support;

use osprey_project::{assemble, AssembledProject, ProjectConfig, SourceFile};
use osprey_syntax::Flavor;
use support::{config, parsed};

#[test]
fn missing_or_ambiguous_entry_never_selects_an_arbitrary_source() {
    let sources =
        ["a/main.osp", "b/main.osp"].map(|name| parsed(name, Flavor::Default, "namespace app;\n"));
    for (entry, expected) in [
        (Some("missing.osp"), "was not found"),
        (Some("main.osp"), "is ambiguous"),
        (None, "cannot infer an entry source"),
    ] {
        let mut options = config("main.osp");
        options.entry = entry.map(Into::into);
        let errors = assemble(&options, &sources).expect_err(expected);
        assert!(
            errors.iter().any(|error| error.message.contains(expected)),
            "{errors:?}"
        );
    }
}

#[test]
fn each_supported_implicit_entry_route_is_accepted() -> Result<(), String> {
    let mut options = config("main.osp");
    options.entry = None;
    for text in ["fn main() = 0", "print(0)", "fn helper() = 0"] {
        let source = parsed("main.osp", Flavor::Default, text);
        let project = checked(&options, &[source])?;
        assert_eq!(project.entry_source, 0);
        assert_eq!(project.sources.len(), 1);
    }
    let errors = assemble(&options, &[]).expect_err("empty project rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("contains no Osprey source")),
        "{errors:?}"
    );
    Ok(())
}

#[test]
fn duplicate_implicit_entries_are_rejected_instead_of_ordered_by_path() {
    let mut options = config("main.osp");
    options.entry = None;
    for text in ["fn main() = 0", "print(0)"] {
        let sources = ["one", "two"].map(|name| {
            parsed(
                &format!("{name}.osp"),
                Flavor::Default,
                &format!("namespace {name};\n{text}"),
            )
        });
        let errors = assemble(&options, &sources).expect_err("ambiguous inferred entry");
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("is ambiguous")),
            "{errors:?}"
        );
    }
}

#[test]
fn project_text_parsing_preserves_flavor_errors_and_explicit_overrides() {
    let path = std::path::PathBuf::from("buffer.osp");
    for marker in ["ml", "unknown"] {
        let source = format!("// osprey: flavor={marker}\nfn main() = 0\n");
        let errors = osprey_project::parse_text(path.clone(), source.clone(), None)
            .expect_err("invalid or conflicting marker rejected");
        assert_ne!(errors, Vec::new());
        assert!(errors
            .iter()
            .all(|error| error.path.as_ref() == Some(&path) && error.line.is_none()));
        let overridden =
            osprey_project::parse_text(path.clone(), source.clone(), Some(Flavor::Default))
                .expect("explicit flavor wins over marker");
        assert_eq!(overridden.flavor, Flavor::Default);
        assert_eq!(overridden.source, source);
    }
}

#[test]
fn project_text_syntax_errors_keep_the_physical_buffer_location() {
    let path = std::path::PathBuf::from("unsaved.osp");
    let errors = osprey_project::parse_text(path.clone(), "fn main() = )\n".into(), None)
        .expect_err("invalid buffer rejected");
    assert_ne!(errors, Vec::new());
    assert!(errors
        .iter()
        .all(|error| error.path.as_ref() == Some(&path)));
    assert!(
        errors
            .iter()
            .any(|error| error.line == Some(1) && error.column.is_some()),
        "{errors:?}"
    );
}

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

const TAX_LIBRARY: &str = "namespace billing;\nmodule Tax {\n    export fn add(a, b) = wrapAdd(a, b)\n    export fn zero() = 0\n    export type Rate = Rate { percent: int }\n}\nmodule Fees {\n    export fn flat() = 5\n}\nmodule Idle {\n    export fn never() = 1\n}\n";

fn unused_imports(sources: &[SourceFile]) -> Result<Vec<String>, String> {
    let project = checked(&config("main.osp"), sources)?;
    Ok(project
        .warnings
        .iter()
        .filter(|warning| warning.rule == "unused-import")
        .map(|warning| warning.message.clone())
        .collect())
}

#[test]
fn an_import_nothing_resolves_through_is_reported_and_a_used_one_is_not() -> Result<(), String> {
    // [MODULES-STYLE]: a qualified path, a bare member, an alias in a type
    // position and a method-style call each use the import they resolve through.
    let library = parsed("lib.osp", Flavor::Default, TAX_LIBRARY);
    let used = "import billing::Tax\nimport billing::Fees::{flat}\nimport billing::Tax as T\nimport billing::Tax::{zero as none}\n";
    let body = "fn percent(rate: T::Rate) = rate.percent\nfn main() = Tax::add(flat(), none())\n";
    let idle = "import billing::Idle\nimport billing::Idle::{never as nope}\n";
    let clean = parsed(
        "main.osp",
        Flavor::Default,
        &format!("namespace app;\n{used}{body}"),
    );
    assert_eq!(
        unused_imports(&[library.clone(), clean])?,
        Vec::<String>::new()
    );
    let noisy = parsed(
        "main.osp",
        Flavor::Default,
        &format!("namespace app;\n{used}{idle}{body}"),
    );
    assert_eq!(
        unused_imports(&[library.clone(), noisy.clone()])?,
        [
            "import `billing::Idle` is never used",
            "import `billing::Idle::{never as nope}` is never used"
        ]
    );
    assert_eq!(unused_imports(&[noisy, library.clone()])?.len(), 2);
    // A module of the importing namespace resolves without its import.
    let sibling = parsed(
        "main.osp",
        Flavor::Default,
        "namespace billing;\nimport billing::Tax\nfn main() = Tax::zero()\n",
    );
    assert_eq!(
        unused_imports(&[library.clone(), sibling])?,
        ["import `billing::Tax` is never used"]
    );
    let ml = parsed(
        "main.ospml",
        Flavor::Ml,
        "namespace app\nimport billing::Tax\nimport billing::Idle\nmain () = Tax::zero ()\n",
    );
    let mut options = config("main.ospml");
    options.entry = Some("main.ospml".into());
    let project = checked(&options, &[library, ml])?;
    let rules: Vec<_> = project
        .warnings
        .iter()
        .map(|w| (w.rule, w.message.as_str()))
        .collect();
    assert_eq!(
        rules,
        [("unused-import", "import `billing::Idle` is never used")]
    );
    Ok(())
}
