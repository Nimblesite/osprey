//! Class-shaped capabilities and handlers use the canonical effects core.
//! Implements [FLAVOR-PYTHON-EFFECT], [FLAVOR-PYTHON-HANDLER].
use super::context::{children, position, Lowerer, Result};
use super::declarations::render;
use osprey_ast::{
    EffectOperation, EffectRef, Expr, HandlerArm, Multiplicity, OperationMode, Stage, Stmt,
};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn effect(&mut self, node: Node<'_>, stage: Stage) -> Result<Stmt> {
        self.reject_field(node, "superclasses")?;
        let name = self.declaration_name(node)?;
        let operations = self
            .body_nodes(Self::field(node, "body")?)
            .into_iter()
            .map(|operation| self.operation(operation))
            .collect::<Result<_>>()?;
        self.documented(
            node,
            Stmt::Effect {
                stage,
                name: self.text(name).into(),
                type_params: self.type_parameters(node)?,
                operations,
                doc: None,
                position: Some(position(node)),
            },
        )
    }

    fn operation(&mut self, node: Node<'_>) -> Result<EffectOperation> {
        let (definition, markers) = self.operation_markers(node)?;
        self.signature_only(definition)?;
        self.reject_field(definition, "type_parameters")?;
        self.scoped(true, |lowerer| {
            lowerer.operation_signature(definition, markers)
        })
    }

    fn operation_signature(
        &mut self,
        definition: Node<'_>,
        markers: Markers,
    ) -> Result<EffectOperation> {
        let ty = format!("fn{}", self.signature_text(definition)?);
        Ok(EffectOperation {
            name: self.text(Self::field(definition, "name")?).into(),
            mode: markers.mode,
            declared_multiplicity: markers.multiplicity,
            replayable: markers.replayable,
            ty,
            parameters: Vec::new(),
            return_type: String::new(),
            doc: self.declaration_doc(definition)?,
            position: Some(position(Self::field(definition, "name")?)),
        })
    }

    pub(super) fn signature_text(&mut self, node: Node<'_>) -> Result<String> {
        let params = self.typed_parameters(node)?;
        let result = self.type_expr(Self::field(node, "return_type")?)?;
        let arguments = params
            .iter()
            .map(|p| render(&p.ty))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!("({arguments}) -> {}", render(&result)))
    }

    fn operation_markers<'t>(&self, node: Node<'t>) -> Result<(Node<'t>, Markers)> {
        if node.kind() == "function_definition" {
            return Ok((node, Markers::default()));
        }
        if node.kind() != "decorated_definition" {
            return self.unsupported(node);
        }
        let mut markers = Markers::default();
        for decorator in children(node)
            .into_iter()
            .filter(|n| n.kind() == "decorator")
        {
            markers.add(self.text(Self::first(decorator)?), decorator)?;
        }
        let definition = self.kind(Self::field(node, "definition")?, "function_definition")?;
        Ok((definition, markers))
    }

    pub(super) fn handler(&mut self, node: Node<'_>, decorator: Node<'_>) -> Result<Stmt> {
        self.reject_field(node, "superclasses")?;
        self.reject_field(node, "type_parameters")?;
        let (stage, effect) = self.handler_head(decorator)?;
        let name = self.declaration_name(node)?;
        let arms = self
            .body_nodes(Self::field(node, "body")?)
            .into_iter()
            .map(|arm| self.handler_arm(arm))
            .collect::<Result<_>>()?;
        let function = osprey_ast::handler_function(
            self.text(name).into(),
            stage,
            effect,
            arms,
            Some(position(node)),
        );
        self.documented(node, function)
    }

    fn handler_head(&self, node: Node<'_>) -> Result<(Stage, String)> {
        let node = self.kind(node, "call")?;
        let stage = match self.text(Self::field(node, "function")?) {
            "handler" => Stage::Dynamic,
            "static_handler" => Stage::Static,
            _ => return self.unsupported(node),
        };
        let effect = self.single(Self::field(node, "arguments")?)?;
        Ok((stage, self.effect_name(effect)?))
    }

    fn handler_arm(&mut self, node: Node<'_>) -> Result<HandlerArm> {
        if node.kind() != "function_definition" {
            return self.unsupported(node);
        }
        Self::synchronous(node)?;
        self.reject_field(node, "type_parameters")?;
        self.reject_field(node, "return_type")?;
        self.scoped(true, |lowerer| {
            let parameters = lowerer.parameters(node)?;
            if parameters.iter().any(|p| p.ty.is_some()) {
                return lowerer.unsupported(node);
            }
            Ok(HandlerArm {
                operation: lowerer.text(Self::field(node, "name")?).into(),
                params: parameters.into_iter().map(|p| p.name).collect(),
                body: lowerer.suite(Self::field(node, "body")?, true)?,
                position: Some(position(Self::field(node, "name")?)),
            })
        })
    }

    fn effect_name(&self, node: Node<'_>) -> Result<String> {
        let reference = self.effect_reference(node)?;
        Ok(osprey_ast::effect_name::instantiated(
            &reference.name,
            &reference.type_args,
        ))
    }

    fn effect_reference(&self, node: Node<'_>) -> Result<EffectRef> {
        let (base, type_args) = if node.kind() == "subscript" {
            (Self::field(node, "value")?, self.type_arguments(node, 1)?)
        } else {
            (node, Vec::new())
        };
        let name = self.logical_path(base)?.join("::");
        Ok(EffectRef {
            name,
            type_args,
            position: Some(position(node)),
        })
    }

    pub(super) fn required_function(
        &mut self,
        node: Node<'_>,
        decorator: Node<'_>,
    ) -> Result<Stmt> {
        if decorator.kind() != "call"
            || self.text(Self::field(decorator, "function")?) != "requires"
        {
            return self.unsupported(decorator);
        }
        let (requirements, tail) = self.requirements(Self::field(decorator, "arguments")?)?;
        let mut statement = self.function(node)?;
        set_requirements(&mut statement, requirements, tail);
        Ok(statement)
    }

    fn requirement_tail(&mut self, node: Node<'_>) -> Result<String> {
        if self.text(Self::field(node, "name")?) != "tail" {
            return self.unsupported(node);
        }
        match self.expr(Self::field(node, "value")?)? {
            Expr::Str(name) if valid_tail(&name) => Ok(name),
            _ => self.unsupported(node),
        }
    }

    fn requirements(&mut self, node: Node<'_>) -> Result<(Vec<EffectRef>, Option<String>)> {
        let mut effects = Vec::new();
        let mut tail = None;
        for arg in children(node) {
            if tail.is_some() {
                return self.unsupported(arg);
            }
            if arg.kind() == "keyword_argument" {
                tail = Some(self.requirement_tail(arg)?);
            } else {
                effects.push(self.effect_reference(arg)?);
            }
        }
        Ok((effects, tail))
    }

    pub(super) fn intrinsic(&mut self, node: Node<'_>, function: Node<'_>) -> Result<Option<Expr>> {
        let name = self.text(function);
        if name == "perform" {
            return self.perform(node).map(Some);
        }
        if name != "resume" {
            return self.fiber_intrinsic(node, name);
        }
        let (args, named) = self.arguments(Self::field(node, "arguments")?)?;
        if !named.is_empty() || args.len() > 1 {
            return self.unsupported(node);
        }
        Ok(Some(Expr::Resume(args.into_iter().next().map(Box::new))))
    }

    fn perform(&mut self, node: Node<'_>) -> Result<Expr> {
        let request = self.kind(self.single(Self::field(node, "arguments")?)?, "call")?;
        let function = self.kind(Self::field(request, "function")?, "attribute")?;
        let effect = self.effect_name(Self::field(function, "object")?)?;
        let operation = self.text(Self::field(function, "attribute")?).into();
        let (arguments, named_arguments) = self.arguments(Self::field(request, "arguments")?)?;
        Ok(Expr::Perform {
            effect,
            operation,
            arguments,
            named_arguments,
            position: Some(position(node)),
        })
    }

    pub(super) fn with_handler(&mut self, node: Node<'_>, tail: bool) -> Result<Expr> {
        Self::synchronous(node)?;
        let item = self.single(Self::first(node)?)?;
        let value = Self::field(item, "value")?;
        if !matches!(value.kind(), "identifier" | "attribute") {
            return self.unsupported(value);
        }
        let function = self.expr(value)?;
        let callback = self.handler_callback(node, tail)?;
        Ok(super::context::apply(function, vec![callback]))
    }

    fn handler_callback(&mut self, node: Node<'_>, tail: bool) -> Result<Expr> {
        let body = self.scoped(false, |lowerer| {
            lowerer.suite(Self::field(node, "body")?, tail)
        })?;
        Ok(Expr::Lambda {
            parameters: Vec::new(),
            return_type: None,
            body: Box::new(body),
            position: Some(position(node)),
        })
    }
}

fn set_requirements(statement: &mut Stmt, requirements: Vec<EffectRef>, tail: Option<String>) {
    if let Stmt::Function {
        effects,
        effect_tail,
        effect_row_present,
        ..
    } = statement
    {
        *effects = requirements;
        *effect_tail = tail;
        *effect_row_present = true;
    }
}

fn valid_tail(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_lowercase)
        && name.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
}

#[derive(Clone, Copy, Default)]
struct Markers {
    mode: OperationMode,
    multiplicity: Option<Multiplicity>,
    replayable: bool,
}

impl Markers {
    fn add(&mut self, name: &str, node: Node<'_>) -> Result<()> {
        match name {
            "control" if !self.mode.is_control() => self.mode = OperationMode::Control,
            "replayable" if !self.replayable => self.replayable = true,
            other => match Multiplicity::from_keyword(other) {
                Some(value) if self.multiplicity.is_none() => self.multiplicity = Some(value),
                _ => {
                    return Err(Lowerer::error(
                        node,
                        "unknown or duplicate operation modifier",
                    ))
                }
            },
        }
        Ok(())
    }
}
