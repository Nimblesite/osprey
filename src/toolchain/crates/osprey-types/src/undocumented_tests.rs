//! Assertions for [DOC-EXPORT-REQUIRED] and [DOC-EXPORT-EXAMPLE-REQUIRED]: an
//! exported declaration without a documentation comment, or a callable one
//! whose comment shows no example, is a warning in both flavors.

use crate::{documentation_warnings, UNDOCUMENTED_EXPORT, UNEXAMPLED_EXPORT};
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// The messages `rule` raises for `source`, each one positioned.
fn raised(flavor: Flavor, source: &str, rule: &str) -> Vec<String> {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
    documentation_warnings(&parsed.program)
        .into_iter()
        .filter(|warning| warning.rule == rule)
        .inspect(|warning| assert!(warning.position.is_some(), "{warning:?}"))
        .map(|warning| warning.message)
        .collect()
}

fn messages(flavor: Flavor, source: &str) -> Vec<String> {
    raised(flavor, source, UNDOCUMENTED_EXPORT)
}

fn unexampled(flavor: Flavor, source: &str) -> Vec<String> {
    raised(flavor, source, UNEXAMPLED_EXPORT)
}

#[test]
fn every_callable_default_export_needs_an_example() {
    let source = "/// Shapes.\nmodule Shapes {\n    /// A shape.\n    export type Shape = Dot | Circle { r: int }\n    /// Logging.\n    export effect Log {\n        /// Writes a line.\n        write: fn(string) -> Unit\n        /// Flushes.\n        ///\n        /// # Examples\n        /// ```osprey\n        /// print(1)\n        /// ```\n        flush: fn() -> Unit\n    }\n    /// One.\n    export let unit = 1\n    /// Area of a shape.\n    export fn area(s) = 0\n    /// Twice.\n    ///\n    /// # Examples\n    /// ```osprey\n    /// print(twice(2))\n    /// ```\n    /// ```output\n    /// 4\n    /// ```\n    export fn twice(x) = x * 2\n    /// Prose only.\n    ///\n    /// # Examples\n    /// Call it with a number.\n    export fn prose(x) = x\n    /// Inner.\n    export module Inner {\n        /// One.\n        export fn one() = 1\n    }\n    /// Private.\n    fn hidden() = 0\n}\n";
    assert_eq!(
        unexampled(Flavor::Default, source),
        [
            "exported operation `Shapes::Log.write` has no example",
            "exported value `Shapes::unit` has no example",
            "exported function `Shapes::area` has no example",
            "exported function `Shapes::prose` has no example",
            "exported function `Shapes::Inner::one` has no example",
        ]
    );
}

#[test]
fn an_undocumented_export_is_reported_only_as_undocumented() {
    let source = "namespace demo\n\nmodule Api\n    export double x = x * 2\n";
    let parsed = parse_program_with_flavor(source, Flavor::Ml);
    let rules: Vec<&str> = documentation_warnings(&parsed.program)
        .iter()
        .map(|warning| warning.rule)
        .collect();
    assert_eq!(rules, [UNDOCUMENTED_EXPORT, UNDOCUMENTED_EXPORT]);
}

#[test]
fn an_ml_export_with_an_example_is_quiet() {
    let source = "(** The arithmetic API. *)\nmodule Api\n    (** Twice `x`.\n    # Examples\n    ```osprey\n    print \"${Api::double 2}\"\n    ```\n    *)\n    export double x = x * 2\n    (** Thrice `x`. *)\n    export triple x = x * 3\n";
    assert_eq!(
        unexampled(Flavor::Ml, source),
        ["exported function `Api::triple` has no example"]
    );
}

#[test]
fn an_undocumented_ml_export_is_named_with_its_module() {
    let source = "namespace demo\n\nmodule Api\n    export double x = x * 2\n";
    assert_eq!(
        messages(Flavor::Ml, source),
        [
            "module `Api` exports an API but has no documentation comment",
            "exported function `Api::double` has no documentation comment",
        ]
    );
}

#[test]
fn documented_exports_and_private_items_are_quiet() {
    let source = "(** The arithmetic API. *)\nmodule Api\n    (** Twice `x`. *)\n    export double x = x * 2\n    helper x = x\n";
    assert_eq!(messages(Flavor::Ml, source), Vec::<String>::new());
}

#[test]
fn an_inner_module_comment_documents_the_module() {
    let source = "module Api {\n    //! The arithmetic API.\n    /// Twice `x`.\n    export fn double(x) = x * 2\n}\n";
    assert_eq!(messages(Flavor::Default, source), Vec::<String>::new());
}

#[test]
fn every_kind_of_default_export_needs_documentation() {
    let source = "/// Shapes.\nmodule Shapes {\n    export type Shape = Dot | Circle { r: int }\n    export effect Log { write: fn(string) -> Unit }\n    export let unit = 1\n    /// Area of a shape.\n    export fn area(s) = 0\n    export module Inner {\n        /// One.\n        export fn one() = 1\n    }\n}\n";
    assert_eq!(
        messages(Flavor::Default, source),
        [
            "exported type `Shapes::Shape` has no documentation comment",
            "exported effect `Shapes::Log` has no documentation comment",
            "exported value `Shapes::unit` has no documentation comment",
            "exported module `Shapes::Inner` has no documentation comment",
        ]
    );
}

#[test]
fn a_module_without_exports_needs_no_documentation() {
    let source = "module Internal {\n    fn helper(x) = x\n}\n";
    assert_eq!(messages(Flavor::Default, source), Vec::<String>::new());
}
