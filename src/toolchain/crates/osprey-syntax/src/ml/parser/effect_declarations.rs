//! ML effect declarations parsing.
use super::{
    keyword_spelling, MlEffectOp, MlEffectRef, MlItem, MlType, Multiplicity, OperationMarkers,
    OperationMode, Parser, Stage, TokKind, CONTROL_KEYWORD, REPLAYABLE_KEYWORD,
};

impl Parser<'_> {
    /// `effect Name` + an indented block of `op : P => R` operation lines — an
    /// algebraic effect declaration ([FLAVOR-ML-EFFECT]). Mirrors [`Self::type_decl`]'s
    /// layout-block parsing.
    pub(in crate::ml) fn effect_decl(&mut self, stage: Stage) -> Option<MlItem> {
        let pos = self.pos();
        self.advance(); // `effect`
        let name = self.ident()?;
        // `effect State T` — type parameters between the name and the
        // operation block. Implements [EFFECTS-GENERIC-DECL].
        let type_params = self.type_params();
        let operations = self.effect_operations();
        Some(MlItem::Effect {
            stage,
            name,
            type_params,
            operations,
            pos,
        })
    }

    /// The indented `op : P => R` operation lines of an `effect` block.
    pub(in crate::ml) fn effect_operations(&mut self) -> Vec<MlEffectOp> {
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

    /// One `op : payload => result` operation line, optionally preceded by its
    /// own `(** … *)` doc ([DOC-EFFECT-OP]).
    pub(in crate::ml) fn effect_op(&mut self) -> Option<MlEffectOp> {
        let doc = self.effect_op_doc();
        let markers = self.operation_markers()?;
        // Anchor on the NAME, as Default does: `control`/`replayable` markers
        // precede it, and hover and diagnostics resolve by the name's column.
        let pos = markers.pos;
        if !self.eat(&TokKind::Colon) {
            self.error("expected ':' in effect operation");
        }
        let payload = self.ty();
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in effect operation");
        }
        let result = self.ty();
        Some(MlEffectOp {
            name: markers.name,
            mode: markers.mode,
            multiplicity: markers.multiplicity,
            replayable: markers.replayable,
            payload,
            result,
            doc,
            pos,
        })
    }

    /// The optional `control [abort|once|many]` and `replayable` markers an
    /// operation line may carry before its name, and the name itself.
    ///
    /// A marker is only a marker when ANOTHER identifier follows it: ML has no
    /// keyword for any of these words, so `abort : string => Unit` still
    /// declares an operation *called* `abort`. That is the same contextual rule
    /// the Default flavor's grammar resolves with GLR. Implements [MULTI-DECL],
    /// [FLAVOR-ML-EFFECT-ANNOTATIONS].
    fn operation_markers(&mut self) -> Option<OperationMarkers> {
        let mut markers = OperationMarkers {
            pos: self.pos(),
            name: self.operation_ident()?,
            ..OperationMarkers::default()
        };
        if markers.name == CONTROL_KEYWORD && self.at_operation_name() {
            markers.mode = OperationMode::Control;
            markers.pos = self.pos();
            markers.name = self.operation_ident()?;
        }
        if let Some(declared) = Multiplicity::from_keyword(&markers.name) {
            if markers.mode.is_control() && self.at_operation_name() {
                markers.multiplicity = Some(declared);
                markers.pos = self.pos();
                markers.name = self.operation_ident()?;
            }
        }
        if markers.name == REPLAYABLE_KEYWORD && self.at_operation_name() {
            markers.replayable = true;
            markers.pos = self.pos();
            markers.name = self.operation_ident()?;
        }
        Some(markers)
    }

    /// Whether the cursor sits on an operation name — the token that proves the
    /// word just read was a marker rather than the operation's own name.
    pub(in crate::ml) fn at_operation_name(&self) -> bool {
        matches!(self.peek(), TokKind::Ident(_)) || keyword_spelling(self.peek()).is_some()
    }

    /// An effect operation's name. Operation names are their own namespace, so
    /// a word this flavor reserves elsewhere still names an operation here:
    /// `send : T => Unit` declares an operation called `send`, exactly as
    /// `abort : string => Unit` declares one called `abort`. The position is
    /// unambiguous — an `effect` block's indented lines, the name after
    /// `perform Effect.`, and a handler arm's head admit nothing but a name.
    /// Implements [FLAVOR-ML-EFFECT-OP-NAME].
    pub(in crate::ml) fn operation_ident(&mut self) -> Option<String> {
        if let Some(spelling) = keyword_spelling(self.peek()) {
            self.advance();
            return Some(spelling.to_owned());
        }
        self.ident()
    }

    /// Consume a `(** … *)` doc token sitting in front of an operation line.
    /// Separators between the doc and the operation are skipped so a doc on its
    /// own line still attaches.
    pub(in crate::ml) fn effect_op_doc(&mut self) -> Option<String> {
        let TokKind::Doc(text) = self.peek().clone() else {
            return None;
        };
        self.i += 1;
        self.skip_separators();
        Some(text)
    }

    /// An optional effect row after a signature's type: `! Ref(, Ref)*` or the
    /// bracketed `! [Ref, …]`, each reference optionally applied to type
    /// arguments (`State<int>`). Empty when no `!` is present
    /// ([FLAVOR-ML-EFFECT], [EFFECTS-GENERIC-ROWS]).
    pub(in crate::ml) fn effect_row(&mut self) -> (Vec<MlEffectRef>, Option<String>, bool) {
        if !matches!(self.peek(), TokKind::Op(op) if op == "!") {
            return (Vec::new(), None, false);
        }
        self.advance(); // `!`
        let bracketed = self.eat(&TokKind::LBracket);
        let mut effects = Vec::new();
        if !bracketed || !matches!(self.peek(), TokKind::RBracket | TokKind::Pipe) {
            if let Some(r) = self.effect_ref() {
                effects.push(r);
                while self.eat(&TokKind::Comma) {
                    if let Some(r) = self.effect_ref() {
                        effects.push(r);
                    }
                }
            }
        }
        let mut tail = if bracketed && self.eat(&TokKind::Pipe) {
            self.ident()
        } else {
            None
        };
        if bracketed && !self.eat(&TokKind::RBracket) {
            self.error("expected ']' to close effect row");
        }
        let implicit_tail = !bracketed
            && effects.first().is_some_and(|effect| {
                effects.len() == 1
                    && effect.args.is_empty()
                    && !effect.name.contains("::")
                    && effect.name.chars().next().is_some_and(char::is_lowercase)
            });
        if implicit_tail {
            tail = effects.pop().map(|effect| effect.name);
        }
        if tail
            .as_ref()
            .is_some_and(|name| !name.chars().next().is_some_and(char::is_lowercase))
        {
            self.error("effect row variable must start with a lowercase letter");
        }
        (effects, tail, true)
    }

    /// One effect reference in an effect row: a name plus optional
    /// angle-bracketed type arguments.
    pub(in crate::ml) fn effect_ref(&mut self) -> Option<MlEffectRef> {
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
}
