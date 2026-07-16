//! Layout-native ML parsing for namespaces, modules, signatures, exports, and
//! imports. Kept outside the expression parser to isolate [MODULES-*] syntax.

use super::cst::{
    MlImport, MlImportMember, MlImportSelection, MlItem, MlModuleKind, MlNamespaceName,
    MlSignatureItem, MlSymbolPath,
};
use super::parser::Parser;
use super::token::TokKind;

pub(super) fn import_decl(parser: &mut Parser<'_>) -> Option<MlItem> {
    let pos = parser.pos();
    parser.advance();
    let namespace = namespace_name(parser)?;
    let path = path_tail(parser);
    let alias = parser.eat(&TokKind::KwAs).then(|| parser.ident()).flatten();
    let selection = parse_import_selection(parser, alias.is_some());
    Some(MlItem::Import {
        import: MlImport {
            namespace,
            path,
            alias,
            selection,
        },
        pos,
    })
}

fn parse_import_selection(parser: &mut Parser<'_>, aliased: bool) -> MlImportSelection {
    if !parser.eat(&TokKind::Indent) {
        return MlImportSelection::Whole;
    }
    if aliased {
        parser.error("an aliased whole import cannot also select members");
    }
    import_selection(parser)
}

fn import_selection(parser: &mut Parser<'_>) -> MlImportSelection {
    parser.skip_separators();
    if matches!(parser.peek(), TokKind::Op(op) if op == "*") {
        return wildcard_import(parser);
    }
    let members = import_members(parser);
    let _ = parser.eat(&TokKind::Dedent);
    MlImportSelection::Members(members)
}

fn wildcard_import(parser: &mut Parser<'_>) -> MlImportSelection {
    parser.advance();
    parser.skip_separators();
    if !parser.at_block_end() {
        parser.error("wildcard import '*' must be the only selected member");
        parser.recover();
    }
    let _ = parser.eat(&TokKind::Dedent);
    MlImportSelection::Wildcard
}

fn import_members(parser: &mut Parser<'_>) -> Vec<MlImportMember> {
    let mut members = Vec::new();
    while !parser.at_block_end() {
        parser.skip_separators();
        if parser.at_block_end() {
            break;
        }
        if let Some(name) = parser.ident() {
            let alias = parser.eat(&TokKind::KwAs).then(|| parser.ident()).flatten();
            members.push(MlImportMember { name, alias });
        } else {
            parser.recover();
        }
    }
    members
}

pub(super) fn namespace_decl(parser: &mut Parser<'_>) -> Option<MlItem> {
    let pos = parser.pos();
    parser.advance();
    let name = namespace_name(parser)?;
    let body = parser
        .eat(&TokKind::Indent)
        .then(|| items_until_dedent(parser));
    Some(MlItem::Namespace { name, body, pos })
}

pub(super) fn module_decl(parser: &mut Parser<'_>, kind: MlModuleKind) -> Option<MlItem> {
    let pos = parser.pos();
    parser.advance();
    reject_redundant_module(parser, kind);
    let path = symbol_path(parser)?;
    let signature = parser
        .eat(&TokKind::Colon)
        .then(|| symbol_path(parser))
        .flatten();
    let body = module_body(parser);
    Some(MlItem::Module {
        path,
        kind,
        signature,
        body,
        pos,
    })
}

fn reject_redundant_module(parser: &mut Parser<'_>, kind: MlModuleKind) {
    if kind == MlModuleKind::State && parser.eat(&TokKind::KwModule) {
        parser.error("ML state modules are written 'state Name', without redundant 'module'");
    }
}

fn module_body(parser: &mut Parser<'_>) -> Vec<MlItem> {
    if parser.eat(&TokKind::Indent) {
        items_until_dedent(parser)
    } else {
        parser.error("module declaration requires an indented body");
        Vec::new()
    }
}

pub(super) fn signature_decl(parser: &mut Parser<'_>) -> Option<MlItem> {
    let pos = parser.pos();
    parser.advance();
    let name = parser.ident()?;
    let items = if parser.eat(&TokKind::Indent) {
        signature_items(parser)
    } else {
        parser.error("signature declaration requires an indented body");
        Vec::new()
    };
    Some(MlItem::ModuleSignature { name, items, pos })
}

fn signature_items(parser: &mut Parser<'_>) -> Vec<MlSignatureItem> {
    let mut items = Vec::new();
    while !parser.at_block_end() {
        parser.skip_separators();
        if parser.at_block_end() {
            break;
        }
        match signature_item(parser) {
            Some(item) => items.push(item),
            None => parser.recover(),
        }
    }
    let _ = parser.eat(&TokKind::Dedent);
    items
}

fn signature_item(parser: &mut Parser<'_>) -> Option<MlSignatureItem> {
    match parser.peek() {
        TokKind::Ident(_) => signature_value(parser),
        TokKind::KwType => signature_type(parser),
        TokKind::KwEffect => signature_effect(parser),
        TokKind::KwModule => signature_module(parser),
        TokKind::KwExport => redundant_signature_word(
            parser,
            "signature items are public by definition; remove redundant 'export'",
        ),
        TokKind::KwOpaque => redundant_signature_word(
            parser,
            "write 'type T' for an abstract ML signature type; 'opaque' is redundant",
        ),
        other => {
            parser.error(format!("unexpected token {other:?} in signature"));
            None
        }
    }
}

fn redundant_signature_word(
    parser: &mut Parser<'_>,
    message: &'static str,
) -> Option<MlSignatureItem> {
    parser.error(message);
    None
}

fn signature_value(parser: &mut Parser<'_>) -> Option<MlSignatureItem> {
    let pos = parser.pos();
    let name = parser.ident()?;
    let type_params = parser.signature_type_params();
    if !parser.eat(&TokKind::Colon) {
        parser.error("expected ':' in signature value");
    }
    let ty = parser.ty();
    let effects = parser.effect_row();
    Some(MlSignatureItem::Value {
        name,
        type_params,
        ty,
        effects,
        pos,
    })
}

fn signature_type(parser: &mut Parser<'_>) -> Option<MlSignatureItem> {
    let pos = parser.pos();
    parser.advance();
    let name = parser.ident()?;
    let manifest = parser.eat(&TokKind::Eq).then(|| parser.ty());
    Some(MlSignatureItem::Type {
        name,
        manifest,
        pos,
    })
}

fn signature_effect(parser: &mut Parser<'_>) -> Option<MlSignatureItem> {
    let pos = parser.pos();
    parser.advance();
    let name = parser.ident()?;
    let type_params = parser.type_params();
    let operations = parser.effect_operations();
    Some(MlSignatureItem::Effect {
        name,
        type_params,
        operations,
        pos,
    })
}

fn signature_module(parser: &mut Parser<'_>) -> Option<MlSignatureItem> {
    let pos = parser.pos();
    parser.advance();
    let name = parser.ident()?;
    if !parser.eat(&TokKind::Colon) {
        parser.error("expected ':' in nested module signature item");
    }
    let signature = symbol_path(parser)?;
    Some(MlSignatureItem::Module {
        name,
        signature,
        pos,
    })
}

pub(super) fn export_decl(parser: &mut Parser<'_>) -> Option<MlItem> {
    let pos = parser.pos();
    parser.advance();
    if parser.eat(&TokKind::KwExport) {
        parser.error_at(pos, "duplicate 'export' is redundant");
    }
    let item = parser.item()?;
    if !super::modules::is_exportable(&item) {
        parser.error_at(
            pos,
            "'export' must modify a value, function, type, effect, extern, or module declaration",
        );
    }
    Some(MlItem::Export {
        item: Box::new(item),
        pos,
    })
}

pub(super) fn opaque_decl(parser: &mut Parser<'_>) -> Option<MlItem> {
    let pos = parser.pos();
    parser.advance();
    if !parser.eat(&TokKind::KwType) {
        parser.error("'opaque' may modify only a type declaration");
        return None;
    }
    let item = parser.type_decl_after_keyword(pos)?;
    Some(MlItem::Opaque {
        item: Box::new(item),
        pos,
    })
}

fn items_until_dedent(parser: &mut Parser<'_>) -> Vec<MlItem> {
    let mut items = Vec::new();
    while !parser.at_block_end() {
        parser.skip_separators();
        if parser.at_block_end() {
            break;
        }
        match parser.item() {
            Some(item) => items.push(item),
            None => parser.recover(),
        }
    }
    let _ = parser.eat(&TokKind::Dedent);
    items
}

fn namespace_name(parser: &mut Parser<'_>) -> Option<MlNamespaceName> {
    match parser.peek().clone() {
        TokKind::Ident(name) => {
            parser.advance();
            Some(MlNamespaceName::Ident(name))
        }
        TokKind::Str(name) => {
            parser.advance();
            Some(MlNamespaceName::Quoted(name))
        }
        other => {
            parser.error(format!("expected namespace name, found {other:?}"));
            None
        }
    }
}

fn symbol_path(parser: &mut Parser<'_>) -> Option<MlSymbolPath> {
    let first = parser.ident()?;
    let mut segments = vec![first];
    path_segments(parser, &mut segments);
    Some(MlSymbolPath { segments })
}

fn path_tail(parser: &mut Parser<'_>) -> MlSymbolPath {
    let mut segments = Vec::new();
    path_segments(parser, &mut segments);
    MlSymbolPath { segments }
}

fn path_segments(parser: &mut Parser<'_>, segments: &mut Vec<String>) {
    while parser.eat(&TokKind::ColonColon) {
        if let Some(segment) = parser.ident() {
            segments.push(segment);
        } else {
            parser.error("expected path segment after '::'");
            break;
        }
    }
}
