//! Type checking: declarations.
use super::{
    parse_fn_sig, type_name_to_type, Checker, CtorInfo, EffectInfo, EffectOperation, EffectRef,
    EffectScope, HashMap, Position, Type, TypeError, TypeParam, TypeVariant,
};

impl Checker {
    /// Registers constructor fields before body inference, including fields
    /// that refer to their own or another declared union [TYPE-UNION-REC].
    pub(super) fn collect_type(
        &mut self,
        name: &str,
        type_params: &[TypeParam],
        variants: &[TypeVariant],
        position: Option<Position>,
    ) {
        let is_record = match variants.first() {
            Some(first) => variants.len() == 1 && first.name == name,
            None => false,
        };
        if !is_record {
            let _ = self.union_variants.insert(
                name.to_string(),
                variants.iter().map(|v| v.name.clone()).collect(),
            );
        }
        let param_names: Vec<String> = type_params.iter().map(|p| p.name.clone()).collect();
        for v in variants {
            let fields: Vec<(String, String)> = v
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.ty.clone()))
                .collect();
            if is_record {
                self.ctx
                    .set_record(name.to_owned(), param_names.clone(), fields.clone());
            }
            let _ = self.ctors.insert(
                v.name.clone(),
                CtorInfo {
                    owner: name.to_string(),
                    owner_is_record: is_record,
                    type_params: param_names.clone(),
                    fields,
                },
            );
        }
        for e in crate::variance::validate_type_decl(&self.ctx, name, type_params, variants) {
            self.record_err(e, position);
        }
    }

    pub(super) fn collect_effect(
        &mut self,
        stage: osprey_ast::Stage,
        name: &str,
        type_params: &[TypeParam],
        operations: &[EffectOperation],
        position: Option<Position>,
    ) {
        if name == osprey_ast::ARITH_EFFECT {
            self.record_err(
                TypeError::new("cannot redeclare compiler built-in effect `Arith`"),
                position,
            );
            return;
        }
        let _ = self.effects.insert(
            name.to_string(),
            EffectInfo {
                type_params: type_params.iter().map(|p| p.name.clone()).collect(),
                ops: operations
                    .iter()
                    .map(|op| (op.name.clone(), op.ty.clone(), op.mode))
                    .collect(),
            },
        );
        for e in crate::variance::validate_effect_decl(&self.ctx, name, type_params, operations) {
            self.record_err(e, position);
        }
        // Implements [MULTI-DECL].
        for operation in operations {
            if let Some(message) = operation.modifier_error(name, stage) {
                self.record_err(TypeError::new(message), operation.position.or(position));
            }
        }
    }

    /// Instantiate an effect's operations at fresh type arguments — one
    /// instance per handle site / unresolved perform. Returns `None` for an
    /// undeclared effect. Implements [EFFECTS-GENERIC-INSTANTIATION].
    pub(crate) fn effect_instance_ops(
        &mut self,
        effect: &str,
    ) -> Option<(Vec<Type>, HashMap<String, crate::info::OpType>)> {
        let info = self
            .effects
            .get(osprey_ast::effect_name::base(effect))?
            .clone();
        let mut pmap = HashMap::new();
        let mut args = Vec::new();
        let written = match type_name_to_type(effect, &HashMap::new()) {
            Type::Con { args, .. } => args,
            _ => Vec::new(),
        };
        for (index, p) in info.type_params.iter().enumerate() {
            let v = written
                .get(index)
                .cloned()
                .unwrap_or_else(|| self.ctx.fresh());
            let v = self.resolve_effect_argument(v);
            args.push(v.clone());
            let _ = pmap.insert(p.clone(), v);
        }
        Some((args, Self::instantiate_ops(&info, &pmap)))
    }

    /// A written `Echo<Point>` and an inferred `Echo` fed a `Point` value
    /// identify the same operation. Resolve nominal record applications to
    /// their instantiated structural shape before publishing effect-site
    /// identities; otherwise a handler keyed by `Point` cannot discharge a
    /// request keyed by `{ x: int, ... }`. Preserve the declaration name for
    /// value layout and diagnostics, while the effect-row key uses its fields.
    pub(super) fn resolve_effect_argument(&mut self, ty: Type) -> Type {
        match ty {
            Type::Con { name, args } => {
                let args: Vec<_> = args
                    .into_iter()
                    .map(|argument| self.resolve_effect_argument(argument))
                    .collect();
                if let Some(fields) = self.ctx.record_fields(&name, &args) {
                    Type::Record { name, fields }
                } else {
                    Type::Con { name, args }
                }
            }
            Type::Fun { params, ret } => Type::Fun {
                params: params
                    .into_iter()
                    .map(|parameter| self.resolve_effect_argument(parameter))
                    .collect(),
                ret: Box::new(self.resolve_effect_argument(*ret)),
            },
            other => other,
        }
    }

    /// Instantiate an effect at an effect-row entry's declared type arguments
    /// (`!State<int>`), resolving each argument against the enclosing
    /// function's type parameters. An omitted argument list is inferred; a
    /// written list must supply exactly the declared number of arguments.
    /// Implements [EFFECTS-GENERIC-ROWS].
    pub(super) fn effect_row_scope(
        &mut self,
        row: &EffectRef,
        fn_typarams: &HashMap<String, Type>,
        position: Option<Position>,
    ) -> Option<EffectScope> {
        let info = self.effects.get(&row.name)?.clone();
        if !row.type_args.is_empty() && row.type_args.len() != info.type_params.len() {
            self.errors.push(
                TypeError::new(format!(
                    "effect `{}` takes {} type argument(s), got {}",
                    row.name,
                    info.type_params.len(),
                    row.type_args.len()
                ))
                .with_pos(position),
            );
        }
        let mut pmap = HashMap::new();
        let mut args = Vec::new();
        for (i, p) in info.type_params.iter().enumerate() {
            let t = match row.type_args.get(i) {
                Some(te) => self.written_type_argument(te, fn_typarams, position),
                None => self.ctx.fresh(),
            };
            let t = self.resolve_effect_argument(t);
            args.push(t.clone());
            let _ = pmap.insert(p.clone(), t);
        }
        Some(EffectScope {
            name: row.name.clone(),
            args,
            ops: Self::instantiate_ops(&info, &pmap),
        })
    }

    /// Parse an effect's raw operation signatures against an instantiation of
    /// its type parameters.
    pub(super) fn instantiate_ops(
        info: &EffectInfo,
        pmap: &HashMap<String, Type>,
    ) -> HashMap<String, crate::info::OpType> {
        info.ops
            .iter()
            .map(|(op_name, sig, mode)| {
                let (params, ret) = parse_fn_sig(sig, pmap);
                (
                    op_name.clone(),
                    crate::info::OpType {
                        params,
                        ret,
                        mode: *mode,
                    },
                )
            })
            .collect()
    }
}
