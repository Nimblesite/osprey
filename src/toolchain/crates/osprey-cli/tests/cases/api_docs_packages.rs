//! Documentation export for packages that import workspace dependencies, in
//! every flavor and both output formats. Implements [DOC-EXPORT],
//! [DOC-EXPORT-HTML] and [PACKAGE-WORKSPACE].

use super::{finish, osprey, read_text, repo_root, temp_dir};
use std::path::Path;

/// A dependency package contributing one module per flavor to namespace `ui`.
const WIDGETS: [(&str, &str); 4] = [
    (
        "osprey.toml",
        "[project]\nname = \"widgets\"\nsource_roots = [\"src\"]\npython_sources = [\"src/lists.py\"]\n\n[package]\nname = \"@acme/widgets\"\nlicense = \"MIT\"\nsummary = \"Widgets\"\n",
    ),
    (
        "src/runtime.osp",
        "namespace ui {\n    /// Steps a counter.\n    module Runtime {\n        /// The next count.\n        export fn step(n) = n + 1\n    }\n}\n",
    ),
    (
        "src/values.ospml",
        "namespace ui\n    (** Text values. *)\n    module Values\n        (** Labels a count. *)\n        export text n = \"count ${toString (n * 1)}\"\n",
    ),
    (
        "src/lists.py",
        "# osprey: flavor=python\n# osprey: namespace=ui\n# osprey: module=Lists\n\"\"\"Counting lists.\"\"\"\n\n__all__ = [\"twice\"]\n\n\ndef twice(n):\n    \"\"\"Doubles a count.\"\"\"\n    return n * 2\n",
    ),
];

/// One application package per flavor, each importing all three `ui` modules.
/// Fields: flavor, source file, Python selection line, source, rendered signature.
const SHOPS: [(&str, &str, &str, &str, &str); 3] = [
    (
        "default",
        "cart.osp",
        "",
        "namespace shop;\nimport ui::Runtime\nimport ui::Values\nimport ui::Lists\n\n/// The basket.\nmodule Cart {\n    /// Labels the next doubled count.\n    export fn next(n) = Values::text(Lists::twice(Runtime::step(n)))\n}\n",
        "fn shop::Cart::next(n: int) -> string",
    ),
    (
        "ml",
        "cart.ospml",
        "",
        "namespace shop\n\nimport ui::Runtime\nimport ui::Values\nimport ui::Lists\n\n(** The basket. *)\nmodule Cart\n    (** Labels the next doubled count. *)\n    export next n = Values::text (Lists::twice (Runtime::step n))\n",
        "shop::Cart::next : int -> string",
    ),
    (
        "python",
        "cart.py",
        "python_sources = [\"src/cart.py\"]\n",
        "# osprey: flavor=python\n# osprey: namespace=shop\n# osprey: module=Cart\n\"\"\"The basket.\"\"\"\nfrom ui import Runtime, Values, Lists\n\n__all__ = [\"next\"]\n\n\ndef next(n):\n    \"\"\"Labels the next doubled count.\"\"\"\n    return Values.text(Lists.twice(Runtime.step(n)))\n",
        "def shop::Cart::next(n: int) -> str",
    ),
];

/// Pages every shop export must publish: its own API and each imported module.
const PAGES: [(&str, &str); 7] = [
    ("shop-cart-next", "Labels the next doubled count."),
    ("shop-cart", "The basket."),
    ("ui-runtime-step", "The next count."),
    ("ui-runtime", "Steps a counter."),
    ("ui-values-text", "Labels a count."),
    ("ui-values", "Text values."),
    ("ui-lists-twice", "Doubles a count."),
];

/// Diagnostics a resolved, fully documented workspace must never print.
const NEVER: [&str; 4] = [
    "unknown import target",
    "unknown symbol path",
    "undocumented-export",
    "error",
];

fn write_all(root: &Path, files: &[(&str, &str)]) {
    for (name, text) in files {
        let path = root.join(name);
        let made = path.parent().map(std::fs::create_dir_all);
        assert!(matches!(made, Some(Ok(()))), "{}: {made:?}", path.display());
        let written = std::fs::write(&path, text);
        assert!(written.is_ok(), "{}: {written:?}", path.display());
    }
}

fn shop_manifest(flavor: &str, python: &str) -> String {
    format!("[project]\nname = \"shop_{flavor}\"\nsource_roots = [\"src\"]\n{python}\n[package]\nname = \"@acme/shop-{flavor}\"\nlicense = \"MIT\"\nsummary = \"Shop\"\n\n[dependencies]\n\"@acme/widgets\" = {{ epoch = 1, min_release = 1 }}\n")
}

fn export(project: &Path, output: &Path, format: &str) -> super::Out {
    let mut command = osprey();
    let _ = command
        .arg("--docs")
        .arg(project)
        .arg("--docs-dir")
        .arg(output)
        .args(["--docs-format", format]);
    finish(&mut command)
}

fn assert_clean(label: &str, result: &super::Out) {
    assert_eq!(result.code, Some(0), "{label}: {}", result.stderr);
    assert!(
        result.stdout.contains("documentation pages"),
        "{label}: {}",
        result.stdout
    );
    for word in NEVER {
        assert!(!result.stderr.contains(word), "{label}: {}", result.stderr);
    }
}

#[test]
fn package_docs_resolve_workspace_dependency_imports_in_every_flavor_and_format() {
    let root = temp_dir("docs_workspace_packages");
    write_all(&root.join("widgets"), &WIDGETS);
    for (flavor, file, python, source, signature) in SHOPS {
        let project = root.join(format!("shop_{flavor}"));
        let manifest = shop_manifest(flavor, python);
        write_all(
            &project,
            &[("osprey.toml", &manifest), (&format!("src/{file}"), source)],
        );
        for (format, extension) in [("markdown", "md"), ("html", "html")] {
            let label = format!("{flavor} {format}");
            let output = root.join(format!("out_{flavor}_{format}"));
            assert_clean(&label, &export(&project, &output, format));
            let api = output.join("api");
            for (page, summary) in PAGES {
                let text = read_text(&api.join(format!("{page}.{extension}")));
                assert!(text.contains(summary), "{label} {page}: {text}");
            }
            let own = read_text(&api.join(format!("shop-cart-next.{extension}")));
            let own = own.replace("-&gt;", "->");
            assert!(own.contains(signature), "{label}: {own}");
            assert!(own.contains(&format!("src/{file}")), "{label}: {own}");
            let index = read_text(&api.join(format!("index.{extension}")));
            for module in ["shop::Cart", "ui::Runtime", "ui::Values", "ui::Lists"] {
                assert!(index.contains(module), "{label} index lacks {module}");
            }
            for (_, summary) in PAGES {
                assert!(index.contains(summary), "{label} index lacks {summary}");
            }
        }
    }
}

/// The repository's own package that imports `@osprey/native`'s `ui` modules
/// across packages: the user-reported `unknown import target ui::Runtime`.
#[test]
fn package_docs_export_the_signal_flow_package_with_its_ui_dependency() {
    let project = repo_root().join("src/osprey/packages/signal_flow");
    let root = temp_dir("docs_signal_flow");
    for (format, extension) in [("markdown", "md"), ("html", "html")] {
        let output = root.join(format);
        let result = export(&project, &output, format);
        assert_eq!(result.code, Some(0), "{format}: {}", result.stderr);
        for word in ["unknown import target", "unknown symbol path"] {
            assert!(!result.stderr.contains(word), "{format}: {}", result.stderr);
        }
        let api = output.join("api");
        for page in ["signalflow", "ui-runtime", "ui-machinedocs", "ui-values"] {
            let path = api.join(format!("{page}.{extension}"));
            assert!(path.is_file(), "{format}: missing {}", path.display());
        }
    }
}
