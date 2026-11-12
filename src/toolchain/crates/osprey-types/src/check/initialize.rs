//! Type checking: initialize.
use super::{names, Checker, CtorInfo, HashMap, HashSet, InferCtx, Variance};

impl Checker {
    pub(super) fn new() -> Checker {
        let mut c = Checker {
            ctx: InferCtx::new(),
            errors: Vec::new(),
            ctors: HashMap::new(),
            effects: crate::builtins::builtin_effects(),
            signal_queries: HashMap::new(),
            function_effects: HashMap::new(),
            library_exports: crate::effect_rows::LibraryExports::default(),
            deferred_gpu_kernel: false,
            expression_types: Some(HashMap::new()),
            constants: Vec::new(),
            union_variants: HashMap::new(),
            fn_params: crate::staging::signal_parameter_names(),
            fn_sigs: HashMap::new(),
            handler_factories: HashSet::new(),
            lambda_tys: Vec::new(),
            binder_tys: crate::binders::Binders::default(),
            let_tys: Vec::new(),
            list_tys: Vec::new(),
            instantiations: HashMap::new(),
            methods: HashMap::new(),
            application_tys: Vec::new(),
            builtin_uses: Vec::new(),
            field_uses_prefix: None,
            opaque_types: HashSet::new(),
            site: String::new(),
            use_sites: HashMap::new(),
            scheme_obligations: HashMap::new(),
            discards: Vec::new(),
            builtins: HashSet::new(),
            source_contracts_validated: false,
            resume_ctx: Vec::new(),
            handler_scopes: Vec::new(),
            perform_tys: Vec::new(),
            perform_actual_tys: Vec::new(),
            handler_tys: Vec::new(),
            capability_sites: Vec::new(),
            fn_typarams: HashMap::new(),
            declared_effect_rows: HashMap::new(),
            current_fn_typarams: HashMap::new(),
            current_fn: None,
            defer_arith: true,
            expr_position: None,
        };
        c.register_result_ctors();
        c.register_builtin_variances();
        // Burn the ids the builtin schemes hand-write as quantified binders so
        // no live inference variable can collide with them (they stay
        // permanently unbound). See `builtins::RESERVED_SCHEME_VARS`.
        for _ in 0..crate::builtins::RESERVED_SCHEME_VARS {
            let _ = c.ctx.fresh();
        }
        c
    }

    /// Built-in constructors' declared variance: producers are covariant in
    /// what they produce; `Map` keys are looked up (invariant) while values
    /// only flow out. Implements [TYPE-VARIANCE-ASSIGN].
    pub(super) fn register_builtin_variances(&mut self) {
        self.ctx.set_variance(
            names::RESULT,
            vec![Variance::Covariant, Variance::Covariant],
        );
        self.ctx
            .set_variance(names::LIST, vec![Variance::Covariant]);
        self.ctx
            .set_variance(names::FIBER, vec![Variance::Covariant]);
        self.ctx
            .set_variance(names::MAP, vec![Variance::Invariant, Variance::Covariant]);
    }

    /// One `Result<T, E>` constructor: a variant of `Result` over both its
    /// parameters, carrying the single field `field`.
    fn insert_result_ctor(&mut self, name: &str, field: (&str, &str)) {
        let _ = self.ctors.insert(
            name.into(),
            CtorInfo {
                owner: names::RESULT.into(),
                owner_is_record: false,
                type_params: vec!["T".into(), "E".into()],
                fields: vec![(field.0.into(), field.1.into())],
            },
        );
    }

    /// Built-in `Result` constructors `Success { value: T }` / `Error { message: E }`.
    pub(super) fn register_result_ctors(&mut self) {
        self.insert_result_ctor(names::SUCCESS, ("value", "T"));
        // `Error { message: <string> }` builds the E side of a `Result<T, E>`;
        // the message is a concrete string, leaving E free to unify with the
        // declared error type (e.g. the nominal `Error`), not pinned to string.
        self.insert_result_ctor(names::ERROR, ("message", "string"));
        let _ = self.union_variants.insert(
            names::RESULT.into(),
            vec![names::SUCCESS.into(), names::ERROR.into()],
        );
        // Built-in HttpResponse record returned by HTTP request handlers.
        let _ = self.ctors.insert(
            "HttpResponse".into(),
            CtorInfo {
                owner: "HttpResponse".into(),
                owner_is_record: true,
                type_params: Vec::new(),
                // Field set + order match `struct HttpResponse` in
                // `runtime/http_shared.h` exactly — the C HTTP runtime reads the
                // handler's returned struct by this layout.
                fields: vec![
                    ("status".into(), "int".into()),
                    ("headers".into(), "string".into()),
                    ("contentType".into(), "string".into()),
                    ("streamFd".into(), "int".into()),
                    ("isComplete".into(), "bool".into()),
                    ("partialBody".into(), "string".into()),
                ],
            },
        );
    }
}
