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

mod blocks;
mod collections;
mod declarations;
mod effect_declarations;
mod effects;
mod expressions;
mod items;
mod parameters;
mod patterns;
mod types;

use super::cst::{
    MlArm, MlBinder, MlEffectOp, MlEffectRef, MlExpr, MlExternParam, MlField, MlHandleArm, MlItem,
    MlModuleKind, MlParam, MlPattern, MlSymbolPath, MlType, MlTypeField, MlTypeParam, MlVariance,
    MlVariant,
};
use super::lexer::lex;
use super::token::{keyword_spelling, TokKind, Token};
use crate::SyntaxError;
use osprey_ast::{
    Multiplicity, OperationMode, Position, Stage, CONTROL_KEYWORD, REPLAYABLE_KEYWORD,
    STATIC_STAGE_KEYWORD,
};

/// The declaration markers read off one effect-operation line, before its name
/// is known to be a name rather than another marker. Implements [MULTI-DECL].
#[derive(Default)]
struct OperationMarkers {
    mode: OperationMode,
    multiplicity: Option<Multiplicity>,
    replayable: bool,
    name: String,
    /// Where the name itself starts, after any markers.
    pos: Position,
}

/// Parse ML-flavor `source` into the ML CST plus any syntax errors. Best-effort:
/// errors never abort the parse ([FLAVOR-LOWER-CONTRACT]).
pub(crate) fn parse(source: &str) -> (Vec<MlItem>, Vec<SyntaxError>) {
    let (items, errors, _) = parse_annotations(source);
    (items, errors)
}

pub(super) fn parse_annotations(
    source: &str,
) -> (Vec<MlItem>, Vec<SyntaxError>, Vec<crate::AnnotationEdit>) {
    let (tokens, mut errors) = lex(source);
    let mut annotations = Vec::new();
    let items = {
        let mut parser = Parser {
            toks: &tokens,
            i: 0,
            errors: &mut errors,
            annotations: &mut annotations,
            source,
        };
        parser.program()
    };
    super::modules::validate(&items, &mut errors);
    (items, errors, annotations)
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
    annotations: &'t mut Vec<crate::AnnotationEdit>,
    source: &'t str,
}

impl Parser<'_> {
    /// Consume the separator between elements of a comma-separated list,
    /// answering whether another element follows. A trailing comma before
    /// `close` ends the list rather than demanding an element after it, so
    /// `[1, 2,]` reads as two elements.
    fn more_in_list(&mut self, close: &TokKind) -> bool {
        self.eat(&TokKind::Comma) && self.peek() != close
    }

    pub(super) fn peek(&self) -> &TokKind {
        self.toks.get(self.i).map_or(&TokKind::Eof, |t| &t.kind)
    }

    pub(super) fn peek_at(&self, ahead: usize) -> &TokKind {
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

    // --- expressions (Pratt) ---------------------------------------------

    // --- bodies and helpers ----------------------------------------------

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
/// The name an inline record literal would construct, for the heads that can
/// carry one: a bare identifier, or a QUALIFIED PATH naming a constructor
/// reached across a module boundary (`Geo::Circle(radius = 3)`). Rendered with
/// `::` exactly as the Default flavor's `field_text` renders the name in
/// `Geo::Circle { radius: 3 }`, so both spellings reach the assembler as one
/// name ([FLAVOR-IR-EQUIV]).
///
/// Without the path arm the qualified form was parsed as APPLICATION to a
/// parenthesised group: `radius = 3` collapsed to a bare `radius` and the value
/// was dropped, so an exported union's constructors were reachable only from
/// inside their own module ([MODULES-EXPORTS], [MODULES-OPAQUE-TYPES] — which
/// singles out OPAQUE constructors as the private ones).
fn record_head(head: &MlExpr) -> Option<String> {
    match head {
        MlExpr::Ident(name) => Some(name.clone()),
        MlExpr::Path(path) => Some(path.segments.join("::")),
        _ => None,
    }
}

pub(super) fn is_constructor(name: &str) -> bool {
    osprey_ast::is_constructor_name(name)
}

/// A parsed `handle Effect` and its arms, waiting for the region it handles.
/// The containing block supplies its remainder. Implements [EFFECTS-HANDLE-REST].
struct HandleHead {
    stage: Stage,
    effect: String,
    arms: Vec<MlHandleArm>,
    return_clause: Option<Box<MlExpr>>,
    pos: Position,
}

impl HandleHead {
    fn over(self, body: MlExpr) -> MlExpr {
        MlExpr::Handle {
            stage: self.stage,
            effect: self.effect,
            arms: self.arms,
            return_clause: self.return_clause,
            body: Box::new(body),
            pos: self.pos,
        }
    }
}
