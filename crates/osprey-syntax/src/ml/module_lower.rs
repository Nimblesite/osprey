//! Canonical lowering for ML module bodies and named signatures
//! ([MODULES-MODEL], [MODULES-EXPORTS], [MODULES-SIGNATURE]).

use super::cst::{MlItem, MlSignatureItem, MlType};
use super::lower::{
    arrow_of, arrow_spine, attach_doc, lower_binding, lower_effect_op, lower_effect_ref,
    lower_items, lower_type_param, render_type, type_expr, MlSig,
};
use osprey_ast::{
    DocComment, DocScope, ModuleItem, SignatureAscription, SignatureItem, SignatureType, Stmt,
    SymbolPath, TypeExpr, Visibility,
};

pub(super) fn module_items(items: Vec<MlItem>) -> Vec<ModuleItem> {
    let mut state = ModuleLower::default();
    for item in items {
        state.lower_item(item);
    }
    state.out
}

#[derive(Default)]
struct ModuleLower {
    out: Vec<ModuleItem>,
    pending: Option<(MlSig, Visibility)>,
    doc: Option<DocComment>,
}

impl ModuleLower {
    fn lower_item(&mut self, raw: MlItem) {
        if let MlItem::Doc(text) = raw {
            self.doc = Some(crate::docparse::parse_doc(&text, DocScope::Outer));
            return;
        }
        let (visibility, opaque, item) = item_metadata(raw);
        match item {
            MlItem::ValueSignature {
                name,
                type_params,
                ty,
                effects,
                ..
            } => self.pending = Some((MlSig::new(name, type_params, ty, effects), visibility)),
            MlItem::Binding {
                mutable,
                name,
                params,
                uncurried,
                body,
                pos,
            } => self.lower_binding_item(
                visibility, opaque, mutable, name, params, uncurried, body, pos,
            ),
            other => self.lower_declaration(visibility, opaque, other),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "faithful Binding CST destructuring before canonical construction"
    )]
    fn lower_binding_item(
        &mut self,
        visibility: Visibility,
        opaque: bool,
        mutable: bool,
        name: String,
        params: Vec<super::cst::MlParam>,
        uncurried: bool,
        body: super::cst::MlExpr,
        pos: osprey_ast::Position,
    ) {
        let paired = self.pending.take().filter(|(sig, _)| sig.name == name);
        let visibility = paired
            .as_ref()
            .map_or(visibility, |(_, signed_visibility)| *signed_visibility);
        let sig = paired.map(|(sig, _)| sig);
        let stmt = lower_binding(mutable, name, params, uncurried, body, pos, sig);
        let doc = self.doc.take();
        self.push(visibility, opaque, attach_doc(stmt, doc));
    }

    fn lower_declaration(&mut self, visibility: Visibility, opaque: bool, item: MlItem) {
        self.pending = None;
        for stmt in lower_items(vec![item]) {
            let statement = attach_declaration_doc(stmt, self.doc.take());
            self.push(visibility, opaque, statement);
        }
    }

    fn push(&mut self, visibility: Visibility, opaque: bool, declaration: Stmt) {
        self.out.push(ModuleItem {
            visibility,
            opaque,
            declaration: Box::new(declaration),
        });
    }
}

fn item_metadata(item: MlItem) -> (Visibility, bool, MlItem) {
    match item {
        MlItem::Export { item, .. } => {
            let (_, opaque, declaration) = item_metadata(*item);
            (Visibility::Exported, opaque, declaration)
        }
        MlItem::Opaque { item, .. } => {
            let (visibility, _, declaration) = item_metadata(*item);
            (visibility, true, declaration)
        }
        declaration => (Visibility::Private, false, declaration),
    }
}

fn attach_declaration_doc(mut stmt: Stmt, doc: Option<DocComment>) -> Stmt {
    match &mut stmt {
        Stmt::Function { doc: slot, .. }
        | Stmt::Let { doc: slot, .. }
        | Stmt::Extern { doc: slot, .. }
        | Stmt::Type { doc: slot, .. }
        | Stmt::Effect { doc: slot, .. }
        | Stmt::Module { doc: slot, .. }
        | Stmt::Signature { doc: slot, .. } => *slot = doc,
        _ => {}
    }
    stmt
}

pub(super) fn signature_item(item: MlSignatureItem) -> SignatureItem {
    match item {
        MlSignatureItem::Value {
            name,
            type_params,
            ty,
            effects,
            pos,
        } => signature_value(name, type_params, &ty, effects, pos),
        MlSignatureItem::Type {
            name,
            manifest,
            pos,
        } => signature_type(name, manifest, pos),
        MlSignatureItem::Effect {
            name,
            type_params,
            operations,
            pos,
        } => SignatureItem::Effect {
            name,
            type_params: type_params.into_iter().map(lower_type_param).collect(),
            operations: operations.into_iter().map(lower_effect_op).collect(),
            position: Some(pos),
        },
        MlSignatureItem::Module {
            name,
            signature,
            pos,
        } => SignatureItem::Module {
            path: SymbolPath::single(name),
            signature: SignatureAscription {
                path: SymbolPath {
                    segments: signature.segments,
                },
                allow_extra: false,
            },
            position: Some(pos),
        },
    }
}

fn signature_type(
    name: String,
    manifest: Option<MlType>,
    pos: osprey_ast::Position,
) -> SignatureItem {
    let opaque = manifest.is_none();
    let definition = manifest.map_or(SignatureType::Abstract, |ty| {
        SignatureType::Manifest(required_type(&ty))
    });
    SignatureItem::Type {
        name,
        type_params: Vec::new(),
        definition,
        opaque,
        position: Some(pos),
    }
}

fn signature_value(
    name: String,
    type_params: Vec<super::cst::MlTypeParam>,
    ty: &MlType,
    effects: Vec<super::cst::MlEffectRef>,
    pos: osprey_ast::Position,
) -> SignatureItem {
    let spine = arrow_spine(ty);
    if spine.len() < 2 {
        return SignatureItem::Value {
            name,
            ty: required_type(ty),
            position: Some(pos),
        };
    }
    let (parameters, return_type) = signature_function_parts(&spine);
    SignatureItem::Function {
        name,
        type_params: type_params.into_iter().map(lower_type_param).collect(),
        parameters,
        return_type,
        effects: effects.into_iter().map(lower_effect_ref).collect(),
        position: Some(pos),
    }
}

fn signature_function_parts(spine: &[MlType]) -> (Vec<TypeExpr>, TypeExpr) {
    match spine.split_first() {
        Some((MlType::Tuple(parts), rest)) => (
            parts.iter().map(required_type).collect(),
            arrow_of(rest).unwrap_or_else(|| TypeExpr::named("Unit")),
        ),
        Some((MlType::Name(name), rest)) if name == "Unit" => (
            Vec::new(),
            arrow_of(rest).unwrap_or_else(|| TypeExpr::named("Unit")),
        ),
        Some((first, rest)) => (
            vec![required_type(first)],
            arrow_of(rest).unwrap_or_else(|| TypeExpr::named("Unit")),
        ),
        None => (Vec::new(), TypeExpr::named("Unit")),
    }
}

pub(super) fn required_type(ty: &MlType) -> TypeExpr {
    type_expr(ty).unwrap_or_else(|| TypeExpr::named(render_type(ty)))
}
