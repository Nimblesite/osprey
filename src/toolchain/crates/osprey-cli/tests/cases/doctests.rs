//! Executable documentation is checked through the public CLI.
//! Implements [DOC-DOCTEST-HARNESS].

use super::{run_file, run_source, temp_dir, temp_osp};

/// The `--doctests` run exited 0 and reported exactly `tally`.
fn assert_doc_tests(result: &super::Out, tally: &str) {
    assert_eq!(result.code, Some(0), "{}", result.stderr);
    assert_eq!(result.stdout, format!("doctests: {tally}\n"));
}

/// The `--doctests` run was rejected, naming every string in `expected` on
/// stderr, and reported exactly `tally` — a rejected run still counts the
/// examples that passed.
fn assert_doc_tests_rejected(result: &super::Out, expected: &[&str], tally: &str) {
    result.expect_stderr(1, expected, &[]);
    assert_eq!(result.stdout, format!("doctests: {tally}\n"));
}

/// The `--doctests` run of the temp fixture `name` holding `source` was
/// rejected, naming every string in `expected` on stderr.
fn assert_rejected(name: &str, source: &str, expected: &[&str]) {
    doc_tests(name, source).expect_stderr(1, expected, &[]);
}

/// The `--doctests` run of the temp fixture `name` holding `source`.
fn doc_tests(name: &str, source: &str) -> super::Out {
    run_source(name, source, &["--doctests"])
}

/// `DOCUMENTED` with every `from` replaced by `to`.
fn documented_with(from: &str, to: &str) -> String {
    DOCUMENTED.replace(from, to)
}

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
    let result = doc_tests("documentation_context", DOCUMENTED);
    assert_doc_tests(&result, "2 passed, 0 failed");
}

#[test]
fn doctests_reject_an_output_mismatch() {
    let text = documented_with("/// documented\n", "/// incorrect\n");
    let result = doc_tests("documentation_mismatch", &text);
    assert_doc_tests_rejected(&result, &["stdout mismatch", "echo"], "1 passed, 1 failed");
}

#[test]
fn doctests_typecheck_examples_without_output_fences() {
    let text = documented_with(
        "let checked = echo(42)",
        "let checked: int = echo(\"wrong\")",
    );
    let result = doc_tests("documentation_type_error", &text);
    assert_doc_tests_rejected(&result, &["cannot unify"], "1 passed, 1 failed");
}

#[test]
fn doctests_inherit_ml_flavor() {
    let root = temp_dir("documentation_ml");
    let path = root.join("library.ospml");
    let source = "(** Echoes a value.\n# Examples\n```osprey\nprint (echo \"ML\")\n```\n```output\nML\n```\n*)\necho value = value\n";
    std::fs::write(&path, source).expect("write ML doc fixture");
    let result = run_file(&path, &["--doctests"]);
    assert_doc_tests(&result, "1 passed, 0 failed");
}

#[test]
fn doctests_keep_examples_isolated() {
    let text = documented_with("let checked = echo(42)", "print(onlyInOtherExample)").replace(
        "print(echo(\"documented\"))",
        "let onlyInOtherExample = 42\n/// print(echo(\"documented\"))",
    );
    let result = doc_tests("documentation_isolation", &text);
    assert_doc_tests_rejected(&result, &["onlyInOtherExample"], "1 passed, 1 failed");
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
    let result = doc_tests("documentation_nested", source);
    assert_doc_tests(&result, "1 passed, 0 failed");
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
    let result = doc_tests("documentation_inner", source);
    assert_doc_tests(&result, "2 passed, 0 failed");
}

#[test]
fn doctests_reject_invalid_source_even_without_examples() {
    assert_rejected(
        "documentation_invalid",
        "let bad: int = \"wrong\"",
        &["cannot unify"],
    );
}

#[test]
fn doctests_preserve_stdout_whitespace_exactly() {
    let text = documented_with("/// documented\n", "/// documented \n");
    assert_rejected("documentation_space", &text, &["stdout mismatch"]);
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
    assert_doc_tests(&result, "1 passed, 0 failed");
}

/// A package names its own namespace as `default_namespace`, so the harness's
/// top-level wrapper is placed in the very namespace that already holds the
/// example's inner wrapper: each nesting level needs its own runner name.
const PACKAGE_SOURCES: [(&str, &str); 4] = [
    (
        "osprey.toml",
        "[project]\nname = \"labels\"\nsource_roots = [\"src\"]\npython_sources = [\"src/numbers.py\"]\ndefault_namespace = \"ui\"\n",
    ),
    (
        "src/words.osp",
        "namespace ui {\n    /// Words.\n    module Words {\n        /// Shouts.\n        /// # Examples\n        /// ```osprey\n        /// print(Words::shout(\"hi\"))\n        /// ```\n        /// ```output\n        /// hi!\n        /// ```\n        export fn shout(word) = \"${word}!\"\n    }\n    /// Greets.\n    /// # Examples\n    /// ```osprey\n    /// print(greet(\"Ada\"))\n    /// ```\n    /// ```output\n    /// hello Ada\n    /// ```\n    fn greet(name) = \"hello ${name}\"\n}\n",
    ),
    (
        "src/labels.ospml",
        "namespace ui\n\n(** Labels. *)\nmodule Labels\n    (** Labels a count.\n    # Examples\n    ```osprey\n    print (Labels::label 3)\n    ```\n    ```output\n    count 3\n    ```\n    *)\n    export label n = \"count ${n}\"\n",
    ),
    (
        "src/numbers.py",
        "# osprey: flavor=python\n# osprey: namespace=ui\n# osprey: module=Numbers\n\"\"\"Numbers.\"\"\"\n\n__all__ = [\"twice\"]\n\n\ndef twice(n):\n    \"\"\"Brackets a count.\n\n    # Examples\n    ```osprey\n    print(twice(4))\n    ```\n    ```output\n    <4>\n    ```\n    \"\"\"\n    return f\"<{n}>\"\n",
    ),
];

#[test]
fn doctests_run_in_packages_whose_default_namespace_is_their_own_in_every_flavor() {
    let root = temp_dir("documentation_package");
    for (name, text) in PACKAGE_SOURCES {
        let path = root.join(name);
        let made = path.parent().map(std::fs::create_dir_all);
        assert!(matches!(made, Some(Ok(()))), "{made:?}");
        assert!(std::fs::write(&path, text).is_ok(), "{}", path.display());
    }
    let result = run_file(&root, &["--doctests"]);
    assert_eq!(result.code, Some(0), "{}{}", result.stdout, result.stderr);
    assert_eq!(
        result.stdout, "doctests: 4 passed, 0 failed\n",
        "{}",
        result.stderr
    );
    assert!(
        !result.stderr.contains("duplicate declaration"),
        "{}",
        result.stderr
    );
}

#[test]
fn doctests_do_not_activate_unrelated_test_helpers() {
    let source = format!("{DOCUMENTED}\nfn unusedTestHelper() = checkAll(\"unused\", [true])\n");
    let result = doc_tests("documentation_unused_tests", &source);
    assert_doc_tests(&result, "2 passed, 0 failed");
}

#[test]
fn doctests_typecheck_uncalled_declarations_inside_examples() {
    let source = documented_with(
        "let checked = echo(42)",
        "fn unused() = missingFromExample()",
    );
    assert_rejected(
        "documentation_uncalled_error",
        &source,
        &["missingFromExample"],
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
    let result = super::finish(&mut command);
    assert_doc_tests_rejected(&result, &["execution timed out"], "0 passed, 1 failed");
}

#[test]
fn doctests_keep_example_type_positions_separate_from_source_positions() {
    let source = "let value = fn() => 42\n/// Separate lambda types.\n/// # Examples\n/// ```osprey\n/// let value = fn() => \"documentation\"\n/// print(value())\n/// ```\n/// ```output\n/// documentation\n/// ```\nfn documented() = value()\n";
    let result = doc_tests("documentation_positions", source);
    assert_doc_tests(&result, "1 passed, 0 failed");
}

#[test]
fn doctests_can_explicitly_call_the_documented_application_main() {
    let source = "/// Entry point.\n/// # Examples\n/// ```osprey\n/// main()\n/// ```\n/// ```output\n/// called explicitly\n/// ```\nfn main() = print(\"called explicitly\")\n";
    for (prefix, name) in [("", "main"), ("namespace app;\n", "namespace_main")] {
        let result = doc_tests(
            &format!("documentation_{name}"),
            &format!("{prefix}{source}"),
        );
        assert_doc_tests(&result, "1 passed, 0 failed");
    }
}

#[test]
fn doctests_allow_example_helpers_to_capture_example_bindings() {
    let source = "/// Local helper.\n/// # Examples\n/// ```osprey\n/// let prefix = \"local\"\n/// fn helper() = prefix\n/// print(helper())\n/// ```\n/// ```output\n/// local\n/// ```\nfn documented() = 1\n";
    let result = doc_tests("documentation_helper_capture", source);
    assert_doc_tests(&result, "1 passed, 0 failed");
}

#[test]
fn doctests_allow_qualified_calls_to_documented_main() {
    let source = "namespace app;\n/// Entry.\n/// # Examples\n/// ```osprey\n/// app::main()\n/// ```\n/// ```output\n/// explicit\n/// ```\nfn main() = print(\"explicit\")\n";
    let result = doc_tests("documentation_qualified_main", source);
    assert_doc_tests(&result, "1 passed, 0 failed");
}

#[test]
fn doctests_run_with_each_native_memory_backend() {
    let path = temp_osp("documentation_allocators", DOCUMENTED);
    for memory in ["gc", "arc"] {
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
            "(** Doubles its argument.\n\n# Examples\n\n```{label}\nprint \"${{double 2}}\"\n```\n```output\n5\n```\n*)\ndouble x = wrapMul x 2\n"
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
    let source = "/// Adds.\n/// # Examples\n/// ```osprey\n/// print(\"${add(1, 2)}\")\n/// ```\n///\n/// ```output\n/// 9\n/// ```\nfn add(a, b) = wrapAdd(a, b)\n";
    let result = doc_tests("documentation_blank_before_output", source);
    assert_doc_tests_rejected(&result, &["stdout mismatch"], "0 passed, 1 failed");
}

/// An `output` fence that no example precedes checks nothing. Whatever the
/// author meant — a missing label, prose between the two — it must stop the
/// gate and name the declaration, never pass by saying nothing.
#[test]
fn doctests_reject_an_output_fence_with_no_example_before_it() {
    let source = "/// Adds.\n/// # Examples\n/// ```\n/// print(\"${add(1, 2)}\")\n/// ```\n/// ```output\n/// 3\n/// ```\nfn add(a, b) = wrapAdd(a, b)\n";
    let result = doc_tests("documentation_orphan_output", source);
    assert_doc_tests_rejected(&result, &["add", "`output` fence"], "0 passed, 1 failed");
}
