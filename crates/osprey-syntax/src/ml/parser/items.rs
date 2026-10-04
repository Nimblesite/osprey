//! ML items parsing.
use super::{MlItem, MlModuleKind, MlTypeParam, Parser, Stage, TokKind, STATIC_STAGE_KEYWORD};

impl Parser<'_> {
    pub(in crate::ml) fn program(&mut self) -> Vec<MlItem> {
        let mut out = Vec::new();
        loop {
            self.skip_separators();
            if matches!(self.peek(), TokKind::Eof) {
                break;
            }
            match self.item() {
                Some(item) => out.push(item),
                None => self.recover(),
            }
        }
        out
    }

    /// Parse one item, or `None` for a skipped signature line or a recoverable
    /// error.
    pub(in crate::ml) fn item(&mut self) -> Option<MlItem> {
        match self.peek() {
            TokKind::Doc(text) => {
                let text = text.clone();
                self.advance();
                Some(MlItem::Doc(text))
            }
            TokKind::InnerDoc(text) => {
                let text = text.clone();
                let pos = self.pos();
                self.advance();
                Some(MlItem::InnerDoc { text, pos })
            }
            TokKind::KwMut => self.mut_binding(),
            TokKind::KwHandle => {
                // A file-scope `handle` has no block to govern; say so, then
                // consume the region it would have governed so its arms do not
                // cascade into unrelated errors. [EFFECTS-HANDLE-REST]
                self.error(crate::HANDLE_NEEDS_A_BLOCK);
                let _ = self.handle_line();
                None
            }
            TokKind::KwType => self.type_decl(),
            TokKind::KwExtern => self.extern_decl(),
            TokKind::KwEffect => self.effect_decl(Stage::Dynamic),
            TokKind::KwImport => self.import_decl(),
            TokKind::KwNamespace => self.namespace_decl(),
            TokKind::KwModule => self.module_decl(MlModuleKind::Plain),
            TokKind::KwState => self.module_decl(MlModuleKind::State),
            TokKind::KwSignature => self.module_signature_decl(),
            TokKind::KwExport => self.export_decl(),
            TokKind::KwOpaque => self.opaque_decl(),
            TokKind::Reserved(word) => {
                let word = word.clone();
                self.error(format!("ML construct '{word}' is not yet supported"));
                None
            }
            // `static effect E` — `static` is CONTEXTUAL, marking a stage only
            // directly in front of `effect`, so it stays an ordinary identifier
            // everywhere else. Without this the marker was silently dropped and
            // the effect lowered DYNAMIC, which is a wrong answer rather than a
            // diagnostic. Implements [STAGE-DECL], [FLAVOR-ML-EFFECT].
            TokKind::Ident(word)
                if word == STATIC_STAGE_KEYWORD && *self.peek_at(1) == TokKind::KwEffect =>
            {
                self.advance();
                self.effect_decl(Stage::Static)
            }
            TokKind::Ident(_) => self.ident_item(),
            _ => Some(self.expr_item()),
        }
    }

    /// `import target`, optional whole-target alias, and an optional indented
    /// member projection ([MODULES-IMPORT]). ML uses layout instead of Default's
    /// punctuation-heavy `::{...}` member list.
    pub(in crate::ml) fn import_decl(&mut self) -> Option<MlItem> {
        crate::ml::module_parse::import_decl(self)
    }

    /// `namespace name`: an indented body is a block contribution; without one
    /// the declaration applies to subsequent declarations in the file
    /// ([MODULES-FILE-SCOPED-NAMESPACE]).
    pub(in crate::ml) fn namespace_decl(&mut self) -> Option<MlItem> {
        crate::ml::module_parse::namespace_decl(self)
    }

    /// A layout module head. ML deliberately spells a state module `state Name`
    /// rather than the redundant `state module Name` ([MODULES-STATE-MODULE]).
    pub(in crate::ml) fn module_decl(&mut self, kind: MlModuleKind) -> Option<MlItem> {
        crate::ml::module_parse::module_decl(self, kind)
    }

    /// `signature Name` plus its public, export-free interface requirements.
    pub(in crate::ml) fn module_signature_decl(&mut self) -> Option<MlItem> {
        crate::ml::module_parse::signature_decl(self)
    }

    /// Wrap exactly one following declaration in explicit visibility metadata.
    pub(in crate::ml) fn export_decl(&mut self) -> Option<MlItem> {
        crate::ml::module_parse::export_decl(self)
    }

    pub(in crate::ml) fn opaque_decl(&mut self) -> Option<MlItem> {
        crate::ml::module_parse::opaque_decl(self)
    }

    /// `mut name = body` → a mutable binding.
    pub(in crate::ml) fn mut_binding(&mut self) -> Option<MlItem> {
        let pos = self.pos();
        self.advance(); // `mut`
        let name = self.ident()?;
        let _ = self.expect_eq();
        let body = self.body_after_eq();
        Some(MlItem::Binding {
            mutable: true,
            name,
            params: Vec::new(),
            uncurried: false,
            body,
            pos,
        })
    }

    /// Dispatch an identifier-led item: signature (skipped), assignment,
    /// binding/function, or a bare expression.
    pub(in crate::ml) fn ident_item(&mut self) -> Option<MlItem> {
        match self.peek_at(1) {
            TokKind::Colon => self.signature(),
            TokKind::ColonEq => self.assignment(),
            _ if self.at_generic_signature() => self.signature(),
            _ if self.is_binding_head() => self.binding(),
            _ => Some(self.expr_item()),
        }
    }

    /// Whether the current item is a generic signature `name<T, U> : type` —
    /// an identifier, then a `<`-delimited list of parameter names (with
    /// optional `out`/`in` markers), then `:`. Distinguished from a `name < x`
    /// comparison by requiring the whole binder-plus-colon shape before
    /// committing. Implements [FLAVOR-ML-GENERICS].
    pub(in crate::ml) fn at_generic_signature(&self) -> bool {
        if !matches!(self.peek_at(1), TokKind::Op(op) if op == "<") {
            return false;
        }
        let mut j = 2;
        loop {
            // Optional variance marker before each parameter name.
            if matches!(self.peek_at(j), TokKind::KwIn)
                || matches!(self.peek_at(j), TokKind::Ident(n) if n == "out"
                    && matches!(self.peek_at(j + 1), TokKind::Ident(_)))
            {
                j += 1;
            }
            if !matches!(self.peek_at(j), TokKind::Ident(_)) {
                return false;
            }
            j += 1;
            match self.peek_at(j) {
                TokKind::Comma => j += 1,
                TokKind::Op(op) if op == ">" => {
                    return matches!(self.peek_at(j + 1), TokKind::Colon)
                }
                _ => return false,
            }
        }
    }

    /// The `<T, U>` binder of a generic signature. The caller has already
    /// validated the whole shape via [`Self::at_generic_signature`], so this
    /// only consumes: `<`, comma-separated parameter groups, `>`.
    pub(in crate::ml) fn signature_type_params(&mut self) -> Vec<MlTypeParam> {
        if !matches!(self.peek(), TokKind::Op(op) if op == "<") {
            return Vec::new();
        }
        self.advance(); // `<`
        let mut out = self.type_params();
        while self.eat(&TokKind::Comma) {
            out.append(&mut self.type_params());
        }
        if matches!(self.peek(), TokKind::Op(op) if op == ">") {
            self.advance(); // `>`
        }
        out
    }

    /// `name := value` → an assignment.
    pub(in crate::ml) fn assignment(&mut self) -> Option<MlItem> {
        let pos = self.pos();
        let name = self.ident()?;
        self.advance(); // `:=`
        let value = self.body_after_eq();
        Some(MlItem::Assign { name, value, pos })
    }

    /// `name : type` / `name<T, U> : type` → a type signature for the binding
    /// that follows, with an optional trailing effect row `! Ref(, Ref)*` or
    /// `! [Ref, …]` ([FLAVOR-ML-EFFECT], [FLAVOR-ML-GENERICS]).
    pub(in crate::ml) fn signature(&mut self) -> Option<MlItem> {
        let start = self.i;
        let pos = self.pos();
        let name = self.ident()?;
        let type_params = self.signature_type_params();
        // Both dispatch paths — a bare `name :` and `at_generic_signature`'s
        // `name<T, U> :` — have already seen the colon, so it is here by
        // construction and needs no diagnostic of its own.
        let _ = self.eat(&TokKind::Colon);
        let ty = self.ty();
        let (effects, effect_tail, effect_row_present) = self.effect_row();
        if type_params.is_empty() && !effect_row_present {
            crate::ml::annotation_edits::signature(
                self.source,
                self.toks,
                start,
                self.i,
                self.annotations,
            );
        }
        Some(MlItem::ValueSignature {
            name,
            type_params,
            ty,
            effects,
            effect_tail,
            effect_row_present,
            pos,
        })
    }

    /// `name param* = body` → a binding (value when `param*` is empty, function
    /// otherwise). Currying is applied later, in the lowerer; the head form
    /// (juxtaposed `f x y` curried vs parenthesised comma-list `f (x, y)`
    /// uncurried) is recorded in `uncurried` ([FLAVOR-ML-CURRY]).
    pub(in crate::ml) fn binding(&mut self) -> Option<MlItem> {
        let pos = self.pos();
        let name = self.ident()?;
        let (params, uncurried) = self.head_params();
        let _ = self.expect_eq();
        let body = self.body_after_eq();
        Some(MlItem::Binding {
            mutable: false,
            name,
            params,
            uncurried,
            body,
            pos,
        })
    }

    pub(in crate::ml) fn expr_item(&mut self) -> MlItem {
        let pos = self.pos();
        let value = self.expr(0);
        MlItem::Expr { value, pos }
    }
}
