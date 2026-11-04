//! Opaque signatures must hide their representation in the flattened public API.

mod support;

use osprey_ast::{Stmt, TypeExpr};
use osprey_project::assemble;
use osprey_syntax::Flavor;
use support::{config, parsed};

#[test]
fn abstract_type_does_not_collapse_in_exported_function_contracts() {
    // Implements [MODULES-OPAQUE-TYPES], [MODULES-SIGNATURE].
    let source = parsed(
        "main.osp",
        Flavor::Default,
        concat!(
            "signature IdApi {\n",
            "    opaque type Id\n",
            "    fn keep(value: Id) -> Id\n",
            "}\n",
            "module Ids : IdApi {\n",
            "    type Id = int\n",
            "    fn keep(value) = value\n",
            "}\n",
            "fn main() = 0\n",
        ),
    );
    let assembled = assemble(&config("main.osp"), &[source]);
    let project = match assembled {
        Ok(project) => project,
        Err(errors) => {
            assert!(
                errors.iter().any(|error| {
                    error.message.contains("opaque") && error.message.contains("unsupported")
                }),
                "unexpected assembly errors: {errors:?}"
            );
            return;
        }
    };
    let keep = project
        .source_name_by_mangled
        .iter()
        .find_map(|(mangled, source)| (source == "app::Ids::keep").then_some(mangled));
    let contract = project
        .program
        .statements
        .iter()
        .find_map(|statement| match statement {
            Stmt::Function {
                name,
                parameters,
                return_type,
                ..
            } if keep.is_some_and(|keep| keep == name) => Some((
                parameters
                    .first()
                    .and_then(|parameter| parameter.ty.as_ref()),
                return_type.as_ref(),
            )),
            _ => None,
        });
    assert!(
        contract.is_some(),
        "missing exported keep contract: {:#?}",
        project.program
    );
    if let Some((Some(parameter), Some(result))) = contract {
        assert_nominal(parameter);
        assert_nominal(result);
        assert_eq!(parameter.name, result.name);
    }
}

fn assert_nominal(ty: &TypeExpr) {
    assert_ne!(
        ty.name, "int",
        "opaque representation leaked into public API"
    );
}
