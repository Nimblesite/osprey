//! Structural API details come from declarations, never from implementation bodies.
//! Implements [DOC-EXPORT].

use osprey_ast::{SignatureItem, SignatureType, Stmt, TypeVariant};
use osprey_lsp::analysis::{render_type, render_type_params as binder};

pub(super) fn details(statement: Option<&Stmt>, opaque: bool) -> String {
    match statement {
        Some(Stmt::Type {
            alias, variants, ..
        }) if !opaque => alias.as_ref().map_or_else(
            || {
                variants
                    .iter()
                    .map(variant)
                    .collect::<Vec<_>>()
                    .join("\n\n")
            },
            |ty| format!("**Representation:** `{}`", render_type(ty)),
        ),
        Some(Stmt::Signature { items, .. }) => {
            format!(
                "## Public interface\n\n{}",
                items
                    .iter()
                    .map(signature_item)
                    .collect::<Vec<_>>()
                    .join("\n\n")
            )
        }
        _ => String::new(),
    }
}

fn variant(variant: &TypeVariant) -> String {
    let fields = variant
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let name = if osprey_ast::is_positional_field(&field.name) {
                format!("payload {}", index + 1)
            } else {
                field.name.clone()
            };
            let constraint = if field.constraint.is_some() {
                " (validated)"
            } else {
                ""
            };
            format!("- `{name}: {}`{constraint}", field.ty)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!("### {}\n\n{fields}", variant.name)
}

fn signature_item(item: &SignatureItem) -> String {
    let rendered = match item {
        SignatureItem::Value { name, ty, .. } => format!("{name}: {}", render_type(ty)),
        SignatureItem::Function {
            name,
            type_params,
            parameters,
            return_type,
            effects,
            ..
        } => {
            let params = parameters
                .iter()
                .map(render_type)
                .collect::<Vec<_>>()
                .join(", ");
            let row = effects
                .iter()
                .map(effect_ref)
                .collect::<Vec<_>>()
                .join(", ");
            let suffix = if row.is_empty() {
                String::new()
            } else {
                format!(" ![{row}]")
            };
            format!(
                "fn {name}{}({params}) -> {}{suffix}",
                binder(type_params),
                render_type(return_type)
            )
        }
        SignatureItem::Type {
            name,
            type_params,
            definition,
            opaque,
            ..
        } => {
            let representation = match definition {
                SignatureType::Manifest(ty) if !opaque => format!(" = {}", render_type(ty)),
                _ => String::new(),
            };
            format!("type {name}{}{representation}", binder(type_params))
        }
        SignatureItem::Effect {
            name,
            type_params,
            operations,
            ..
        } => format!(
            "effect {name}{} {{ {} }}",
            binder(type_params),
            operations
                .iter()
                .map(|operation| format!("{}: {}", operation.name, operation.ty))
                .collect::<Vec<_>>()
                .join("; ")
        ),
        SignatureItem::Module {
            path, signature, ..
        } => format!("module {path}: {}", signature.path),
    };
    format!("- `{rendered}`")
}

fn effect_ref(effect: &osprey_ast::EffectRef) -> String {
    if effect.type_args.is_empty() {
        return effect.name.clone();
    }
    format!(
        "{}<{}>",
        effect.name,
        effect
            .type_args
            .iter()
            .map(render_type)
            .collect::<Vec<_>>()
            .join(", ")
    )
}
