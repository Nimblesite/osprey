//! Effect operations, extern parameters, effect references and type expressions.

use super::{Lowerer, WILDCARD_PARAM};
use osprey_ast::{
    EffectOperation, EffectRef, ExternParameter, Multiplicity, OperationMode, Parameter, TypeExpr,
    TypeParam, Variance,
};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(crate) fn lower_operations(&self, node: Node<'_>) -> Vec<EffectOperation> {
        self.named_of_kind(node, "operation_declaration")
            .iter()
            .map(|op| EffectOperation {
                name: self.field_text(*op, "name"),
                // `control` is what makes an arm own a continuation. Implements
                // [EFFECTS-HANDLER-ARMS].
                mode: match op.child_by_field_name("mode") {
                    Some(_) => OperationMode::Control,
                    None => OperationMode::Value,
                },
                // The keyword AS WRITTEN: absent means `once` everywhere it is
                // read as a value, but [MULTI-AXIS-STATIC] needs to know it was
                // never written. Implements [MULTI-DECL].
                declared_multiplicity: op
                    .child_by_field_name("multiplicity")
                    .and_then(|n| Multiplicity::from_keyword(&self.text(n))),
                replayable: op.child_by_field_name("replayable").is_some(),
                ty: op
                    .child_by_field_name("type")
                    .map(|n| self.text(n))
                    .unwrap_or_default(),
                parameters: Vec::new(),
                return_type: String::new(),
                doc: self.doc_text(*op),
                // Anchor on the NAME, not the node: a leading `///` is now a
                // child of `operation_declaration`, so `pos(op)` would land on
                // the comment and defeat position-based hover resolution.
                position: Some(self.field_pos(*op, "name")),
            })
            .collect()
    }

    pub(crate) fn lower_params(&self, list: Option<Node<'_>>) -> Vec<Parameter> {
        let Some(list) = list else { return Vec::new() };
        self.named_of_kind(list, "parameter")
            .iter()
            .enumerate()
            .map(|(slot, p)| Parameter {
                // `_` takes the generated ignored-parameter name for its slot,
                // the same name the ML lowerer emits, so `|acc, _| => …` and
                // `\(acc, _) => …` stay IR-equivalent twins ([PARAM-WILDCARD]).
                name: match self.field_text(*p, "name") {
                    name if name == WILDCARD_PARAM => osprey_ast::wildcard_param_name(slot),
                    name => name,
                },
                ty: p.child_by_field_name("type").map(|n| self.lower_type(n)),
                inline_constraint: false,
            })
            .collect()
    }

    pub(super) fn lower_extern_params(&self, list: Option<Node<'_>>) -> Vec<ExternParameter> {
        let Some(list) = list else { return Vec::new() };
        self.named_of_kind(list, "extern_parameter")
            .iter()
            .map(|p| ExternParameter {
                name: self.field_text(*p, "name"),
                ty: self.type_or_empty(*p),
            })
            .collect()
    }

    /// Lower a declaration's `type_parameters` field into variance-carrying
    /// [`TypeParam`]s. Implements [TYPE-VARIANCE-DECL].
    pub(crate) fn lower_type_params(&self, node: Node<'_>) -> Vec<TypeParam> {
        let Some(list) = node.child_by_field_name("type_parameters") else {
            return Vec::new();
        };
        self.named_of_kind(list, "type_parameter")
            .iter()
            .map(|tp| TypeParam {
                name: self.field_text(*tp, "name"),
                variance: match tp.child_by_field_name("variance") {
                    Some(v) if self.text(v) == "out" => Variance::Covariant,
                    Some(_) => Variance::Contravariant,
                    None => Variance::Invariant,
                },
            })
            .collect()
    }

    /// Lower a closed or open effect row, preserving a quantified tail.
    /// Implements [EFFECTS-ROW-POLY] and [EFFECTS-GENERIC-ROWS].
    pub(crate) fn lower_effect_row(
        &self,
        effects: Option<Node<'_>>,
    ) -> (Vec<EffectRef>, Option<String>) {
        let Some(effects) = effects else {
            return (Vec::new(), None);
        };
        let tail = effects
            .child_by_field_name("tail")
            .map(|node| self.text(node));
        let refs = self.descendants_of_kind(effects, "effect_ref");
        if tail.is_none() && !self.text(effects).contains('[') {
            if let [effect] = refs.as_slice() {
                let name = self.field_text(*effect, "name");
                if name.chars().next().is_some_and(char::is_lowercase)
                    && !name.contains("::")
                    && self
                        .first_child_of_kind(*effect, "type_arguments")
                        .is_none()
                {
                    return (Vec::new(), Some(name));
                }
            }
        }
        (self.lower_effect_refs(&refs), tail)
    }

    /// Lower each `effect_ref` of a row, with its type arguments.
    pub(super) fn lower_effect_refs(&self, refs: &[Node<'_>]) -> Vec<EffectRef> {
        refs.iter()
            .map(|r| EffectRef {
                name: self.field_text(*r, "name"),
                type_args: self.type_arguments_of(*r),
                position: Some(self.pos(*r)),
            })
            .collect()
    }

    /// Lower a `_type` node (function/generic/array/identifier).
    pub(crate) fn lower_type(&self, node: Node<'_>) -> TypeExpr {
        match node.kind() {
            "type_identifier" => TypeExpr::named(
                self.first_named(node)
                    .map(|n| self.text(n))
                    .unwrap_or_default(),
            ),
            "generic_type" => {
                let mut t = TypeExpr::named(self.field_text(node, "name"));
                if let Some(list) = self.first_child_of_kind(node, "type_list") {
                    t.generic_params = self.lower_type_list(list);
                }
                t
            }
            "array_type" => {
                let mut t = TypeExpr::named(self.field_text(node, "name"));
                t.is_array = true;
                t.array_element = self.descendants_type_in(node).map(Box::new);
                t
            }
            "function_type" => {
                let types = self
                    .first_child_of_kind(node, "type_list")
                    .map(|l| self.lower_type_list(l))
                    .unwrap_or_default();
                // last bare type child is the return type
                let ret = self.last_type_child(node);
                TypeExpr {
                    name: "fn".into(),
                    generic_params: Vec::new(),
                    is_array: false,
                    array_element: None,
                    is_function: true,
                    parameter_types: types,
                    return_type: ret.map(Box::new),
                    position: Some(self.pos(node)),
                }
            }
            _ => TypeExpr::named(self.text(node)),
        }
    }

    /// The declared type a node carries, or an empty name when it wrote none.
    pub(crate) fn type_or_empty(&self, node: Node<'_>) -> TypeExpr {
        node.child_by_field_name("type")
            .map_or_else(|| TypeExpr::named(""), |ty| self.lower_type(ty))
    }

    /// The types a `type_list` holds, lowered in order.
    pub(crate) fn type_list_of(&self, node: Node<'_>) -> Vec<TypeExpr> {
        self.first_child_of_kind(node, "type_list")
            .map_or_else(Vec::new, |list| self.lower_type_list(list))
    }

    /// The explicit type arguments a head wrote, or none when it wrote none.
    pub(crate) fn type_arguments_of(&self, node: Node<'_>) -> Vec<TypeExpr> {
        self.first_child_of_kind(node, "type_arguments")
            .map_or_else(Vec::new, |arguments| self.type_list_of(arguments))
    }

    pub(crate) fn lower_type_list(&self, list: Node<'_>) -> Vec<TypeExpr> {
        let mut out = Vec::new();
        let mut cursor = list.walk();
        for child in list.named_children(&mut cursor) {
            if Self::is_type_kind(child.kind()) {
                out.push(self.lower_type(child));
            }
        }
        out
    }

    fn descendants_type_in(&self, node: Node<'_>) -> Option<TypeExpr> {
        let mut cursor = node.walk();
        let found = node
            .named_children(&mut cursor)
            .find(|c| Self::is_type_kind(c.kind()));
        found.map(|c| self.lower_type(c))
    }

    pub(crate) fn last_type_child(&self, node: Node<'_>) -> Option<TypeExpr> {
        let mut cursor = node.walk();
        let found = node
            .named_children(&mut cursor)
            .filter(|c| Self::is_type_kind(c.kind()))
            .last();
        found.map(|c| self.lower_type(c))
    }

    pub(crate) fn is_type_kind(kind: &str) -> bool {
        matches!(
            kind,
            "type_identifier" | "generic_type" | "array_type" | "function_type"
        )
    }
}
