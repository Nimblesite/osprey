//! Emitter function types.
use super::{
    ltype_of, Codegen, CtorView, Expr, FiberSig, FnSig, LType, ParamSig, ProgramTypes, Type,
};

impl Codegen {
    /// Register a function-typed local: its lowered signature for indirect
    /// calls plus its full [`Type`] for chained applications.
    pub(crate) fn bind_fn_local(&mut self, name: &str, ty: Type) {
        if let Some(sig) = Codegen::fn_value_sig(&self.prog, &ty) {
            let _ = self.fn_ptr_locals.insert(name.to_string(), sig);
            let _ = self.fn_value_types.insert(name.to_string(), ty);
        }
    }

    /// The function type of the value a call to `f` returns, when `f` is a
    /// top-level function or a function-typed local that returns a function.
    pub(crate) fn call_result_fn_type(&self, f: &str) -> Option<Type> {
        let ret = match self.prog.return_type(f) {
            Some(ret) => ret.clone(),
            // Not a top-level name: whatever function value `f` denotes here —
            // a local, a module global, or an inlining alias — its return type
            // is the arrow one application peels off ([`identifier_fn_type`]).
            None => match self.identifier_fn_type(f)? {
                Type::Fun { ret, .. } => *ret,
                _ => return None,
            },
        };
        matches!(ret, Type::Fun { .. }).then_some(ret)
    }

    /// The function [`Type`] an expression evaluates to in callee/callback
    /// position — `None` when it is not (statically) a function value. Powers
    /// higher-order calls through arbitrary callee expressions: a chained
    /// application (`add3(1)(2)(3)`), a function held in a record field
    /// (`cfg.processor`), or a function-typed local. Implements
    /// [TYPE-FN-HIGHER-ORDER].
    pub(crate) fn callee_fn_type(&self, expr: &Expr) -> Option<Type> {
        match expr {
            Expr::TypeApply {
                function, position, ..
            } => self
                .callee_fn_type(function)
                .map(|ty| self.prog.application_type(*position, &ty)),
            Expr::Identifier(name) => self.identifier_fn_type(name),
            // A call evaluates to its callee's return type — recurse so a chain
            // peels one arrow per application.
            Expr::Call { function, .. } => match self.callee_fn_type(function)? {
                Type::Fun { ret, .. } => Some(*ret),
                _ => None,
            },
            Expr::FieldAccess { target, field } => self.field_fn_type(target, field),
            _ => None,
        }
    }

    /// The function type of a named callee: a function-typed local first (its
    /// inferred value type), else a top-level function's resolved signature.
    pub(super) fn identifier_fn_type(&self, name: &str) -> Option<Type> {
        if let Some(t) = self.fn_value_types.get(name).or_else(|| {
            self.lambda_def(name)
                .and_then(|(_, _, position)| self.prog.lambda_type(*position))
        }) {
            return Some(t.clone());
        }
        // A function-valued parameter bound while INLINING a generic function
        // is recorded as an alias of the callee it stands for, not as a local
        // value ([`crate::genfn`]). A chained application through it —
        // `fn apply(f, a, b) = f(a)(b)`, the shape ML's curry-by-default gives
        // every higher-order body — has to follow that alias to find the
        // arrow it is peeling. [TYPE-FN-HIGHER-ORDER]
        if let Some(target) = self.call_aliases.get(name).filter(|t| *t != name) {
            return self.identifier_fn_type(&target.clone());
        }
        // A file-scope function value read from inside a function body: its
        // closure cell lives in a module global, not this frame
        // ([`crate::globals`]).
        if let Some(t) = crate::globals::fn_type(self, name) {
            return Some(t);
        }
        match self.prog.functions.get(name) {
            Some((params, ret)) => Some(Type::fun(params.clone(), ret.clone())),
            None => osprey_types::builtin_function_type(name),
        }
    }

    /// The type of `target.field` when `field` names a function-valued record
    /// field — resolving `target`'s owner from the bound value's type tag, or a
    /// unique field-name match across known layouts.
    pub(super) fn field_fn_type(&self, target: &Expr, field: &str) -> Option<Type> {
        let owner = self.callee_field_owner(target, field)?;
        self.ctor_field_ty(&owner, field).cloned()
    }

    /// Resolve the owner type of `target.field`: prefer a bound identifier's
    /// static type tag, else the unique constructor declaring `field`.
    pub(super) fn callee_field_owner(&self, target: &Expr, field: &str) -> Option<String> {
        if let Expr::Identifier(name) = target {
            let tagged = self.lookup(name).and_then(|v| v.osp_ty);
            if let Some(owner) = tagged {
                if self.declares_field(&owner, field) {
                    return Some(owner);
                }
            }
        }
        self.find_field_owner(field)
    }

    /// The declared type of `field` on constructor `owner` — the single field
    /// lookup behind [`Self::declares_field`] and every `ctor_field_*` accessor.
    pub(crate) fn ctor_field_ty(&self, owner: &str, field: &str) -> Option<&Type> {
        self.prog
            .ctors
            .get(owner)?
            .fields
            .iter()
            .find(|(f, _)| f == field)
            .map(|(_, t)| t)
    }

    /// Whether `owner`'s layout declares `field`.
    pub(super) fn declares_field(&self, owner: &str, field: &str) -> bool {
        self.ctor_field_ty(owner, field).is_some()
    }

    /// Whether `name` is a user function whose inferred signature still contains
    /// a type variable (in a parameter or the return) — i.e. it is polymorphic
    /// and must be specialised to the concrete call-site types.
    pub(crate) fn is_generic_fn(&self, name: &str) -> bool {
        let Some((params, ret)) = self.prog.functions.get(name) else {
            return false;
        };
        params
            .iter()
            .chain(std::iter::once(ret))
            .any(osprey_types::has_type_var)
    }

    /// The declared type parameters of a constructor's owner (`["T"]` for
    /// `Generic<T>`), used to spot a generic field whose LLVM type is fixed per
    /// construction rather than by the (placeholder) written type.
    pub(crate) fn ctor_type_params(&self, name: &str) -> Vec<String> {
        self.prog
            .ctors
            .get(name)
            .map(|c| c.type_params.clone())
            .unwrap_or_default()
    }

    /// The lowered [`FnSig`] of a function-typed value `ty`, for the closure
    /// ABI — `None` if `ty` is not a function. Result returns retain their
    /// discriminant-bearing ABI; function values must never erase failure.
    pub(crate) fn fn_value_sig(prog: &ProgramTypes, ty: &Type) -> Option<FnSig> {
        match ty {
            Type::Fun { params, ret } => Some((
                params.iter().map(|t| ParamSig::of(prog, t)).collect(),
                ltype_of(ret),
                crate::types::result_inner(ret),
                FiberSig::of(prog, ret),
                crate::types::result_payload_owner(prog, ret)
                    .or_else(|| crate::types::owner_name(prog, ret)),
            )),
            _ => None,
        }
    }

    // ---- inferred typing ----

    /// The LLVM return type of a user/runtime function, from inference.
    pub(crate) fn fn_ret_ltype(&self, name: &str) -> Option<LType> {
        self.prog.return_type(name).map(ltype_of)
    }

    /// Whether a user function is inferred to return `Unit` — i.e. its body's
    /// value is discarded.
    pub(crate) fn fn_ret_is_unit(&self, name: &str) -> bool {
        self.prog
            .return_type(name)
            .is_some_and(|t| *t == Type::unit())
    }

    /// The LLVM parameter types of a user function, from inference.
    pub(crate) fn fn_param_ltypes(&self, name: &str) -> Option<Vec<LType>> {
        self.prog
            .param_types(name)
            .map(|ps| ps.iter().map(|t| ParamSig::of(&self.prog, t).ty).collect())
    }

    /// Full parameter ABI slots, including Result layout metadata.
    pub(crate) fn fn_param_abis(&self, name: &str) -> Option<Vec<ParamSig>> {
        self.prog
            .param_types(name)
            .map(|ps| ps.iter().map(|t| ParamSig::of(&self.prog, t)).collect())
    }

    /// The `(LType, owner)` parameter signature — `owner` tags record/union
    /// parameters so their fields are reachable inside the body.
    pub(crate) fn fn_param_sig(&self, name: &str) -> Option<Vec<(ParamSig, Option<String>)>> {
        self.prog.param_types(name).map(|ps| {
            ps.iter()
                .map(|t| {
                    // A handle parameter has no owner of its own, so that slot
                    // carries its ELEMENT's tag for `recv`/`await` to restore.
                    let owner = crate::types::owner_name(&self.prog, t)
                        .or_else(|| crate::types::handle_elem_owner(&self.prog, t));
                    (ParamSig::of(&self.prog, t), owner)
                })
                .collect()
        })
    }

    /// The owner type name of a function's return value, if it is a record/union.
    pub(crate) fn fn_ret_owner(&self, name: &str) -> Option<String> {
        self.prog
            .return_type(name)
            .and_then(|t| crate::types::owner_name(&self.prog, t))
    }

    /// The inner [`LType`] when a function is declared to return `Result<T, E>`
    /// — the success payload's LLVM type — so calls and returns carry the
    /// `{ T, i8 }*` Result block rather than a bare `T`.
    pub(crate) fn fn_ret_result_inner(&self, name: &str) -> Option<LType> {
        crate::types::result_inner(self.prog.return_type(name)?)
    }

    /// Fiber element shape carried by a function's erased `i64` return slot.
    pub(crate) fn fn_ret_fiber_sig(&self, name: &str) -> Option<FiberSig> {
        FiberSig::of(&self.prog, self.prog.return_type(name)?)
    }

    /// The LLVM spelling of `name`'s emitted return slot (Result block or
    /// scalar) — the type its `define`/`call` lines actually carry.
    pub(crate) fn fn_ret_spelling(&self, name: &str) -> String {
        crate::llty::ret_spelling(
            self.fn_ret_ltype(name).unwrap_or(LType::I64),
            self.fn_ret_result_inner(name),
        )
    }

    /// The full heap layout of a constructor: owning type, whether it is a
    /// record, the discriminant tag (variant index within its union; 0 for a
    /// record), and ordered `(field, LType)` pairs.
    pub(crate) fn ctor_layout(&self, name: &str) -> Option<CtorView> {
        let c = self.prog.ctors.get(name)?;
        let tag = i64::try_from(
            self.prog
                .unions
                .get(&c.owner)
                .and_then(|vs| vs.iter().position(|v| v == name))
                .unwrap_or(0),
        )
        .unwrap_or(0);
        let fields = c
            .fields
            .iter()
            .map(|(f, t)| (f.clone(), ParamSig::of(&self.prog, t).ty))
            .collect();
        let mut mf = vec![crate::meta::MetaField::Word]; // leading tag
        mf.extend(c.fields.iter().map(|(_, t)| self.field_meta(t)));
        Some(CtorView {
            owner: c.owner.clone(),
            owner_is_record: c.owner_is_record,
            tag,
            fields,
            meta: crate::meta::struct_meta(&mf),
        })
    }

    /// The layout field for a constructor field of Osprey type `t`. A field
    /// whose type is a DECLARED UNION is `PtrDirect`: union values can only
    /// come from constructors (always `@osp_alloc_tagged`-headered ARC
    /// bodies) — unless an extern claims to return that union, which would
    /// smuggle in a foreign pointer and break the proof. Everything else
    /// (strings can be rodata, records can cross the C ABI, `Ptr` is FFI)
    /// keeps the probe-tolerant `LType` mapping. [GC-ARC-PERCEUS]
    pub(super) fn field_meta(&self, t: &Type) -> crate::meta::MetaField {
        let proven = crate::types::proven_heap_name(t).is_some_and(|n| {
            self.prog.unions.contains_key(n) && !self.extern_ret_types.contains(n)
        });
        if proven && ltype_of(t) == LType::Ptr {
            crate::meta::MetaField::PtrDirect
        } else {
            crate::meta::MetaField::of_lty(ParamSig::of(&self.prog, t).ty)
        }
    }

    /// Record every type name in an extern's declared return type (see
    /// [`Self::extern_ret_types`]). Called from the lowering pre-pass, before
    /// any constructor layout is computed.
    pub(crate) fn poison_extern_ret(&mut self, t: &osprey_ast::TypeExpr) {
        let _ = self.extern_ret_types.insert(t.name.clone());
        let nested = t
            .generic_params
            .iter()
            .chain(t.array_element.as_deref())
            .chain(t.parameter_types.iter())
            .chain(t.return_type.as_deref());
        for inner in nested {
            self.poison_extern_ret(inner);
        }
    }

    /// Resolve a field name to an owning constructor when the target's static
    /// type is unknown — the polymorphic field-access fallback for a generic
    /// accessor like `fn getFirst(p) = p.first`, where `p` infers to a type
    /// variable. Prefers a layout whose field type is a concrete scalar (so the
    /// load type and `toString` match the runtime value), breaking ties by
    /// owner name for deterministic output.
    pub(crate) fn find_field_owner(&self, field: &str) -> Option<String> {
        let mut candidates: Vec<(&String, LType)> = self
            .prog
            .ctors
            .iter()
            .filter_map(|(name, c)| {
                c.fields
                    .iter()
                    .find(|(f, _)| f == field)
                    .map(|(_, t)| (name, ltype_of(t)))
            })
            .collect();
        candidates.sort_by(|a, b| a.0.cmp(b.0));
        candidates
            .iter()
            .find(|(_, lt)| *lt != LType::Ptr)
            .or_else(|| candidates.first())
            .map(|(name, _)| (*name).clone())
    }

    /// The LLVM struct spelling for a constructor's heap block: `{ i64, f0, … }`
    /// — a leading `i64` discriminant tag followed by each field's LLVM type.
    pub(crate) fn ctor_struct_ty(&self, name: &str) -> Option<String> {
        if name == crate::aggregate::HTTP_RESPONSE {
            return Some(crate::aggregate::HTTP_RESPONSE_STRUCT.to_string());
        }
        let view = self.ctor_layout(name)?;
        let mut parts = vec!["i64".to_string()];
        for (_, lt) in &view.fields {
            parts.push(lt.as_str().to_string());
        }
        Some(format!("{{ {} }}", parts.join(", ")))
    }

    /// Whether a name is a known constructor.
    pub(crate) fn is_ctor(&self, name: &str) -> bool {
        self.prog.ctors.contains_key(name)
    }

    /// The owner type name to tag a loaded aggregate field with: the field's
    /// resolved type when that type is a known record/union or a collection
    /// handle, else `None` (scalars carry no owner).
    ///
    /// Collections belong here. A field holding a `List<string>` came back
    /// untagged, so `listGet(record.tags, 0)` read the element as the raw `i64`
    /// storage word and a `?: "none"` default beside it printed `0`
    /// ([`crate::collections::LIST_TAG`]).
    pub(crate) fn ctor_field_owner(&self, owner: &str, field: &str) -> Option<String> {
        // A registered layout carries the owner each slot was BUILT with, which
        // is the only record of it when the declared field type is a type
        // parameter: `Envelope<Map<string, List<string>>, int>.payload` has no
        // nominal owner in `type Envelope<T, U>`, only in the value stored.
        if let Some(fields) = self.obj_layouts.get(owner) {
            return fields
                .iter()
                .find(|(f, _)| f == field)
                .and_then(|(_, value)| value.osp_ty.clone());
        }
        let ty = self.ctor_field_ty(owner, field)?.clone();
        let head = crate::types::owner_name(&self.prog, &ty)?;
        let known = self.prog.ctors.contains_key(&head)
            || self.prog.unions.contains_key(&head)
            || crate::collections::is_list_owner(&head)
            || crate::collections::is_map_owner(&head);
        known.then_some(head)
    }

    /// The variant constructor names of a union owner, in tag order.
    pub(crate) fn union_variants(&self, owner: &str) -> Option<&[String]> {
        self.prog.unions.get(owner).map(Vec::as_slice)
    }
}
