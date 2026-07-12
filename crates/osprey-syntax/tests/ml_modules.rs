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

#[test]
fn signature_values_effects_and_nested_modules_lower_completely() {
    let statements = ok(concat!(
        "signature Service\n",
        "    version : int\n",
        "    choose : (int, string) -> bool\n",
        "    effect Store T\n",
        "        get : Unit => T\n",
        "    module Nested : Contracts::NestedApi\n",
    ));

    let Stmt::Signature { items, .. } = &statements[0] else {
        panic!("expected signature, got {:?}", statements[0]);
    };
    assert!(matches!(
        &items[0],
        SignatureItem::Value { name, ty, .. } if name == "version" && ty.name == "int"
    ));
    assert!(matches!(
        &items[1],
        SignatureItem::Function {
            name,
            parameters,
            return_type,
            ..
        } if name == "choose" && parameters.len() == 2 && return_type.name == "bool"
    ));
    assert!(matches!(
        &items[2],
        SignatureItem::Effect {
            name,
            type_params,
            operations,
            ..
        } if name == "Store" && type_params.len() == 1 && operations.len() == 1
    ));
    assert!(matches!(
        &items[3],
        SignatureItem::Module { path, signature, .. }
            if path.to_string() == "Nested"
                && signature.path.to_string() == "Contracts::NestedApi"
                && !signature.allow_extra
    ));
}

#[test]
fn module_declaration_docs_attach_to_every_supported_declaration() {
    let statements = ok(concat!(
        "module Documented\n",
        "    (** value docs *)\n",
        "    answer = 42\n",
        "    (** function docs *)\n",
        "    identity x = x\n",
        "    (** extern docs *)\n",
        "    extern log (message : string)\n",
        "    (** type docs *)\n",
        "    type Id = int\n",
        "    (** effect docs *)\n",
        "    effect Ping\n",
        "        ping : Unit => Unit\n",
        "    (** nested module docs *)\n",
        "    module Inner\n",
        "        value = 1\n",
        "    (** signature docs *)\n",
        "    signature InnerApi\n",
        "        value : int\n",
        "    (** import docs are deliberately discarded *)\n",
        "    import billing\n",
    ));

    let Stmt::Module { body, .. } = &statements[0] else {
        panic!("expected module, got {:?}", statements[0]);
    };
    assert_eq!(body.len(), 8);
    for declaration in &body[..7] {
        let has_doc = match declaration.declaration.as_ref() {
            Stmt::Let { doc, .. }
            | Stmt::Function { doc, .. }
            | Stmt::Extern { doc, .. }
            | Stmt::Type { doc, .. }
            | Stmt::Effect { doc, .. }
            | Stmt::Module { doc, .. }
            | Stmt::Signature { doc, .. } => doc.is_some(),
            other => panic!("expected a documentable declaration, got {other:?}"),
        };
        assert!(has_doc, "missing attached doc: {declaration:?}");
    }
    assert!(matches!(body[7].declaration.as_ref(), Stmt::Import(_)));
}

#[test]
fn malformed_module_heads_and_signature_items_recover_with_specific_errors() {
    let missing_module_body = errors("module Empty\nnext = 1\n");
    assert!(missing_module_body
        .iter()
        .any(|error| error.contains("module declaration requires an indented body")));

    let missing_signature_body = errors("signature Empty\nnext = 1\n");
    assert!(missing_signature_body
        .iter()
        .any(|error| error.contains("signature declaration requires an indented body")));

    let missing_value_colon = errors("signature Api\n    value int\n");
    assert!(missing_value_colon
        .iter()
        .any(|error| error.contains("expected ':' in signature value")));

    let missing_module_colon = errors("signature Api\n    module Child ChildApi\n");
    assert!(missing_module_colon
        .iter()
        .any(|error| error.contains("expected ':' in nested module signature item")));

    let unexpected_item = errors("signature Api\n    import billing\n");
    assert!(unexpected_item
        .iter()
        .any(|error| error.contains("unexpected token") && error.contains("signature")));
}

#[test]
fn malformed_import_projections_recover_with_specific_errors() {
    let selected_alias = errors("import billing as b\n    item\n");
    assert!(selected_alias
        .iter()
        .any(|error| error.contains("aliased whole import cannot also select members")));

    let wildcard_tail = errors("import billing\n    *\n    item\n");
    assert!(wildcard_tail
        .iter()
        .any(|error| error.contains("wildcard import '*' must be the only selected member")));

    let invalid_member = errors("import billing\n    42\n");
    assert!(
        invalid_member
            .iter()
            .any(|error| error.contains("expected an identifier")),
        "{invalid_member:?}"
    );

    let missing_segment = errors("import billing::\n");
    assert!(missing_segment
        .iter()
        .any(|error| error.contains("expected path segment after '::'")));
}

#[test]
fn invalid_export_opaque_and_namespace_forms_are_diagnosed() {
    let duplicate_export = errors("module M\n    export export x = 1\n");
    assert!(duplicate_export
        .iter()
        .any(|error| error.contains("duplicate 'export'")));

    let invalid_export = errors("module M\n    export import billing\n");
    assert!(invalid_export
        .iter()
        .any(|error| error.contains("'export' must modify")));

    let invalid_opaque = errors("module M\n    export opaque effect E\n");
    assert!(invalid_opaque
        .iter()
        .any(|error| error.contains("'opaque' may modify only a type declaration")));

    let unexported_opaque = errors("module M\n    opaque type Id = int\n");
    assert!(unexported_opaque
        .iter()
        .any(|error| error.contains("requires exactly one 'export opaque type'")));

    let namespace_export = errors("export x = 1\n");
    assert!(namespace_export
        .iter()
        .any(|error| error.contains("namespace declarations are public by default")));

    let invalid_namespace = errors("namespace 42\n");
    assert!(invalid_namespace
        .iter()
        .any(|error| error.contains("expected namespace name")));
}
