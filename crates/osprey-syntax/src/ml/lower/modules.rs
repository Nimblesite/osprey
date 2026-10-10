//! Lower module identities and public signatures.
use super::{
    unquote, ImportDecl, ImportMember, ImportSelection, ImportTarget, MlImport, MlImportSelection,
    MlItem, MlNamespaceName, MlSignatureItem, MlSymbolPath, MlType, ModuleItem, NamespaceName,
    Position, SignatureItem, SymbolPath, TypeExpr,
};

pub(super) fn lower_namespace_name(name: MlNamespaceName) -> NamespaceName {
    match name {
        MlNamespaceName::Ident(label) => NamespaceName::Identifier(label),
        MlNamespaceName::Quoted(label) => NamespaceName::Quoted(unquote(&label)),
    }
}

pub(super) fn lower_symbol_path(path: MlSymbolPath) -> SymbolPath {
    SymbolPath {
        segments: path.segments,
    }
}

pub(super) fn lower_import(import: MlImport, pos: Position) -> ImportDecl {
    ImportDecl {
        target: ImportTarget {
            namespace: lower_namespace_name(import.namespace),
            path: lower_symbol_path(import.path),
        },
        alias: import.alias,
        selection: match import.selection {
            MlImportSelection::Whole => ImportSelection::Whole,
            MlImportSelection::Wildcard => ImportSelection::Wildcard,
            MlImportSelection::Members(members) => ImportSelection::Members(
                members
                    .into_iter()
                    .map(|member| ImportMember {
                        name: member.name,
                        alias: member.alias,
                    })
                    .collect(),
            ),
        },
        position: Some(pos),
    }
}

/// Lower implementation declarations while retaining the visibility/opacity
/// wrappers which exist only inside a closed module ([MODULES-EXPORTS]).
pub(super) fn lower_module_items(items: Vec<MlItem>) -> Vec<ModuleItem> {
    crate::ml::module_lower::module_items(items)
}

pub(super) fn lower_signature_item(item: MlSignatureItem) -> SignatureItem {
    crate::ml::module_lower::signature_item(item)
}

pub(super) fn required_type_expr(ty: &MlType) -> TypeExpr {
    crate::ml::module_lower::required_type(ty)
}
