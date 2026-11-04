//! Assertions for [DOC-EXPORT-REQUIRED]: an exported declaration without a
//! documentation comment is a warning in both flavors.

use crate::{undocumented_exports, UNDOCUMENTED_EXPORT};
use osprey_syntax::{parse_program_with_flavor, Flavor};

fn messages(flavor: Flavor, source: &str) -> Vec<String> {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(parsed.errors.is_empty(), "{source}\n{:?}", parsed.errors);
    undocumented_exports(&parsed.program)
        .into_iter()
        .inspect(|warning| {
            assert_eq!(warning.rule, UNDOCUMENTED_EXPORT);
            assert!(warning.position.is_some(), "{warning:?}");
        })
        .map(|warning| warning.message)
        .collect()
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
