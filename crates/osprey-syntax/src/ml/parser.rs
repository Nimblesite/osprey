//! The ML-flavor parser: a hand-written **recursive-descent** parser with a
//! **Pratt / precedence-climbing** expression core, run over the layout-resolved
//! token stream from [`super::lexer`]. It produces the ML **concrete syntax
//! tree** ([`super::cst`]) and nothing else — every canonicalisation (currying,
//! pipe desugaring, record/block normalisation, string interpolation) is the
//! lowerer's job ([`super::lower`]). This keeps a clean parse/lower seam: the
//! parser decides *what was written*, the lowerer decides *what it means*
//! ([FLAVOR-FRONTEND], docs/specs/0023-LanguageFlavors.md).
//!
//! ## Design, and the authorities it follows
//!
//! The expression grammar is parsed by binding powers in one driving loop
//! ([`Parser::expr`]) rather than one routine per precedence level. This is
//! Pratt's *top-down operator precedence*; precedence climbing is the same
//! algorithm phrased with explicit minimum-binding-power, so the two names
//! describe one technique. The statement grammar is straight predictive
//! recursive descent. Layout (`Indent`/`Dedent`/`Newline`) is the offside rule,
//! resolved in the lexer and consumed here as ordinary tokens.
//!
//! References (verified 2026-06-30):
//! - V. R. Pratt, "Top Down Operator Precedence", POPL 1973, pp. 41–51.
//!   DOI <https://doi.org/10.1145/512927.512931>. The origin of binding-power
//!   expression parsing used by [`Parser::expr`].
//! - T. Norvell, "Parsing Expressions by Recursive Descent", Memorial Univ.,
//!   1999. <https://www.engr.mun.ca/~theo/Misc/exp_parsing.htm>. Establishes
//!   precedence climbing (origin: M. Richards / K. Clarke) and that it "is a
//!   special case of … Pratt parsing".
//! - A. V. Aho, M. S. Lam, R. Sethi, J. D. Ullman, *Compilers: Principles,
//!   Techniques, and Tools*, 2nd ed., 2006, ISBN 978-0-321-48681-3, ch. 4 §4.4
//!   (recursive-descent / predictive parsing) and §4.1.3–4.1.4 (error recovery:
//!   panic-mode, used by [`Parser::recover`]).
//! - P. J. Landin, "The Next 700 Programming Languages", CACM 9(3), 1966,
//!   pp. 157–166. DOI <https://doi.org/10.1145/365230.365257>. Origin of the
//!   offside rule the layout lexer implements ([FLAVOR-ML-LAYOUT]).
//! - *Haskell 2010 Report*, ch. 10 §10.3 "Layout".
//!   <https://www.haskell.org/onlinereport/haskell2010/haskellch10.html>. A
//!   concrete authoritative spec of layout-driven token insertion.

use super::cst::{
    MlArm, MlEffectOp, MlEffectRef, MlExpr, MlExternParam, MlField, MlHandleArm, MlItem,
    MlModuleKind, MlParam, MlPattern, MlSymbolPath, MlType, MlTypeField, MlTypeParam, MlVariance,
    MlVariant,
};
use super::lexer::lex;
use super::token::{TokKind, Token};
use crate::SyntaxError;
use osprey_ast::Position;

/// Parse ML-flavor `source` into the ML CST plus any syntax errors. Best-effort:
/// errors never abort the parse ([FLAVOR-LOWER-CONTRACT]).
pub(crate) fn parse(source: &str) -> (Vec<MlItem>, Vec<SyntaxError>) {
    let (tokens, mut errors) = lex(source);
    let items = {
        let mut parser = Parser {
            toks: &tokens,
            i: 0,
            errors: &mut errors,
        };
        parser.program()
    };
    super::modules::validate(&items, &mut errors);
    (items, errors)
}

/// The `-` operator lexeme — used as both a binary subtraction operator and the
/// prefix sign of a negative literal (including in patterns, where `-N` folds
/// into a negated integer literal).
const MINUS_OP: &str = "-";

/// The Result-default operator, spelled the same in both flavors
/// ([PATTERN-RESULT-DEFAULT]). Right-associative and binding below everything
/// else, so `1 + 2 ?: 0` groups as `(1 + 2) ?: 0`.
pub(super) const ELVIS_OP: &str = "?:";

/// Binding powers, mirroring the Default grammar's precedence table so equal
/// programs in either flavor produce the same canonical AST (higher binds
/// tighter): default < or < and < compare < add < mul < pipe. Application
/// (whitespace) and prefix unary bind tighter still and are handled
/// structurally.
fn infix_bp(op: &str) -> Option<u8> {
    let bp = match op {
        ELVIS_OP => 1,
        "||" => 2,
        "&&" => 3,
        "==" | "!=" | "<" | ">" | "<=" | ">=" => 4,
        "+" | "-" => 5,
        "*" | "/" | "%" => 6,
        "|>" => 8,
        _ => return None,
    };
    Some(bp)
}

/// Recursive-descent + Pratt parser over the layout-resolved token slice.
pub(super) struct Parser<'t> {
    toks: &'t [Token],
    i: usize,
    errors: &'t mut Vec<SyntaxError>,
}

impl Parser<'_> {
    pub(super) fn peek(&self) -> &TokKind {
        self.toks.get(self.i).map_or(&TokKind::Eof, |t| &t.kind)
    }

    fn peek_at(&self, ahead: usize) -> &TokKind {
        self.toks
            .get(self.i + ahead)
            .map_or(&TokKind::Eof, |t| &t.kind)
    }

    pub(super) fn pos(&self) -> Position {
        self.toks.get(self.i).map_or(Position::default(), |t| t.pos)
    }

    /// Consume the current token, discarding it (callers peek first when they
    /// need its payload).
    pub(super) fn advance(&mut self) {
        if self.i < self.toks.len() {
            self.i += 1;
        }
    }

    pub(super) fn eat(&mut self, kind: &TokKind) -> bool {
        if self.peek() == kind {
            self.i += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn error(&mut self, message: impl Into<String>) {
        let position = self.pos();
        self.error_at(position, message);
    }

    pub(super) fn error_at(&mut self, position: Position, message: impl Into<String>) {
        self.errors.push(SyntaxError {
            message: message.into(),
            position,
        });
    }

    /// Panic-mode recovery (Dragon Book §4.1.4): drop tokens up to the next
    /// statement separator so one bad line cannot derail the rest.
    pub(super) fn recover(&mut self) {
        while !matches!(
            self.peek(),
            TokKind::Newline | TokKind::Dedent | TokKind::Eof
        ) {
            self.i += 1;
        }
    }

    pub(super) fn skip_separators(&mut self) {
        while matches!(self.peek(), TokKind::Newline) {
            self.i += 1;
        }
    }

    pub(super) fn at_block_end(&self) -> bool {
        matches!(self.peek(), TokKind::Dedent | TokKind::Eof)
    }

    // --- statements -------------------------------------------------------

    fn program(&mut self) -> Vec<MlItem> {
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
    pub(super) fn item(&mut self) -> Option<MlItem> {
        match self.peek() {
            TokKind::Doc(text) => {
                let text = text.clone();
                self.advance();
                Some(MlItem::Doc(text))
            }
            TokKind::KwMut => self.mut_binding(),
            TokKind::KwType => self.type_decl(),
            TokKind::KwExtern => self.extern_decl(),
            TokKind::KwEffect => self.effect_decl(),
            TokKind::KwImport => self.import_decl(),
            TokKind::KwNamespace => self.namespace_decl(),
            TokKind::KwModule => self.module_decl(MlModuleKind::Plain),
            TokKind::KwState => self.module_decl(MlModuleKind::State),
            TokKind::KwSignature => self.module_signature_decl(),
            TokKind::KwExport => self.export_decl(),
            TokKind::KwOpaque => self.opaque_decl(),
            TokKind::Reserved(word) => {
                let word = word.clone();
                self.error(format!(
                    "ML construct '{word}' is not yet supported (plan 0013); \
                     use the Default flavor for now"
                ));
                None
            }
            TokKind::Ident(_) => self.ident_item(),
            _ => Some(self.expr_item()),
        }
    }

    /// `import target`, optional whole-target alias, and an optional indented
    /// member projection ([MODULES-IMPORT]). ML uses layout instead of Default's
    /// punctuation-heavy `::{...}` member list.
    fn import_decl(&mut self) -> Option<MlItem> {
        super::module_parse::import_decl(self)
    }

    /// `namespace name`: an indented body is a block contribution; without one
    /// the declaration applies to subsequent declarations in the file
    /// ([MODULES-FILE-SCOPED-NAMESPACE]).
    fn namespace_decl(&mut self) -> Option<MlItem> {
        super::module_parse::namespace_decl(self)
    }

    /// A layout module head. ML deliberately spells a state module `state Name`
    /// rather than the redundant `state module Name` ([MODULES-STATE-MODULE]).
    fn module_decl(&mut self, kind: MlModuleKind) -> Option<MlItem> {
        super::module_parse::module_decl(self, kind)
    }

    /// `signature Name` plus its public, export-free interface requirements.
    fn module_signature_decl(&mut self) -> Option<MlItem> {
        super::module_parse::signature_decl(self)
    }

    /// Wrap exactly one following declaration in explicit visibility metadata.
    fn export_decl(&mut self) -> Option<MlItem> {
        super::module_parse::export_decl(self)
    }

    fn opaque_decl(&mut self) -> Option<MlItem> {
        super::module_parse::opaque_decl(self)
    }

    /// `mut name = body` → a mutable binding.
    fn mut_binding(&mut self) -> Option<MlItem> {
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

    /// `type Name param* =` + an indented block of variants ([FLAVOR-ML-TYPE]).
    /// A union/enum lists uppercase constructor lines (each with an optional
    /// nested `field : type` block); a record is the single-variant form whose
    /// first block line is a lowercase `field : type`, in which case the lone
    /// variant takes the type's own name (matching the Default record shape).
    fn type_decl(&mut self) -> Option<MlItem> {
        let pos = self.pos();
        self.advance(); // `type`
        self.type_decl_after_keyword(pos)
    }

    /// Finish a type declaration after its `type` token has already been
    /// consumed (also reused by `opaque type`).
    pub(super) fn type_decl_after_keyword(&mut self, pos: Position) -> Option<MlItem> {
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
    pub(super) fn type_params(&mut self) -> Vec<MlTypeParam> {
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
    fn type_body(&mut self, type_name: &str) -> Vec<MlVariant> {
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
    fn at_record_field(&self) -> bool {
        matches!(self.peek(), TokKind::Ident(name) if !is_constructor(name))
            && matches!(self.peek_at(1), TokKind::Colon)
    }

    /// The uppercase constructor variants of a union/enum, each optionally
    /// followed by an indented `field : type` payload block.
    fn union_variants(&mut self) -> Vec<MlVariant> {
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
    fn type_fields(&mut self) -> Vec<MlTypeField> {
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
    fn type_field(&mut self) -> Option<MlTypeField> {
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
    fn inline_union(&mut self) -> Vec<MlVariant> {
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
    fn inline_variant(&mut self) -> Option<MlVariant> {
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
    fn at_named_payload(&self) -> bool {
        matches!(self.peek(), TokKind::LParen)
            && matches!(self.peek_at(1), TokKind::Ident(field) if !is_constructor(field))
            && matches!(self.peek_at(2), TokKind::Colon)
    }

    /// `( field : type (, field : type)* )` — the inline named payload.
    fn named_payload(&mut self) -> Vec<MlTypeField> {
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
    fn positional_payload(&mut self) -> Vec<MlTypeField> {
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
    fn extern_decl(&mut self) -> Option<MlItem> {
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
    fn extern_param(&mut self) -> Option<MlExternParam> {
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

    /// `effect Name` + an indented block of `op : P => R` operation lines — an
    /// algebraic effect declaration ([FLAVOR-ML-EFFECT]). Mirrors [`Self::type_decl`]'s
    /// layout-block parsing.
    fn effect_decl(&mut self) -> Option<MlItem> {
        let pos = self.pos();
        self.advance(); // `effect`
        let name = self.ident()?;
        // `effect State T` — type parameters between the name and the
        // operation block. Implements [EFFECTS-GENERIC-DECL].
        let type_params = self.type_params();
        let operations = self.effect_operations();
        Some(MlItem::Effect {
            name,
            type_params,
            operations,
            pos,
        })
    }

    /// The indented `op : P => R` operation lines of an `effect` block.
    pub(super) fn effect_operations(&mut self) -> Vec<MlEffectOp> {
        let mut operations = Vec::new();
        if !self.eat(&TokKind::Indent) {
            return operations;
        }
        while !self.at_block_end() {
            self.skip_separators();
            if self.at_block_end() {
                break;
            }
            let before = self.i;
            match self.effect_op() {
                Some(op) => operations.push(op),
                None => self.recover(),
            }
            if self.i == before {
                self.recover();
            }
        }
        let _ = self.eat(&TokKind::Dedent);
        operations
    }

    /// One `op : payload => result` operation line.
    fn effect_op(&mut self) -> Option<MlEffectOp> {
        let name = self.ident()?;
        if !self.eat(&TokKind::Colon) {
            self.error("expected ':' in effect operation");
        }
        let payload = self.ty();
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in effect operation");
        }
        let result = self.ty();
        Some(MlEffectOp {
            name,
            payload,
            result,
        })
    }

    /// Dispatch an identifier-led item: signature (skipped), assignment,
    /// binding/function, or a bare expression.
    fn ident_item(&mut self) -> Option<MlItem> {
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
    fn at_generic_signature(&self) -> bool {
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
    pub(super) fn signature_type_params(&mut self) -> Vec<MlTypeParam> {
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
    fn assignment(&mut self) -> Option<MlItem> {
        let pos = self.pos();
        let name = self.ident()?;
        self.advance(); // `:=`
        let value = self.body_after_eq();
        Some(MlItem::Assign { name, value, pos })
    }

    /// `name : type` / `name<T, U> : type` → a type signature for the binding
    /// that follows, with an optional trailing effect row `! Ref(, Ref)*` or
    /// `! [Ref, …]` ([FLAVOR-ML-EFFECT], [FLAVOR-ML-GENERICS]).
    fn signature(&mut self) -> Option<MlItem> {
        let pos = self.pos();
        let name = self.ident()?;
        let type_params = self.signature_type_params();
        if !self.eat(&TokKind::Colon) {
            self.error("expected ':' in signature");
        }
        let ty = self.ty();
        let effects = self.effect_row();
        Some(MlItem::ValueSignature {
            name,
            type_params,
            ty,
            effects,
            pos,
        })
    }

    /// An optional effect row after a signature's type: `! Ref(, Ref)*` or the
    /// bracketed `! [Ref, …]`, each reference optionally applied to type
    /// arguments (`State<int>`). Empty when no `!` is present
    /// ([FLAVOR-ML-EFFECT], [EFFECTS-GENERIC-ROWS]).
    pub(super) fn effect_row(&mut self) -> Vec<MlEffectRef> {
        if !matches!(self.peek(), TokKind::Op(op) if op == "!") {
            return Vec::new();
        }
        self.advance(); // `!`
        let bracketed = self.eat(&TokKind::LBracket);
        let mut effects = Vec::new();
        if let Some(r) = self.effect_ref() {
            effects.push(r);
            while self.eat(&TokKind::Comma) {
                if let Some(r) = self.effect_ref() {
                    effects.push(r);
                }
            }
        }
        if bracketed && !self.eat(&TokKind::RBracket) {
            self.error("expected ']' to close effect row");
        }
        effects
    }

    /// One effect reference in an effect row: a name plus optional
    /// angle-bracketed type arguments.
    fn effect_ref(&mut self) -> Option<MlEffectRef> {
        let pos = self.pos();
        let first = self.ident()?;
        let name = self.qualified_name_tail(first);
        let args = if self.at_angle_open() {
            match self.ty_generic_args(name.clone()) {
                MlType::App { args, .. } => args,
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        };
        Some(MlEffectRef { name, args, pos })
    }

    /// A type: arrows are right-associative (`a -> b -> c` = `a -> (b -> c)`).
    pub(super) fn ty(&mut self) -> MlType {
        let from = self.ty_app();
        if self.eat(&TokKind::Arrow) {
            return MlType::Arrow {
                from: Box::new(from),
                to: Box::new(self.ty()),
            };
        }
        from
    }

    /// Type application `head arg…` — a head name applied to atom types.
    fn ty_app(&mut self) -> MlType {
        let head = self.ty_atom();
        let mut args = Vec::new();
        while self.starts_ty_atom() {
            args.push(self.ty_atom());
        }
        match head {
            MlType::Name(head) if !args.is_empty() => MlType::App { head, args },
            head => head,
        }
    }

    fn starts_ty_atom(&self) -> bool {
        matches!(self.peek(), TokKind::Ident(_) | TokKind::LParen)
    }

    /// A type atom: a name (optionally with `<…>` generic arguments), or a
    /// parenthesised group / tuple.
    fn ty_atom(&mut self) -> MlType {
        match self.peek().clone() {
            TokKind::Ident(name) => {
                self.advance();
                let name = self.qualified_name_tail(name);
                if self.at_angle_open() {
                    self.ty_generic_args(name)
                } else {
                    MlType::Name(name)
                }
            }
            TokKind::LParen => self.ty_paren(),
            other => {
                self.error(format!("unexpected token {other:?} in type"));
                MlType::Name("Unit".to_owned())
            }
        }
    }

    /// Whether the current token opens a generic argument list (`<`).
    fn at_angle_open(&self) -> bool {
        matches!(self.peek(), TokKind::Op(op) if op == "<")
    }

    /// `Head< t (, t)* >` — angle-bracketed generic arguments, lowered to the
    /// same [`MlType::App`] as the whitespace `Head t…` form so both render to
    /// `Head<…>` ([FLAVOR-ML-FN]). Reuses [`Self::ty`] for each argument.
    fn ty_generic_args(&mut self, head: String) -> MlType {
        self.advance(); // `<`
        let mut args = vec![self.ty()];
        while self.eat(&TokKind::Comma) {
            args.push(self.ty());
        }
        if matches!(self.peek(), TokKind::Op(op) if op == ">") {
            self.advance(); // `>`
        } else {
            self.error("expected '>' to close generic arguments");
        }
        MlType::App { head, args }
    }

    /// `( t )` grouping or `( t, t, … )` a tupled argument.
    fn ty_paren(&mut self) -> MlType {
        self.advance(); // `(`
        let mut parts = vec![self.ty()];
        while self.eat(&TokKind::Comma) {
            parts.push(self.ty());
        }
        let _ = self.eat(&TokKind::RParen);
        if parts.len() == 1 {
            parts
                .into_iter()
                .next()
                .unwrap_or(MlType::Name("Unit".to_owned()))
        } else {
            MlType::Tuple(parts)
        }
    }

    /// `name param* = body` → a binding (value when `param*` is empty, function
    /// otherwise). Currying is applied later, in the lowerer; the head form
    /// (juxtaposed `f x y` curried vs parenthesised comma-list `f (x, y)`
    /// uncurried) is recorded in `uncurried` ([FLAVOR-ML-CURRY]).
    fn binding(&mut self) -> Option<MlItem> {
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

    fn expr_item(&mut self) -> MlItem {
        let pos = self.pos();
        let value = self.expr(0);
        MlItem::Expr { value, pos }
    }

    /// The parameter list of a binding or lambda head, plus whether it was the
    /// **uncurried** parenthesised comma-list form `(x, y)` (→ a flat
    /// multi-parameter function/lambda) rather than the juxtaposed curried form
    /// `x y` (→ a nested-lambda chain) ([FLAVOR-ML-CURRY]). The uncurried form is
    /// a single parenthesised group holding a top-level comma; everything else
    /// (juxtaposed names, a lone `(x)` / `(x : t)` / `()`) is curried.
    fn head_params(&mut self) -> (Vec<MlParam>, bool) {
        if matches!(self.peek(), TokKind::LParen) && self.first_paren_has_comma() {
            (self.uncurried_params(), true)
        } else {
            (self.params(), false)
        }
    }

    /// Collect zero or more juxtaposed surface parameter patterns up to the
    /// `=`/`=>` — the curried head form.
    fn params(&mut self) -> Vec<MlParam> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                // An uppercase head in parameter position is a nullary
                // constructor pattern, not a binder ([FLAVOR-ML-CLAUSES]).
                TokKind::Ident(name) if is_constructor(name) => {
                    out.push(MlParam::Pattern(self.pattern()));
                }
                TokKind::Ident(name) => {
                    let name = name.clone();
                    self.advance();
                    out.push(MlParam::Named(name));
                }
                TokKind::LParen if self.at_pattern_param() => {
                    out.push(MlParam::Pattern(self.pattern()));
                }
                TokKind::LParen => out.push(self.paren_param()),
                TokKind::Int(_) | TokKind::Str(_) | TokKind::KwTrue | TokKind::KwFalse => {
                    out.push(MlParam::Pattern(self.pattern()));
                }
                TokKind::Op(op) if op == MINUS_OP && matches!(self.peek_at(1), TokKind::Int(_)) => {
                    out.push(MlParam::Pattern(self.pattern()));
                }
                _ => break,
            }
        }
        out
    }

    /// Whether the `(` here opens a grouped clause pattern (`(Node l r)`,
    /// `(-1)`) rather than a parameter binder (`(x)`, `(x : int)`, `()`).
    fn at_pattern_param(&self) -> bool {
        match self.peek_at(1) {
            TokKind::Ident(name) => is_constructor(name),
            TokKind::Int(_) | TokKind::Str(_) | TokKind::KwTrue | TokKind::KwFalse => true,
            TokKind::Op(op) => op == MINUS_OP,
            _ => false,
        }
    }

    /// `( p ( , p )* )` — the parenthesised comma-list parameters of the
    /// uncurried head form ([FLAVOR-ML-CURRY]).
    fn uncurried_params(&mut self) -> Vec<MlParam> {
        self.advance(); // `(`
        let mut out = Vec::new();
        if !matches!(self.peek(), TokKind::RParen) {
            loop {
                out.push(self.one_param());
                if !self.eat(&TokKind::Comma) {
                    break;
                }
                if matches!(self.peek(), TokKind::RParen) {
                    break; // tolerate a trailing comma
                }
            }
        }
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        out
    }

    /// A parenthesised parameter: `()` (the unit marker), `(name)`, or the inline
    /// type-annotated `(name : type)` a lambda uses for a load-bearing parameter
    /// type ([FLAVOR-ML-FN]).
    fn paren_param(&mut self) -> MlParam {
        self.advance(); // `(`
        let param = self.one_param();
        let _ = self.eat(&TokKind::RParen);
        param
    }

    /// One parameter inside a `(…)` group: a named `name`, a type-annotated
    /// `name : type`, or the unit marker (no name). Shared by the lone `(x)` and
    /// the comma-list `(x, y)` forms so neither duplicates the rule.
    fn one_param(&mut self) -> MlParam {
        match self.peek() {
            TokKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                if self.eat(&TokKind::Colon) {
                    MlParam::Typed(name, self.ty())
                } else {
                    MlParam::Named(name)
                }
            }
            _ => MlParam::Unit,
        }
    }

    /// Non-consuming: does the parenthesised group opening at the current `(`
    /// hold a top-level comma before its matching `)`? Distinguishes the
    /// uncurried comma-list `(x, y)` from grouping `(x)` and the unit `()`.
    fn first_paren_has_comma(&self) -> bool {
        let mut depth = 0i32;
        let mut j = self.i;
        while let Some(tok) = self.toks.get(j) {
            match tok.kind {
                TokKind::LParen | TokKind::LBracket => depth += 1,
                TokKind::RParen | TokKind::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                TokKind::Comma if depth == 1 => return true,
                TokKind::Eof => return false,
                _ => {}
            }
            j += 1;
        }
        false
    }

    /// Lookahead (non-consuming): does the run from the current identifier end
    /// in `=` on this logical line (`Ident headAtom* =`)? A head atom is a
    /// binder, a literal, or a bracketed group — the clause forms
    /// ([FLAVOR-ML-CLAUSES]). Operators are deliberately absent, so `f 1 == 2`
    /// stays an expression.
    fn is_binding_head(&self) -> bool {
        let mut j = self.i + 1; // past the leading identifier
        loop {
            match self.toks.get(j).map(|t| &t.kind) {
                Some(
                    TokKind::Ident(_)
                    | TokKind::Int(_)
                    | TokKind::Str(_)
                    | TokKind::KwTrue
                    | TokKind::KwFalse,
                ) => j += 1,
                Some(TokKind::Op(op)) if op == MINUS_OP => j += 1,
                Some(TokKind::LParen) => j = Self::past_group(self.toks, j, &TokKind::RParen),
                Some(TokKind::LBracket) => j = Self::past_group(self.toks, j, &TokKind::RBracket),
                Some(TokKind::Eq) => return true,
                _ => return false,
            }
        }
    }

    /// Index just past the bracketed group opening at `open`, scanning to its
    /// `close` token. Nesting is not tracked: a head atom's group holds a
    /// pattern, which cannot itself contain a bracket in this flavor.
    fn past_group(toks: &[Token], open: usize, close: &TokKind) -> usize {
        let mut j = open + 1;
        while !matches!(toks.get(j).map(|t| &t.kind), Some(TokKind::Eof) | None) {
            if toks.get(j).map(|t| &t.kind) == Some(close) {
                return j + 1;
            }
            j += 1;
        }
        j
    }

    // --- expressions (Pratt) ---------------------------------------------

    /// Parse an expression whose operators bind at least as tightly as `min_bp`
    /// — the driving loop of Pratt / precedence climbing (Pratt 1973; Norvell).
    fn expr(&mut self, min_bp: u8) -> MlExpr {
        let mut left = self.unary();
        while let TokKind::Op(op) = self.peek() {
            let op = op.clone();
            let Some(bp) = infix_bp(&op) else { break };
            if bp < min_bp {
                break;
            }
            self.advance();
            // `?:` is right-associative — recurse at its own binding power so
            // `f x ?: 0 ?: 1` groups as `f x ?: (0 ?: 1)`.
            let right = self.expr(if op == ELVIS_OP { bp } else { bp + 1 });
            left = MlExpr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        left
    }

    /// A prefix unary (`-x`, `!x`) or an application.
    fn unary(&mut self) -> MlExpr {
        if let TokKind::Op(op) = self.peek() {
            if op == "-" || op == "!" {
                let op = op.clone();
                self.advance();
                let operand = self.unary();
                return MlExpr::Unary {
                    op,
                    operand: Box::new(operand),
                };
            }
        }
        self.application()
    }

    /// Whitespace application `f a b`, left-associative, recorded as nested
    /// single-argument [`MlExpr::App`] ([FLAVOR-ML-CALL]).
    fn application(&mut self) -> MlExpr {
        let mut func = self.postfix();
        // `Head(field = v, …)` is an inline record literal, not application: any
        // identifier immediately followed by `(ident = …`. An UPPERCASE head is
        // construction (`Ctor(...)`); a LOWERCASE head is a non-destructive record
        // update (`receiver(...)`). Both lower to the same `MlExpr::Record` node —
        // and to the same canonical `Expr::TypeConstructor { name }` the Default
        // `Ctor { f: v }` / `receiver { f: v }` produce ([FLAVOR-ML-RECORD]).
        if let MlExpr::Ident(name) = &func {
            if self.at_inline_record() {
                let name = name.clone();
                func = self.inline_record(name, Vec::new());
            } else if is_constructor(name) && self.at_generic_record() {
                // `Box<int>(item = 7)` — explicit construction-site type
                // arguments. Implements [TYPE-GENERICS-DECL],
                // [FLAVOR-ML-GENERICS].
                let name = name.clone();
                let type_args = match self.ty_generic_args(name.clone()) {
                    MlType::App { args, .. } => args,
                    _ => Vec::new(),
                };
                func = self.inline_record(name, type_args);
            }
        }
        // `f ()` is a zero-argument application, not application to unit.
        if matches!(self.peek(), TokKind::LParen) && matches!(self.peek_at(1), TokKind::RParen) {
            self.advance();
            self.advance();
            func = MlExpr::UnitApp {
                func: Box::new(func),
            };
        }
        while self.starts_atom() {
            // `f (a, b)` — a parenthesised comma-list argument is the uncurried
            // saturated call: a single multi-argument `Call` ([FLAVOR-ML-CALL]).
            // A lone `f (a)` has no top-level comma and stays plain grouping.
            if matches!(self.peek(), TokKind::LParen) && self.first_paren_has_comma() {
                let args = self.uncurried_args();
                func = MlExpr::AppMulti {
                    func: Box::new(func),
                    args,
                };
                continue;
            }
            let arg = self.postfix();
            func = MlExpr::App {
                func: Box::new(func),
                arg: Box::new(arg),
            };
        }
        func
    }

    /// `( e ( , e )* )` — the parenthesised comma-list arguments of an uncurried
    /// saturated call, lowered to a single multi-argument `Call` ([FLAVOR-ML-CALL]).
    fn uncurried_args(&mut self) -> Vec<MlExpr> {
        self.advance(); // `(`
        let mut args = Vec::new();
        if !matches!(self.peek(), TokKind::RParen) {
            loop {
                args.push(self.expr(0));
                if !self.eat(&TokKind::Comma) {
                    break;
                }
                if matches!(self.peek(), TokKind::RParen) {
                    break; // tolerate a trailing comma
                }
            }
        }
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        args
    }

    /// Postfix `.field` access and glued `[index]` chained onto an atom. A `[`
    /// only indexes when it abuts the target (`xs[0]`); a spaced `[` is a list
    /// literal argument, left for [`Self::application`] ([FLAVOR-ML-INDEX]).
    fn postfix(&mut self) -> MlExpr {
        let mut target = self.atom();
        loop {
            if self.eat(&TokKind::Dot) {
                if let Some(name) = self.ident() {
                    target = MlExpr::Field {
                        target: Box::new(target),
                        name,
                    };
                }
            } else if matches!(self.peek(), TokKind::LBracket) && self.glued() {
                target = self.index(target);
            } else {
                return target;
            }
        }
    }

    /// `target[index]` — consume a glued bracket index.
    fn index(&mut self, target: MlExpr) -> MlExpr {
        self.advance(); // `[`
        let index = self.expr(0);
        if !self.eat(&TokKind::RBracket) {
            self.error("expected ']'");
        }
        MlExpr::Index {
            target: Box::new(target),
            index: Box::new(index),
        }
    }

    /// Whether the current token abuts the previous one with no whitespace.
    fn glued(&self) -> bool {
        self.toks.get(self.i).is_some_and(|t| t.glued)
    }

    /// Whether the next token can begin an argument atom.
    fn starts_atom(&self) -> bool {
        matches!(
            self.peek(),
            TokKind::Int(_)
                | TokKind::Float(_)
                | TokKind::Str(_)
                | TokKind::Ident(_)
                | TokKind::KwTrue
                | TokKind::KwFalse
                | TokKind::LParen
                | TokKind::LBracket
        )
    }

    fn atom(&mut self) -> MlExpr {
        match self.peek().clone() {
            TokKind::Int(n) => {
                self.advance();
                MlExpr::Int(n)
            }
            TokKind::Float(f) => {
                self.advance();
                MlExpr::Float(f)
            }
            TokKind::KwTrue => {
                self.advance();
                MlExpr::Bool(true)
            }
            TokKind::KwFalse => {
                self.advance();
                MlExpr::Bool(false)
            }
            TokKind::Str(raw) => {
                self.advance();
                MlExpr::Str(raw)
            }
            TokKind::KwMatch => self.match_expr(),
            TokKind::KwSpawn => self.spawn_expr(),
            TokKind::KwPerform => self.perform_expr(),
            TokKind::KwHandle => self.handle_expr(),
            TokKind::KwResume => self.resume_expr(),
            TokKind::KwAwait => self.await_expr(),
            TokKind::KwYield => self.yield_expr(),
            TokKind::KwSend => self.send_expr(),
            TokKind::KwRecv => self.recv_expr(),
            TokKind::KwSelect => self.select_expr(),
            TokKind::Backslash => self.lambda(),
            TokKind::LParen => self.paren(),
            TokKind::LBracket => self.list(),
            TokKind::Ident(name) => {
                self.advance();
                self.ident_atom(name)
            }
            other => {
                self.error(format!("unexpected token {other:?} in expression"));
                self.advance();
                MlExpr::Bool(false)
            }
        }
    }

    /// An identifier atom: a bare reference, or — for an uppercase constructor
    /// directly followed by an indented `field = value` block — a record
    /// literal ([FLAVOR-ML-RECORD]).
    fn ident_atom(&mut self, name: String) -> MlExpr {
        let mut segments = vec![name];
        while self.eat(&TokKind::ColonColon) {
            if let Some(segment) = self.ident() {
                segments.push(segment);
            } else {
                self.error("expected path segment after '::'");
                break;
            }
        }
        if segments.len() > 1 {
            return MlExpr::Path(MlSymbolPath { segments });
        }
        let name = segments.pop().unwrap_or_default();
        if is_constructor(&name) && matches!(self.peek(), TokKind::Indent) {
            let fields = self.record_fields();
            MlExpr::Record {
                name,
                type_args: Vec::new(),
                fields,
            }
        } else {
            MlExpr::Ident(name)
        }
    }

    /// A bracket literal: a `[ k => v, … ]` map when a top-level `=>` (or the
    /// explicit empty form `[=>]`) is present, otherwise a `[ a, b, c ]` list
    /// ([FLAVOR-ML-LIST], [FLAVOR-ML-MAP]). Layout is suppressed inside brackets,
    /// so elements may span lines.
    fn list(&mut self) -> MlExpr {
        if self.bracket_is_map() {
            return self.map_literal();
        }
        self.advance(); // `[`
        let mut items = Vec::new();
        if !matches!(self.peek(), TokKind::RBracket) {
            items.push(self.expr(0));
            while self.eat(&TokKind::Comma) {
                if matches!(self.peek(), TokKind::RBracket) {
                    break; // tolerate a trailing comma
                }
                items.push(self.expr(0));
            }
        }
        if !self.eat(&TokKind::RBracket) {
            self.error("expected ']'");
        }
        MlExpr::List(items)
    }

    /// Non-consuming lookahead: does the bracket group opening at the current `[`
    /// hold map entries? True when a `=>` appears at the group's own nesting
    /// depth before the matching `]`, or for the explicit empty form `[=>]`.
    fn bracket_is_map(&self) -> bool {
        let mut depth = 0i32;
        let mut j = self.i;
        while let Some(tok) = self.toks.get(j) {
            match tok.kind {
                TokKind::LBracket | TokKind::LParen => depth += 1,
                TokKind::RBracket | TokKind::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return false; // closed without a top-level `=>`
                    }
                }
                TokKind::FatArrow if depth == 1 => return true,
                TokKind::Eof => return false,
                _ => {}
            }
            j += 1;
        }
        false
    }

    /// `[ k => v ( , k => v )* ]` or the empty `[=>]` — a map literal. Each entry
    /// is `key => value`; it lowers to the same [`Expr::Map`] the Default
    /// `{ k: v }` produces ([FLAVOR-ML-MAP]).
    fn map_literal(&mut self) -> MlExpr {
        self.advance(); // `[`
        let mut entries = Vec::new();
        // The explicit empty form `[=>]` yields a zero-entry map.
        if self.eat(&TokKind::FatArrow) {
            let _ = self.eat(&TokKind::RBracket);
            return MlExpr::Map(entries);
        }
        if !matches!(self.peek(), TokKind::RBracket) {
            loop {
                entries.push(self.map_entry());
                if !self.eat(&TokKind::Comma) {
                    break;
                }
                if matches!(self.peek(), TokKind::RBracket) {
                    break; // tolerate a trailing comma
                }
            }
        }
        if !self.eat(&TokKind::RBracket) {
            self.error("expected ']'");
        }
        MlExpr::Map(entries)
    }

    /// One `key => value` map entry.
    fn map_entry(&mut self) -> (MlExpr, MlExpr) {
        let key = self.expr(0);
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in map entry");
        }
        let value = self.expr(0);
        (key, value)
    }

    /// `( expr )` grouping, kept as an [`MlExpr::Paren`] node.
    fn paren(&mut self) -> MlExpr {
        self.advance(); // `(`
        let inner = self.expr(0);
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        MlExpr::Paren(Box::new(inner))
    }

    /// `\param* => body` lambda. The juxtaposed head `\x y =>` is curried; the
    /// parenthesised comma-list head `\(x, y) =>` is uncurried ([FLAVOR-ML-CURRY]).
    fn lambda(&mut self) -> MlExpr {
        let pos = self.pos();
        self.advance(); // `\`
        let (params, uncurried) = self.head_params();
        // Clause heads are a *definition* form; a lambda has nowhere to put the
        // alternative arms ([FLAVOR-ML-CLAUSES]).
        if params.iter().any(|p| matches!(p, MlParam::Pattern(_))) {
            self.error_at(
                pos,
                "a lambda head takes plain parameters; use 'match' to select on a pattern",
            );
        }
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in lambda");
        }
        let body = self.body_after_eq();
        MlExpr::Lambda {
            params,
            uncurried,
            body: Box::new(body),
            pos,
        }
    }

    /// `spawn body` — start a fiber. The body is an indented layout block or an
    /// inline expression, parsed exactly like a `=`/`=>` body ([FLAVOR-ML-SPAWN]).
    fn spawn_expr(&mut self) -> MlExpr {
        self.advance(); // `spawn`
        MlExpr::Spawn(Box::new(self.body_after_eq()))
    }

    /// `perform Effect.op arg…` — perform an effect operation with
    /// whitespace-applied arguments ([FLAVOR-ML-EFFECT]). The head is the
    /// dotted `Effect.operation`; the trailing atoms are its arguments.
    fn perform_expr(&mut self) -> MlExpr {
        let pos = self.pos();
        self.advance(); // `perform`
        let first = self.ident().unwrap_or_default();
        let effect = self.qualified_name_tail(first);
        if !self.eat(&TokKind::Dot) {
            self.error("expected '.' between effect and operation in perform");
        }
        let operation = self.ident().unwrap_or_default();
        // `op ()` is a zero-argument performance, not application to unit.
        if matches!(self.peek(), TokKind::LParen) && matches!(self.peek_at(1), TokKind::RParen) {
            self.advance();
            self.advance();
            return MlExpr::Perform {
                effect,
                operation,
                args: Vec::new(),
                pos,
            };
        }
        let mut args = Vec::new();
        while self.starts_atom() {
            args.push(self.postfix());
        }
        MlExpr::Perform {
            effect,
            operation,
            args,
            pos,
        }
    }

    /// `handle Effect` + indented `op param* => body` arms + `in body` — install
    /// an effect handler over the body expression ([FLAVOR-ML-EFFECT]).
    fn handle_expr(&mut self) -> MlExpr {
        let pos = self.pos();
        self.advance(); // `handle`
        let first = self.ident().unwrap_or_default();
        let effect = self.qualified_name_tail(first);
        let mut arms = Vec::new();
        if self.eat(&TokKind::Indent) {
            while !self.at_block_end() {
                self.skip_separators();
                if self.at_block_end() {
                    break;
                }
                let before = self.i;
                arms.push(self.handle_arm());
                if self.i == before {
                    self.recover();
                }
            }
            let _ = self.eat(&TokKind::Dedent);
        }
        self.skip_separators();
        if !self.eat(&TokKind::KwIn) {
            self.error("expected 'in' after handle arms");
        }
        let body = self.body_after_eq();
        MlExpr::Handle {
            effect,
            arms,
            body: Box::new(body),
            pos,
        }
    }

    /// One `op param* => body` arm of a `handle` expression.
    fn handle_arm(&mut self) -> MlHandleArm {
        let operation = self.ident().unwrap_or_default();
        let mut params = Vec::new();
        while let TokKind::Ident(name) = self.peek() {
            params.push(name.clone());
            self.advance();
        }
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in handle arm");
        }
        let body = self.body_after_eq();
        MlHandleArm {
            operation,
            params,
            body,
        }
    }

    /// `resume`, `resume value`, or `resume` + an indented block — resume a
    /// suspended continuation. A `resume` with no argument yields a unit resume,
    /// like the Default `resume()` ([FLAVOR-ML-EFFECT]).
    fn resume_expr(&mut self) -> MlExpr {
        self.advance(); // `resume`
                        // `resume ()` is a unit resume, like the Default `resume()`.
        if matches!(self.peek(), TokKind::LParen) && matches!(self.peek_at(1), TokKind::RParen) {
            self.advance();
            self.advance();
            return MlExpr::Resume(None);
        }
        // An indented block, or an inline `match`/expression, is the resumed
        // value; bare `resume` on its own line resumes with unit.
        if matches!(self.peek(), TokKind::Indent) || self.starts_resume_arg() {
            return MlExpr::Resume(Some(Box::new(self.body_after_eq())));
        }
        MlExpr::Resume(None)
    }

    /// Whether the current token begins an inline `resume` argument: an ordinary
    /// argument atom, or a `match` whose own arms supply the resumed value.
    fn starts_resume_arg(&self) -> bool {
        self.starts_atom() || matches!(self.peek(), TokKind::KwMatch)
    }

    /// `await fiber` — block on a spawned fiber. Takes one postfix atom (the
    /// fiber handle), so `await (spawn f x)` nests via the parenthesised group
    /// ([FLAVOR-ML-CONCURRENCY]).
    fn await_expr(&mut self) -> MlExpr {
        self.advance(); // `await`
        MlExpr::Await(Box::new(self.postfix()))
    }

    /// `yield` or `yield value` — yield from the current fiber. A bare `yield`
    /// (nothing more on the line) yields unit ([FLAVOR-ML-CONCURRENCY]).
    fn yield_expr(&mut self) -> MlExpr {
        self.advance(); // `yield`
        if self.starts_atom() {
            return MlExpr::Yield(Some(Box::new(self.postfix())));
        }
        MlExpr::Yield(None)
    }

    /// `send channel value` — send a value on a channel; channel and value are
    /// each one postfix atom ([FLAVOR-ML-CONCURRENCY]).
    fn send_expr(&mut self) -> MlExpr {
        self.advance(); // `send`
        let channel = Box::new(self.postfix());
        let value = Box::new(self.postfix());
        MlExpr::Send { channel, value }
    }

    /// `recv channel` — receive a value from a channel ([FLAVOR-ML-CONCURRENCY]).
    fn recv_expr(&mut self) -> MlExpr {
        self.advance(); // `recv`
        MlExpr::Recv(Box::new(self.postfix()))
    }

    /// `select` + indented `pattern => body` arms — choose among ready channel
    /// arms, reusing the `match` arm grammar ([FLAVOR-ML-CONCURRENCY]).
    fn select_expr(&mut self) -> MlExpr {
        self.advance(); // `select`
        MlExpr::Select(self.match_arms_block())
    }

    /// `match scrutinee` + indented `pattern => body` arms.
    fn match_expr(&mut self) -> MlExpr {
        self.advance(); // `match`
        let scrutinee = self.expr(0);
        let arms = self.match_arms_block();
        MlExpr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
        }
    }

    /// Parse an optional indented run of `pattern => body` arms shared by
    /// `match` and `select`.
    fn match_arms_block(&mut self) -> Vec<MlArm> {
        let mut arms = Vec::new();
        if self.eat(&TokKind::Indent) {
            while !self.at_block_end() {
                self.skip_separators();
                if self.at_block_end() {
                    break;
                }
                arms.push(self.match_arm());
            }
            let _ = self.eat(&TokKind::Dedent);
        }
        arms
    }

    fn match_arm(&mut self) -> MlArm {
        let pattern = self.pattern();
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in match arm");
        }
        let body = self.body_after_eq();
        MlArm { pattern, body }
    }

    /// A match pattern, plus the diagnostic for the or-pattern users reach for
    /// once `|` lexes: `|` separates union *variants*, never patterns
    /// ([FLAVOR-ML-UNION-INLINE]).
    fn pattern(&mut self) -> MlPattern {
        let pat = self.pattern_atom();
        if matches!(self.peek(), TokKind::Pipe) {
            self.error("or-patterns are not supported; write one arm per alternative");
            while self.eat(&TokKind::Pipe) {
                let _ = self.pattern_atom();
            }
        }
        pat
    }

    /// `( p )` — grouping only, erased at parse time
    /// ([FLAVOR-ML-PATTERN-GROUP]); it is what lets a clause head write
    /// `check (Node l r)`. A comma inside is not a tuple — Osprey has no tuple
    /// patterns.
    fn group_pattern(&mut self) -> MlPattern {
        self.advance(); // `(`
        let inner = self.pattern();
        while self.eat(&TokKind::Comma) {
            self.error("'(' groups a single pattern; Osprey has no tuple patterns");
            let _ = self.pattern();
        }
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        inner
    }

    /// A match pattern: `_`, a literal, `Ctor field…`, `( p )`, or a bare
    /// binding.
    fn pattern_atom(&mut self) -> MlPattern {
        match self.peek().clone() {
            // `-N` — a negative integer literal pattern. The lexer splits this
            // into `-` then the magnitude, so fold the sign into the literal so
            // `-5` matches `-5`, mirroring the Default flavor ([FLAVOR-ML-MATCH]).
            TokKind::Op(op) if op == MINUS_OP && matches!(self.peek_at(1), TokKind::Int(_)) => {
                self.advance(); // `-`
                match self.peek().clone() {
                    TokKind::Int(n) => {
                        self.advance();
                        MlPattern::Int(-n)
                    }
                    _ => MlPattern::Wildcard,
                }
            }
            TokKind::Int(n) => {
                self.advance();
                MlPattern::Int(n)
            }
            TokKind::Str(raw) => {
                self.advance();
                MlPattern::Str(raw)
            }
            TokKind::KwTrue => {
                self.advance();
                MlPattern::Bool(true)
            }
            TokKind::KwFalse => {
                self.advance();
                MlPattern::Bool(false)
            }
            TokKind::Ident(name) => {
                self.advance();
                self.ident_pattern(name)
            }
            TokKind::LBracket => self.list_pattern(),
            TokKind::LParen => self.group_pattern(),
            other => {
                self.error(format!("unexpected token {other:?} in pattern"));
                MlPattern::Wildcard
            }
        }
    }

    /// `[ p, … ]` or `[ p, …, ...rest ]` — a list pattern with fixed-prefix
    /// element patterns and an optional trailing `...name` rest-binder
    /// ([FLAVOR-ML-MATCH], [TYPE-LIST-PATTERNS]). Layout is suppressed inside
    /// brackets, so elements may span lines.
    fn list_pattern(&mut self) -> MlPattern {
        self.advance(); // `[`
        let mut elements = Vec::new();
        let mut rest = None;
        if !matches!(self.peek(), TokKind::RBracket) {
            loop {
                if let Some(name) = self.rest_binder() {
                    rest = Some(name);
                    break; // `...rest` is always the final element
                }
                elements.push(self.pattern());
                if !self.eat(&TokKind::Comma) {
                    break;
                }
                if matches!(self.peek(), TokKind::RBracket) {
                    break; // tolerate a trailing comma
                }
            }
        }
        if !self.eat(&TokKind::RBracket) {
            self.error("expected ']'");
        }
        MlPattern::List { elements, rest }
    }

    /// A `...name` rest-binder (three `.` tokens then an identifier), consumed
    /// only when it is actually present. Returns the bound name, or `None`.
    fn rest_binder(&mut self) -> Option<String> {
        let is_spread = matches!(self.peek(), TokKind::Dot)
            && matches!(self.peek_at(1), TokKind::Dot)
            && matches!(self.peek_at(2), TokKind::Dot);
        if !is_spread {
            return None;
        }
        self.advance();
        self.advance();
        self.advance();
        self.ident()
    }

    /// `_` → wildcard; `Ctor a b` → constructor binding payload fields; a bare
    /// lowercase name → a binding ([FLAVOR-ML-MATCH]).
    fn ident_pattern(&mut self, name: String) -> MlPattern {
        let name = self.qualified_name_tail(name);
        if name == "_" {
            return MlPattern::Wildcard;
        }
        if is_constructor(&name) {
            let mut fields = Vec::new();
            while let TokKind::Ident(field) = self.peek() {
                fields.push(field.clone());
                self.advance();
            }
            if matches!(self.peek(), TokKind::LParen) {
                self.error(
                    "nested constructor patterns are not supported; \
                     bind the payload and match it in a second expression",
                );
            }
            return MlPattern::Ctor { name, fields };
        }
        MlPattern::Bind(name)
    }

    /// The indented `field = value` lines of a layout record literal.
    fn record_fields(&mut self) -> Vec<MlField> {
        let mut fields = Vec::new();
        let _ = self.eat(&TokKind::Indent);
        while !self.at_block_end() {
            self.skip_separators();
            if self.at_block_end() {
                break;
            }
            match self.parse_record_field() {
                Some(field) => fields.push(field),
                None => self.recover(),
            }
        }
        let _ = self.eat(&TokKind::Dedent);
        fields
    }

    /// `( field = expr ( , field = expr )* )` — an inline record literal in
    /// expression/argument position ([FLAVOR-ML-RECORD]). Layout is suppressed
    /// inside parens, so the fields are a simple comma list; it lowers to the
    /// same [`MlExpr::Record`] the layout form produces.
    fn inline_record(&mut self, name: String, type_args: Vec<MlType>) -> MlExpr {
        self.advance(); // `(`
        let mut fields = Vec::new();
        if !matches!(self.peek(), TokKind::RParen) {
            loop {
                match self.parse_record_field() {
                    Some(field) => fields.push(field),
                    None => self.recover(),
                }
                if !self.eat(&TokKind::Comma) {
                    break;
                }
                if matches!(self.peek(), TokKind::RParen) {
                    break; // tolerate a trailing comma
                }
            }
        }
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        MlExpr::Record {
            name,
            type_args,
            fields,
        }
    }

    /// One `field = value` initialiser, shared by the layout and inline record
    /// forms so neither duplicates the field-parsing rule.
    fn parse_record_field(&mut self) -> Option<MlField> {
        let name = self.ident()?;
        let _ = self.expect_eq();
        let value = self.body_after_eq();
        Some(MlField { name, value })
    }

    /// Whether the current `(` opens an inline record literal — its first two
    /// tokens are `Ident` then `=`. Used to disambiguate `Ctor(field = v)` (a
    /// record) from `Ctor (expr)` (application) and `Ctor ()` (unit application).
    fn at_inline_record(&self) -> bool {
        matches!(self.peek(), TokKind::LParen)
            && matches!(self.peek_at(1), TokKind::Ident(_))
            && matches!(self.peek_at(2), TokKind::Eq)
    }

    /// Whether the current `<` opens a construction-site type-argument list
    /// followed by an inline record (`Ctor<int>(field = v)`): scan a balanced
    /// `<…>` of type-shaped tokens, then require the `( Ident =` record
    /// opener — so a `Ctor < x` comparison never misparses.
    fn at_generic_record(&self) -> bool {
        if !matches!(self.peek(), TokKind::Op(op) if op == "<") {
            return false;
        }
        let mut depth = 0usize;
        let mut j = 0usize;
        loop {
            match self.peek_at(j) {
                TokKind::Op(op) if op == "<" => depth += 1,
                TokKind::Op(op) if op == ">" => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return matches!(self.peek_at(j + 1), TokKind::LParen)
                            && matches!(self.peek_at(j + 2), TokKind::Ident(_))
                            && matches!(self.peek_at(j + 3), TokKind::Eq);
                    }
                }
                TokKind::Ident(_)
                | TokKind::Comma
                | TokKind::Arrow
                | TokKind::LParen
                | TokKind::RParen => {}
                _ => return false,
            }
            j += 1;
        }
    }

    // --- bodies and helpers ----------------------------------------------

    /// The body after `=`/`=>`: an inline expression, or an indented layout
    /// block whose trailing expression is its value ([FLAVOR-ML-BLOCK]).
    fn body_after_eq(&mut self) -> MlExpr {
        if !matches!(self.peek(), TokKind::Indent) {
            return self.expr(0);
        }
        self.advance(); // `Indent`
        let (items, value) = self.block_items();
        let _ = self.eat(&TokKind::Dedent);
        MlExpr::Block { items, value }
    }

    /// The items (and optional trailing value) of an indented block.
    fn block_items(&mut self) -> (Vec<MlItem>, Option<Box<MlExpr>>) {
        let mut items = Vec::new();
        let mut value = None;
        while !self.at_block_end() {
            self.skip_separators();
            if self.at_block_end() {
                break;
            }
            let before = self.i;
            value = self.block_line(&mut items);
            // Forward-progress guard ([FLAVOR-LOWER-CONTRACT]): a `block_line`
            // whose `item()` errored without consuming a token — a reserved word
            // (`do`/`effect`/…) or a malformed line inside the block — would
            // otherwise spin this loop forever. Recover past the offending token,
            // exactly as the top-level `program()` loop does, so any input
            // terminates.
            if self.i == before {
                self.recover();
            }
        }
        (items, value)
    }

    /// Parse one block line. A trailing bare expression with nothing after it is
    /// the block value; anything else is appended as an item.
    fn block_line(&mut self, items: &mut Vec<MlItem>) -> Option<Box<MlExpr>> {
        match self.item() {
            Some(MlItem::Expr { value, .. }) if self.at_block_end() => Some(Box::new(value)),
            Some(item) => {
                items.push(item);
                None
            }
            None => None,
        }
    }

    pub(super) fn ident(&mut self) -> Option<String> {
        if let TokKind::Ident(name) = self.peek() {
            let name = name.clone();
            self.advance();
            Some(name)
        } else {
            self.error("expected an identifier");
            None
        }
    }

    /// Consume zero or more `::segment` suffixes after an already-consumed
    /// identifier, retaining the written qualification for canonical fields
    /// which still store type/effect/constructor names as strings.
    fn qualified_name_tail(&mut self, first: String) -> String {
        let mut segments = vec![first];
        while self.eat(&TokKind::ColonColon) {
            if let Some(segment) = self.ident() {
                segments.push(segment);
            } else {
                self.error("expected path segment after '::'");
                break;
            }
        }
        segments.join("::")
    }

    fn expect_eq(&mut self) -> bool {
        if self.eat(&TokKind::Eq) {
            true
        } else {
            self.error("expected '='");
            false
        }
    }
}

/// An uppercase initial marks a constructor/type name; lowercase marks a value
/// binding or variable, mirroring the Default flavor's lexical convention.
fn is_constructor(name: &str) -> bool {
    name.chars().next().is_some_and(char::is_uppercase)
}
