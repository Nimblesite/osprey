//! TypeScript interfaces that extend `Effect` or `StaticEffect` declare Osprey
//! effects. Implements [FLAVOR-TS-EFFECTS].
use super::context::{Lowerer, Result};
use osprey_ast::{EffectOperation, Multiplicity, OperationMode, Stage, Stmt, TypeExpr};
use oxc_ast::ast::{TSInterfaceDeclaration, TSMethodSignature, TSSignature, TSType, TSTypeName};
use oxc_span::GetSpan;

/// What an operation's result-type wrappers declare.
#[derive(Default)]
struct Markers {
    mode: OperationMode,
    multiplicity: Option<Multiplicity>,
    replayable: bool,
}

impl Lowerer<'_> {
    /// The stage an interface's heritage declares, or `None` for a record.
    pub(super) fn effect_stage(&self, node: &TSInterfaceDeclaration<'_>) -> Result<Option<Stage>> {
        let stage = |heritage: &oxc_ast::ast::TSInterfaceHeritage<'_>| match &heritage.type_name {
            TSTypeName::IdentifierReference(name) if heritage.type_arguments.is_none() => {
                match name.name.as_str() {
                    "Effect" => Some(Stage::Dynamic),
                    "StaticEffect" => Some(Stage::Static),
                    _ => None,
                }
            }
            _ => None,
        };
        let stages: Vec<_> = node.extends.iter().map(stage).collect();
        match stages.as_slice() {
            [] => Ok(None),
            [Some(stage)] => Ok(Some(*stage)),
            many if many.iter().any(Option::is_some) => Err(self.error(
                node.span,
                "an effect extends `Effect` or `StaticEffect` alone",
            )),
            _ => self.unsupported(
                node.extends.first().map_or(node.span, GetSpan::span),
                "interface `extends`",
            ),
        }
    }

    pub(super) fn effect(
        &mut self,
        node: &TSInterfaceDeclaration<'_>,
        stage: Stage,
    ) -> Result<Stmt> {
        let operations = node
            .body
            .body
            .iter()
            .map(|member| match member {
                TSSignature::TSMethodSignature(method) => self.effect_operation(method),
                other => Err(self.error(other.span(), "an effect operation is a method signature")),
            })
            .collect::<Result<_>>()?;
        Ok(Stmt::Effect {
            stage,
            name: node.id.name.as_str().to_owned(),
            type_params: self.type_parameters(node.type_parameters.as_deref())?,
            operations,
            doc: None,
            position: Some(self.at(node.id.span)),
        })
    }

    /// `write(message: string): void` is `write: fn(string) -> Unit`; the
    /// result's `Control`, `Abort`, `Once`, `Many` and `Replayable` wrappers
    /// are the operation's mode, multiplicity and replay marker.
    fn effect_operation(&self, method: &TSMethodSignature<'_>) -> Result<EffectOperation> {
        if method.optional || method.computed || method.type_parameters.is_some() {
            return self.unsupported(method.span, "optional, computed or generic operation");
        }
        let parameters = method
            .params
            .items
            .iter()
            .map(|parameter| {
                self.annotation(parameter.type_annotation.as_deref())?
                    .ok_or_else(|| self.error(parameter.span, "a signature parameter needs a type"))
            })
            .collect::<Result<Vec<_>>>()?;
        let result = method
            .return_type
            .as_deref()
            .ok_or_else(|| self.error(method.span, "an effect operation needs a result type"))?;
        let mut markers = Markers::default();
        let result = self.marked(&result.type_annotation, &mut markers)?;
        Ok(EffectOperation {
            name: self.key(&method.key)?,
            mode: markers.mode,
            declared_multiplicity: markers.multiplicity,
            replayable: markers.replayable,
            ty: signature_text(&parameters, &self.type_expr(result)?),
            parameters: Vec::new(),
            return_type: String::new(),
            doc: None,
            position: Some(self.at(method.key.span())),
        })
    }

    /// The result type inside the operation's wrappers, recording each one.
    fn marked<'b, 'a>(&self, ty: &'b TSType<'a>, markers: &mut Markers) -> Result<&'b TSType<'a>> {
        let TSType::TSTypeReference(reference) = ty else {
            return Ok(ty);
        };
        let (TSTypeName::IdentifierReference(name), Some(arguments)) =
            (&reference.type_name, reference.type_arguments.as_deref())
        else {
            return Ok(ty);
        };
        let [inner] = arguments.params.as_slice() else {
            return Ok(ty);
        };
        match name.name.as_str() {
            "Replayable" if !markers.replayable => markers.replayable = true,
            "Control" if !markers.mode.is_control() => markers.mode = OperationMode::Control,
            keyword @ ("Abort" | "Once" | "Many") if markers.multiplicity.is_none() => {
                markers.mode = OperationMode::Control;
                markers.multiplicity = Multiplicity::from_keyword(&keyword.to_lowercase());
            }
            "Replayable" | "Control" | "Abort" | "Once" | "Many" => {
                return Err(self.error(reference.span, "duplicate operation marker"));
            }
            _ => return Ok(ty),
        }
        self.marked(inner, markers)
    }
}

fn signature_text(parameters: &[TypeExpr], result: &TypeExpr) -> String {
    let arguments: Vec<String> = parameters.iter().map(crate::desugar::field_type).collect();
    format!(
        "fn({}) -> {}",
        arguments.join(", "),
        crate::desugar::field_type(result)
    )
}
