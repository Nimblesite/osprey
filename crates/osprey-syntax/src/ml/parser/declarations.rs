//! ML declarations parsing.
use super::{
    is_constructor, MlExternParam, MlItem, MlTypeField, MlTypeParam, MlVariance, MlVariant, Parser,
    Position, TokKind,
};

impl Parser<'_> {
    /// `type Name param* =` + an indented block of variants ([FLAVOR-ML-TYPE]).
    /// A union/enum lists uppercase constructor lines (each with an optional
    /// nested `field : type` block); a record is the single-variant form whose
    /// first block line is a lowercase `field : type`, in which case the lone
    /// variant takes the type's own name (matching the Default record shape).
    pub(in crate::ml) fn type_decl(&mut self) -> Option<MlItem> {
        let pos = self.pos();
        self.advance(); // `type`
        self.type_decl_after_keyword(pos)
    }

    /// Finish a type declaration after its `type` token has already been
    /// consumed (also reused by `opaque type`).
    pub(in crate::ml) fn type_decl_after_keyword(&mut self, pos: Position) -> Option<MlItem> {
        let name = self.ident()?;
        let type_params = self.type_params();
        let _ = self.expect_eq();
        let (variants, alias) = match self.peek() {
            TokKind::Indent => (self.type_body(&name), None),
            TokKind::Newline | TokKind::Dedent | TokKind::Eof => (Vec::new(), None),
            // An uppercase head commits to the inline union form; a lowercase
            // one stays a manifest alias (`type UserId = int`)
            // ([FLAVOR-ML-UNION-INLINE]).
            TokKind::Ident(head) if is_constructor(head) => (self.inline_union(), None),
            _ => (Vec::new(), Some(self.ty())),
        };
        Some(MlItem::Type {
            name,
            type_params,
            variants,
            alias,
            pos,
        })
    }

    /// Type parameters between a declaration's name and its body (e.g. `T` in
    /// `type Box T = …`), in order, each with an optional variance marker:
    /// `out T` (covariant) / `in T` (contravariant). `out` and `in` are
    /// contextual keywords reserved inside type-parameter position in BOTH
    /// flavors — a marker must be followed by a parameter name. Implements
    /// [TYPE-VARIANCE-DECL].
    pub(in crate::ml) fn type_params(&mut self) -> Vec<MlTypeParam> {
        let mut out = Vec::new();
        loop {
            let variance = match self.peek() {
                TokKind::Ident(name) if name == "out" => Some(MlVariance::Covariant),
                TokKind::KwIn => Some(MlVariance::Contravariant),
                _ => None,
            };
            match (variance, self.peek()) {
                (Some(variance), _) => {
                    let marker = if variance == MlVariance::Covariant {
                        "out"
                    } else {
                        "in"
                    };
                    self.advance(); // the marker
                    if let Some(name) = self.ident() {
                        out.push(MlTypeParam { name, variance });
                    } else {
                        self.error(format!("expected a type parameter name after '{marker}'"));
                        break;
                    }
                }
                (None, TokKind::Ident(name)) => {
                    let name = name.clone();
                    self.advance();
                    out.push(MlTypeParam {
                        name,
                        variance: MlVariance::Invariant,
                    });
                }
                _ => break,
            }
        }
        out
    }

    /// The indented body of a `type`. If the first non-blank line is a lowercase
    /// `field : type`, the whole block is one record variant named after the
    /// type; otherwise each uppercase line is a union/enum constructor variant.
    pub(in crate::ml) fn type_body(&mut self, type_name: &str) -> Vec<MlVariant> {
        if !self.eat(&TokKind::Indent) {
            return Vec::new();
        }
        self.skip_separators();
        let variants = if self.at_record_field() {
            let fields = self.type_fields();
            vec![MlVariant {
                name: type_name.to_owned(),
                fields,
            }]
        } else {
            self.union_variants()
        };
        let _ = self.eat(&TokKind::Dedent);
        variants
    }

    /// Whether the current block line is a record field `name : type` (a
    /// lowercase identifier directly followed by `:`), versus a constructor line.
    pub(in crate::ml) fn at_record_field(&self) -> bool {
        matches!(self.peek(), TokKind::Ident(name) if !is_constructor(name))
            && matches!(self.peek_at(1), TokKind::Colon)
    }

    /// The uppercase constructor variants of a union/enum, each optionally
    /// followed by an indented `field : type` payload block.
    pub(in crate::ml) fn union_variants(&mut self) -> Vec<MlVariant> {
        let mut variants = Vec::new();
        while !self.at_block_end() {
            self.skip_separators();
            if self.at_block_end() {
                break;
            }
            let before = self.i;
            match self.ident() {
                Some(name) => {
                    let fields = if matches!(self.peek(), TokKind::Indent) {
                        self.advance(); // `Indent`
                        let fields = self.type_fields();
                        let _ = self.eat(&TokKind::Dedent);
                        fields
                    } else {
                        Vec::new()
                    };
                    variants.push(MlVariant { name, fields });
                }
                None => self.recover(),
            }
            if self.i == before {
                self.recover();
            }
        }
        variants
    }

    /// A run of `field : type` lines (a variant payload or a record body).
    pub(in crate::ml) fn type_fields(&mut self) -> Vec<MlTypeField> {
        let mut fields = Vec::new();
        while !self.at_block_end() {
            self.skip_separators();
            if self.at_block_end() {
                break;
            }
            let before = self.i;
            match self.type_field() {
                Some(field) => fields.push(field),
                None => self.recover(),
            }
            if self.i == before {
                self.recover();
            }
        }
        fields
    }

    /// One `field : type` declaration, shared by the layout block and the
    /// inline parenthesised payload so neither restates the rule.
    pub(in crate::ml) fn type_field(&mut self) -> Option<MlTypeField> {
        let name = self.ident()?;
        if !self.eat(&TokKind::Colon) {
            self.error("expected ':' in type field");
        }
        Some(MlTypeField {
            name,
            ty: self.ty(),
        })
    }

    /// `variant ("|" variant)*` written on the declaration line — the inline
    /// union form ([FLAVOR-ML-UNION-INLINE]). The layout form remains available
    /// for declarations too wide to read on one line.
    pub(in crate::ml) fn inline_union(&mut self) -> Vec<MlVariant> {
        let mut variants = Vec::new();
        loop {
            let before = self.i;
            match self.inline_variant() {
                Some(variant) => variants.push(variant),
                None => self.recover(),
            }
            if self.i == before {
                self.recover();
                break;
            }
            if !self.eat(&TokKind::Pipe) {
                break;
            }
        }
        variants
    }

    /// One inline variant: `Ctor` alone, `Ctor typeAtom*` (positional payload),
    /// or `Ctor(field : type, …)` (named payload).
    pub(in crate::ml) fn inline_variant(&mut self) -> Option<MlVariant> {
        let name = self.ident()?;
        if !is_constructor(&name) {
            self.error(format!(
                "union variant '{name}' must start with an uppercase letter"
            ));
            return None;
        }
        let fields = if self.at_named_payload() {
            self.named_payload()
        } else {
            self.positional_payload()
        };
        Some(MlVariant { name, fields })
    }

    /// Whether the `(` here opens a named payload `(field : type, …)` rather
    /// than a parenthesised positional payload type such as `(List int)`.
    pub(in crate::ml) fn at_named_payload(&self) -> bool {
        matches!(self.peek(), TokKind::LParen)
            && matches!(self.peek_at(1), TokKind::Ident(field) if !is_constructor(field))
            && matches!(self.peek_at(2), TokKind::Colon)
    }

    /// `( field : type (, field : type)* )` — the inline named payload.
    pub(in crate::ml) fn named_payload(&mut self) -> Vec<MlTypeField> {
        self.advance(); // `(`
        let mut fields = Vec::new();
        while let Some(field) = self.type_field() {
            fields.push(field);
            if !self.eat(&TokKind::Comma) {
                break;
            }
        }
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        fields
    }

    /// `typeAtom*` after a variant name — a positional payload. Slots carry
    /// generated index names because they have no source spelling
    /// ([TYPE-UNION-POSITIONAL]).
    pub(in crate::ml) fn positional_payload(&mut self) -> Vec<MlTypeField> {
        let mut fields = Vec::new();
        while self.starts_ty_atom() {
            fields.push(MlTypeField {
                name: osprey_ast::positional_field_name(fields.len()),
                ty: self.ty_atom(),
            });
        }
        fields
    }

    /// `extern name (pname : ptype)* -> rettype` — an external (FFI) function
    /// declaration ([FLAVOR-ML-EXTERN]). Each parameter is a parenthesised
    /// `name : type`; an optional trailing `-> type` gives the return type.
    pub(in crate::ml) fn extern_decl(&mut self) -> Option<MlItem> {
        let pos = self.pos();
        self.advance(); // `extern`
        let name = self.ident()?;
        let mut params = Vec::new();
        while matches!(self.peek(), TokKind::LParen) {
            if let Some(param) = self.extern_param() {
                params.push(param);
            }
        }
        let return_type = if self.eat(&TokKind::Arrow) {
            Some(self.ty())
        } else {
            None
        };
        Some(MlItem::Extern {
            name,
            params,
            return_type,
            pos,
        })
    }

    /// One `( name : type )` parameter of an `extern` declaration.
    pub(in crate::ml) fn extern_param(&mut self) -> Option<MlExternParam> {
        self.advance(); // `(`
        let name = self.ident()?;
        if !self.eat(&TokKind::Colon) {
            self.error("expected ':' in extern parameter");
        }
        let ty = self.ty();
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        Some(MlExternParam { name, ty })
    }
}
