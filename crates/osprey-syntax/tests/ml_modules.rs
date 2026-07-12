//! ML module-surface coverage for [MODULES-NAMESPACE], [MODULES-MODULE],
//! [MODULES-SIGNATURE], [MODULES-EXPORTS], and [MODULES-IMPORT].
#![expect(
    clippy::indexing_slicing,
    reason = "test assertions: malformed AST indexing is a test failure"
)]

use osprey_ast::{
    Expr, ImportSelection, ModuleKind, NamespaceName, SignatureItem, SignatureType, Stmt,
    Visibility,
};
use osprey_syntax::{parse_program_with_flavor, Flavor, Parsed};

fn parse(src: &str) -> Parsed {
    parse_program_with_flavor(src, Flavor::Ml)
}

fn ok(src: &str) -> Vec<Stmt> {
    let parsed = parse(src);
    assert!(parsed.errors.is_empty(), "errors: {:?}", parsed.errors);
    parsed.program.statements
}

fn errors(src: &str) -> Vec<String> {
    let parsed = parse(src);
    assert!(!parsed.errors.is_empty(), "expected errors: {parsed:#?}");
    parsed
        .errors
        .into_iter()
        .map(|error| error.message)
        .collect()
}

#[test]
fn file_scoped_namespace_owns_following_declarations() {
    let statements = ok("namespace billing\nanswer = 42\n");
    match &statements[0] {
        Stmt::Namespace {
            name,
            body,
            file_scoped,
            ..
        } => {
            assert_eq!(name, &NamespaceName::Identifier("billing".to_owned()));
            assert!(*file_scoped);
            assert!(matches!(&body[0], Stmt::Let { name, .. } if name == "answer"));
        }
        other => panic!("expected namespace, got {other:?}"),
    }
}

#[test]
fn indented_namespace_is_a_block_contribution() {
    let statements = ok("namespace billing\n    inside = 1\noutside = 2\n");
    assert_eq!(statements.len(), 2);
    match &statements[0] {
        Stmt::Namespace {
            body, file_scoped, ..
        } => {
            assert!(!file_scoped);
            assert!(matches!(&body[0], Stmt::Let { name, .. } if name == "inside"));
        }
        other => panic!("expected namespace, got {other:?}"),
    }
}

#[test]
fn signature_and_ascribed_module_have_no_redundant_exports() {
    let src = "signature TaxApi\n    addTax : int -> int\n\nmodule Tax : TaxApi\n    rate = 10\n    addTax cents = cents + rate\n";
    let statements = ok(src);
    assert!(matches!(&statements[0], Stmt::Signature { name, .. } if name == "TaxApi"));
    match &statements[1] {
        Stmt::Module {
            kind,
            signature,
            body,
            ..
        } => {
            assert_eq!(*kind, ModuleKind::Plain);
            assert_eq!(
                signature
                    .as_ref()
                    .map(|sig| sig.path.to_string())
                    .as_deref(),
                Some("TaxApi")
            );
            assert!(body
                .iter()
                .all(|item| item.visibility == Visibility::Private));
        }
        other => panic!("expected module, got {other:?}"),
    }
}

#[test]
fn state_is_the_complete_ml_state_module_head() {
    let statements = ok("state Counter\n    mut count = 0\n");
    match &statements[0] {
        Stmt::Module { kind, body, .. } => {
            assert_eq!(*kind, ModuleKind::State);
            assert!(matches!(
                body[0].declaration.as_ref(),
                Stmt::Let { mutable: true, .. }
            ));
        }
        other => panic!("expected state module, got {other:?}"),
    }
}

#[test]
fn exported_annotation_propagates_to_one_bare_definition() {
    let statements = ok("module Tax\n    export addTax : int -> int\n    addTax cents = cents\n");
    match &statements[0] {
        Stmt::Module { body, .. } => {
            assert_eq!(body.len(), 1);
            assert_eq!(body[0].visibility, Visibility::Exported);
            assert!(
                matches!(body[0].declaration.as_ref(), Stmt::Function { name, .. } if name == "addTax")
            );
        }
        other => panic!("expected module, got {other:?}"),
    }
}

#[test]
fn unsigned_definition_can_be_exported_once() {
    let statements = ok("module Tax\n    export addTax cents = cents\n");
    match &statements[0] {
        Stmt::Module { body, .. } => assert_eq!(body[0].visibility, Visibility::Exported),
        other => panic!("expected module, got {other:?}"),
    }
}

#[test]
fn redundancy_and_mutable_exports_are_rejected() {
    let double =
        errors("module Tax\n    export addTax : int -> int\n    export addTax cents = cents\n");
    assert!(double
        .iter()
        .any(|error| error.contains("already exported")));

    let ascribed = errors("module Tax : Api\n    export x = 1\n");
    assert!(ascribed
        .iter()
        .any(|error| error.contains("exactly its signature")));

    let mutable = errors("state Store\n    export mut count = 0\n");
    assert!(mutable
        .iter()
        .any(|error| error.contains("cannot be exported")));
}

#[test]
fn orphan_and_redundant_signature_markers_are_rejected() {
    let orphan = errors("module Tax\n    f : int -> int\n    g x = x\n");
    assert!(orphan
        .iter()
        .any(|error| error.contains("immediately followed")));

    let exported = errors("signature Api\n    export f : int -> int\n");
    assert!(exported
        .iter()
        .any(|error| error.contains("public by definition")));

    let opaque = errors("signature Api\n    opaque type T\n");
    assert!(opaque.iter().any(|error| error.contains("redundant")));
}

#[test]
fn redundant_state_module_spelling_is_rejected() {
    let found = errors("state module Store\n    x = 1\n");
    assert!(found
        .iter()
        .any(|error| error.contains("without redundant 'module'")));
}

#[test]
fn signature_abstract_and_manifest_types_lower_distinctly() {
    let statements = ok("signature Ids\n    type UserId\n    type Raw = int\n");
    match &statements[0] {
        Stmt::Signature { items, .. } => {
            assert!(matches!(
                &items[0],
                SignatureItem::Type {
                    definition: SignatureType::Abstract,
                    opaque: true,
                    ..
                }
            ));
            assert!(matches!(
                &items[1],
                SignatureItem::Type {
                    definition: SignatureType::Manifest(_),
                    opaque: false,
                    ..
                }
            ));
        }
        other => panic!("expected signature, got {other:?}"),
    }
}

#[test]
fn opaque_export_retains_private_representation_metadata() {
    let statements = ok("module Ids\n    export opaque type UserId = int\n");
    match &statements[0] {
        Stmt::Module { body, .. } => {
            assert_eq!(body[0].visibility, Visibility::Exported);
            assert!(body[0].opaque);
            assert!(matches!(
                body[0].declaration.as_ref(),
                Stmt::Type { alias: Some(_), .. }
            ));
        }
        other => panic!("expected module, got {other:?}"),
    }
}

#[test]
fn whole_alias_member_and_wildcard_imports_lower() {
    let src = "import billing::Tax\nimport billing::Tax as T\nimport billing::Tax\n    addTax\n    zero as noTax\nimport billing::Tax\n    *\n";
    let statements = ok(src);
    assert_eq!(statements.len(), 4);
    assert!(
        matches!(&statements[0], Stmt::Import(import) if import.selection == ImportSelection::Whole)
    );
    assert!(matches!(&statements[1], Stmt::Import(import) if import.alias.as_deref() == Some("T")));
    assert!(
        matches!(&statements[2], Stmt::Import(import) if matches!(&import.selection, ImportSelection::Members(items) if items.len() == 2))
    );
    assert!(
        matches!(&statements[3], Stmt::Import(import) if import.selection == ImportSelection::Wildcard)
    );
}

#[test]
fn quoted_namespace_import_preserves_opaque_label() {
    let statements = ok("import \"billing/api\" as api\n");
    assert!(matches!(
        &statements[0],
        Stmt::Import(import)
            if import.target.namespace == NamespaceName::Quoted("billing/api".to_owned())
                && import.alias.as_deref() == Some("api")
    ));
}

#[test]
fn qualification_is_not_value_field_access_and_stays_curried() {
    let statements = ok("gross = Tax::addTax 100 2\nfield = invoice.total\n");
    match &statements[0] {
        Stmt::Let { value, .. } => match value {
            Expr::Call { function, .. } => {
                assert!(
                    matches!(function.as_ref(), Expr::Call { function, .. } if matches!(function.as_ref(), Expr::Path(_)))
                );
            }
            other => panic!("expected nested curried call, got {other:?}"),
        },
        other => panic!("expected let, got {other:?}"),
    }
    assert!(matches!(
        &statements[1],
        Stmt::Let {
            value: Expr::FieldAccess { .. },
            ..
        }
    ));
}

#[test]
fn parenthesised_cross_flavor_call_is_flat() {
    let statements = ok("gross = Tax::addTax (100, 2)\n");
    assert!(matches!(
        &statements[0],
        Stmt::Let {
            value: Expr::Call { arguments, .. },
            ..
        } if arguments.len() == 2
    ));
}
