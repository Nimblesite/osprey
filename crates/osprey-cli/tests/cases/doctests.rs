//! Executable documentation is checked through the public CLI.
//! Implements [DOC-DOCTEST-HARNESS].

use super::{run_file, temp_dir, temp_osp};

const DOCUMENTED: &str = r#"/// Echoes a value.
/// # Examples
/// ```osprey
/// print(echo("documented"))
/// ```
/// ```output
/// documented
/// ```
/// ```osprey
/// let checked = echo(42)
/// print("compile only")
/// ```
fn echo(value) = value
print("application entry must not run")
"#;

#[test]
fn doctests_run_in_declaration_context_and_check_compile_only_examples() {
    let source = temp_osp("documentation_context", DOCUMENTED);
    let result = run_file(&source, &["--doctests"]);
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 2 passed, 0 failed\n");
}

#[test]
fn doctests_reject_an_output_mismatch() {
    let text = DOCUMENTED.replace("/// documented\n", "/// incorrect\n");
    let result = run_file(&temp_osp("documentation_mismatch", &text), &["--doctests"]);
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert!(
        result.stderr.contains("stdout mismatch"),
        "{}",
        result.stderr
    );
    assert!(result.stderr.contains("echo"), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 1 passed, 1 failed\n");
}

#[test]
fn doctests_typecheck_examples_without_output_fences() {
    let text = DOCUMENTED.replace(
        "let checked = echo(42)",
        "let checked: int = echo(\"wrong\")",
    );
    let result = run_file(
        &temp_osp("documentation_type_error", &text),
        &["--doctests"],
    );
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert!(result.stderr.contains("cannot unify"), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 1 passed, 1 failed\n");
}

#[test]
fn doctests_inherit_ml_flavor() {
    let root = temp_dir("documentation_ml");
    let path = root.join("library.ospml");
    let source = "(** Echoes a value.\n# Examples\n```osprey\nprint (echo \"ML\")\n```\n```output\nML\n```\n*)\necho value = value\n";
    std::fs::write(&path, source).expect("write ML doc fixture");
    let result = run_file(&path, &["--doctests"]);
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 1 passed, 0 failed\n");
}

#[test]
fn doctests_keep_examples_isolated() {
    let text = DOCUMENTED
        .replace("let checked = echo(42)", "print(onlyInOtherExample)")
        .replace(
            "print(echo(\"documented\"))",
            "let onlyInOtherExample = 42\n/// print(echo(\"documented\"))",
        );
    let result = run_file(&temp_osp("documentation_isolation", &text), &["--doctests"]);
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert!(
        result.stderr.contains("onlyInOtherExample"),
        "{}",
        result.stderr
    );
    assert_eq!(result.stdout, "doctests: 1 passed, 1 failed\n");
}

#[test]
fn doctests_use_the_owning_nested_module_and_private_helpers() {
    let source = r#"namespace sample {
    module Math {
        fn secret() = "module"
        export module Inner {
            fn secret() = "nested"
            /// Calls the nearest private helper.
            /// # Examples
            /// ```osprey
            /// print(value())
            /// ```
            /// ```output
            /// nested
            /// ```
            export fn value() = secret()
        }
    }
}
fn main() = print("application main must not run")
"#;
    let result = run_file(&temp_osp("documentation_nested", source), &["--doctests"]);
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 1 passed, 0 failed\n");
}

#[test]
fn doctests_include_file_and_module_inner_documentation() {
    let source = r#"//! File documentation.
//! # Examples
//! ```osprey
//! print("file")
//! ```
//! ```output
//! file
//! ```
module Container {
    //! Module documentation.
    //! # Examples
    //! ```osprey
    //! print(label())
    //! ```
    //! ```output
    //! inner
    //! ```
    export fn label() = "inner"
}
"#;
    let result = run_file(&temp_osp("documentation_inner", source), &["--doctests"]);
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 2 passed, 0 failed\n");
}

#[test]
fn doctests_reject_invalid_source_even_without_examples() {
    let result = run_file(
        &temp_osp("documentation_invalid", "let bad: int = \"wrong\""),
        &["--doctests"],
    );
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert!(result.stderr.contains("cannot unify"), "{}", result.stderr);
}

#[test]
fn doctests_preserve_stdout_whitespace_exactly() {
    let text = DOCUMENTED.replace("/// documented\n", "/// documented \n");
    let result = run_file(&temp_osp("documentation_space", &text), &["--doctests"]);
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert!(
        result.stderr.contains("stdout mismatch"),
        "{}",
        result.stderr
    );
}

#[test]
fn doctests_work_in_mixed_flavor_projects() {
    let root = temp_dir("documentation_project");
    std::fs::write(
        root.join("osprey.toml"),
        "[project]\nname = \"doc_project\"\nentry = \"main.osp\"\n",
    )
    .expect("manifest");
    std::fs::write(
        root.join("main.osp"),
        "import support::Labels\nfn main() = print(Labels::label())\n",
    )
    .expect("entry");
    let source = "namespace support\n    module Labels\n        (** Label.\n# Examples\n```osprey\nprint (label ())\n```\n```output\nproject\n```\n*)\n        export label () = \"project\"\n";
    std::fs::write(root.join("labels.ospml"), source).expect("module");
    let result = run_file(&root, &["--doctests"]);
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 1 passed, 0 failed\n");
}

#[test]
fn doctests_do_not_activate_unrelated_test_helpers() {
    let source = format!("{DOCUMENTED}\nfn unusedTestHelper() = checkAll(\"unused\", [true])\n");
    let result = run_file(
        &temp_osp("documentation_unused_tests", &source),
        &["--doctests"],
    );
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 2 passed, 0 failed\n");
}

#[test]
fn doctests_typecheck_uncalled_declarations_inside_examples() {
    let source = DOCUMENTED.replace(
        "let checked = echo(42)",
        "fn unused() = missingFromExample()",
    );
    let result = run_file(
        &temp_osp("documentation_uncalled_error", &source),
        &["--doctests"],
    );
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert!(
        result.stderr.contains("missingFromExample"),
        "{}",
        result.stderr
    );
}

#[test]
fn doctests_stop_examples_that_exceed_the_execution_deadline() {
    let source = "/// Slow example.\n/// # Examples\n/// ```osprey\n/// sleep(1000)\n/// ```\n/// ```output\n/// ```\nfn documented() = 42\n";
    let path = temp_osp("documentation_timeout", source);
    let mut command = super::osprey();
    let _ = command
        .arg(path)
        .arg("--doctests")
        .env("OSPREY_DOCTEST_TIMEOUT_MS", "50");
    let result = super::finish(command);
    assert_eq!(result.code, Some(1), "{}", result.stderr);
    assert!(
        result.stderr.contains("execution timed out"),
        "{}",
        result.stderr
    );
    assert_eq!(result.stdout, "doctests: 0 passed, 1 failed\n");
}

#[test]
fn doctests_keep_example_type_positions_separate_from_source_positions() {
    let source = "let value = fn() => 42\n/// Separate lambda types.\n/// # Examples\n/// ```osprey\n/// let value = fn() => \"documentation\"\n/// print(value())\n/// ```\n/// ```output\n/// documentation\n/// ```\nfn documented() = value()\n";
    let result = run_file(
        &temp_osp("documentation_positions", source),
        &["--doctests"],
    );
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 1 passed, 0 failed\n");
}

#[test]
fn doctests_can_explicitly_call_the_documented_application_main() {
    let source = "/// Entry point.\n/// # Examples\n/// ```osprey\n/// main()\n/// ```\n/// ```output\n/// called explicitly\n/// ```\nfn main() = print(\"called explicitly\")\n";
    for (prefix, name) in [("", "main"), ("namespace app;\n", "namespace_main")] {
        let result = run_file(
            &temp_osp(
                &format!("documentation_{name}"),
                &format!("{prefix}{source}"),
            ),
            &["--doctests"],
        );
        assert_eq!(result.code, Some(0), "{}", result.stderr);
        assert_eq!(result.stdout, "doctests: 1 passed, 0 failed\n");
    }
}

#[test]
fn doctests_allow_example_helpers_to_capture_example_bindings() {
    let source = "/// Local helper.\n/// # Examples\n/// ```osprey\n/// let prefix = \"local\"\n/// fn helper() = prefix\n/// print(helper())\n/// ```\n/// ```output\n/// local\n/// ```\nfn documented() = 1\n";
    let result = run_file(
        &temp_osp("documentation_helper_capture", source),
        &["--doctests"],
    );
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 1 passed, 0 failed\n");
}

#[test]
fn doctests_allow_qualified_calls_to_documented_main() {
    let source = "namespace app;\n/// Entry.\n/// # Examples\n/// ```osprey\n/// app::main()\n/// ```\n/// ```output\n/// explicit\n/// ```\nfn main() = print(\"explicit\")\n";
    let result = run_file(
        &temp_osp("documentation_qualified_main", source),
        &["--doctests"],
    );
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout, "doctests: 1 passed, 0 failed\n");
}

#[test]
fn doctests_run_with_each_native_memory_backend() {
    let path = temp_osp("documentation_allocators", DOCUMENTED);
    for memory in ["default", "gc", "arc"] {
        let result = run_file(&path, &["--doctests", &format!("--memory={memory}")]);
        assert_eq!(result.code, Some(0), "{memory}: {}", result.stderr);
        assert_eq!(result.stdout, "doctests: 2 passed, 0 failed\n");
    }
}

/// The spec labels ML snippets ```` ```osprey-ml ````, so an ML author copies
/// that label. The harness used to recognise only ```` ```osprey ````, dropped
/// the example unseen and reported `0 passed, 0 failed` with exit 0 — a wrong
/// expectation (`5` for `double 2`) that was never executed passed the gate.
#[test]
fn doctests_run_examples_labelled_in_the_ml_spelling() {
    let root = temp_dir("documentation_ml_label");
    for label in ["osprey-ml", "ospml"] {
        let path = root.join(format!("{}.ospml", label.replace('-', "_")));
        let source = format!(
            "(** Doubles its argument.\n\n# Examples\n\n```{label}\nprint \"${{double 2}}\"\n```\n```output\n5\n```\n*)\ndouble x = x * 2 ?: 0\n"
        );
        std::fs::write(&path, source).expect("write ML-labelled doc fixture");
        let result = run_file(&path, &["--doctests"]);
        assert_eq!(
            result.code,
            Some(1),
            "a ```{label} example with the wrong output must fail: {} {}",
            result.stdout,
            result.stderr
        );
        assert_eq!(result.stdout, "doctests: 0 passed, 1 failed\n", "{label}");
    }
}

/// A blank line between the example and its output fence is ordinary Markdown.
/// It used to turn the example into compile-only without a word, so the
/// expected `9` for `add(1, 2)` was never compared and the gate said `1 passed`.
#[test]
fn doctests_compare_output_separated_from_its_example_by_blank_lines() {
    let source = "/// Adds.\n/// # Examples\n/// ```osprey\n/// print(\"${add(1, 2)}\")\n/// ```\n///\n/// ```output\n/// 9\n/// ```\nfn add(a, b) = a + b ?: 0\n";
    let result = run_file(
        &temp_osp("documentation_blank_before_output", source),
        &["--doctests"],
    );
    assert_eq!(result.code, Some(1), "{} {}", result.stdout, result.stderr);
    assert!(
        result.stderr.contains("stdout mismatch"),
        "{}",
        result.stderr
    );
    assert_eq!(result.stdout, "doctests: 0 passed, 1 failed\n");
}

/// An `output` fence that no example precedes checks nothing. Whatever the
/// author meant — a missing label, prose between the two — it must stop the
/// gate and name the declaration, never pass by saying nothing.
#[test]
fn doctests_reject_an_output_fence_with_no_example_before_it() {
    let source = "/// Adds.\n/// # Examples\n/// ```\n/// print(\"${add(1, 2)}\")\n/// ```\n/// ```output\n/// 3\n/// ```\nfn add(a, b) = a + b ?: 0\n";
    let result = run_file(
        &temp_osp("documentation_orphan_output", source),
        &["--doctests"],
    );
    assert_eq!(result.code, Some(1), "{} {}", result.stdout, result.stderr);
    assert!(
        result.stderr.contains("add"),
        "must name the declaration: {}",
        result.stderr
    );
    assert!(
        result.stderr.contains("`output` fence"),
        "must say what is wrong: {}",
        result.stderr
    );
    assert_eq!(result.stdout, "doctests: 0 passed, 1 failed\n");
}
