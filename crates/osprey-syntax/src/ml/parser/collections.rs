//! ML collections parsing.
use super::{MlExpr, MlField, MlType, Parser, TokKind};

impl Parser<'_> {
    /// Parse the first expression before distinguishing a list from a map.
    /// Lambda and match arrows belong to that expression, not the collection.
    /// Implements [FLAVOR-ML-LIST], [FLAVOR-ML-MAP].
    pub(in crate::ml) fn list(&mut self) -> MlExpr {
        let open = self.pos();
        self.advance();
        if self.eat(&TokKind::FatArrow) {
            self.close_bracket();
            return MlExpr::Map(Vec::new());
        }
        if self.eat(&TokKind::RBracket) {
            return MlExpr::List(Vec::new(), open);
        }
        let first = self.expr(0);
        if self.eat(&TokKind::FatArrow) {
            return self.map_tail(first);
        }
        self.list_tail(first, open)
    }

    fn list_tail(&mut self, first: MlExpr, open: super::Position) -> MlExpr {
        let mut items = vec![first];
        while self.more_in_list(&TokKind::RBracket) {
            items.push(self.expr(0));
        }
        self.close_bracket();
        MlExpr::List(items, open)
    }

    fn map_tail(&mut self, first_key: MlExpr) -> MlExpr {
        let mut entries = vec![(first_key, self.expr(0))];
        while self.more_in_list(&TokKind::RBracket) {
            entries.push(self.map_entry());
        }
        self.close_bracket();
        MlExpr::Map(entries)
    }

    fn close_bracket(&mut self) {
        if !self.eat(&TokKind::RBracket) {
            self.error("expected ']'");
        }
    }

    /// One `key => value` map entry.
    pub(in crate::ml) fn map_entry(&mut self) -> (MlExpr, MlExpr) {
        let key = self.expr(0);
        if !self.eat(&TokKind::FatArrow) {
            self.error("expected '=>' in map entry");
        }
        let value = self.expr(0);
        (key, value)
    }

    /// `( expr )` grouping, kept as an [`MlExpr::Paren`] node.
    pub(in crate::ml) fn paren(&mut self) -> MlExpr {
        self.advance(); // `(`
        let inner = self.expr(0);
        if !self.eat(&TokKind::RParen) {
            self.error("expected ')'");
        }
        MlExpr::Paren(Box::new(inner))
    }

    /// The indented `field = value` lines of a layout record literal.
    pub(in crate::ml) fn record_fields(&mut self) -> Vec<MlField> {
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
    pub(in crate::ml) fn inline_record(&mut self, name: String, type_args: Vec<MlType>) -> MlExpr {
        let fields = self.delimited_record_fields(&TokKind::RParen, "')'");
        MlExpr::Record {
            name,
            type_args,
            fields,
        }
    }

    /// Empty braces retain the shared empty-map meaning. [TYPE-RECORD-ANON]
    pub(in crate::ml) fn anonymous_record(&mut self) -> MlExpr {
        let fields = self.delimited_record_fields(&TokKind::RBrace, "'}'");
        if fields.is_empty() {
            MlExpr::Map(Vec::new())
        } else {
            MlExpr::Object(fields)
        }
    }

    /// Shared comma-list rule for named and anonymous records. [FLAVOR-ML-RECORD-ANON]
    pub(in crate::ml) fn delimited_record_fields(
        &mut self,
        close: &TokKind,
        label: &str,
    ) -> Vec<MlField> {
        self.advance();
        let mut fields = Vec::new();
        while self.peek() != close && self.peek() != &TokKind::Eof {
            match self.parse_record_field() {
                Some(field) => fields.push(field),
                None => self.recover_record_field(close),
            }
            if !self.more_in_list(close) {
                break;
            }
        }
        if !self.eat(close) {
            self.error(format!("expected {label}"));
        }
        fields
    }

    fn recover_record_field(&mut self, close: &TokKind) {
        while self.peek() != close && !matches!(self.peek(), TokKind::Comma | TokKind::Eof) {
            self.advance();
        }
    }

    /// One `field = value` initialiser, shared by the layout and inline record
    /// forms so neither duplicates the field-parsing rule.
    pub(in crate::ml) fn parse_record_field(&mut self) -> Option<MlField> {
        let name = self.ident()?;
        let _ = self.expect_eq();
        let value = self.body_after_eq();
        Some(MlField { name, value })
    }

    /// Whether the current `(` opens an inline record literal — its first two
    /// tokens are `Ident` then `=`. Used to disambiguate `Ctor(field = v)` (a
    /// record) from `Ctor (expr)` (application) and `Ctor ()` (unit application).
    pub(in crate::ml) fn at_inline_record(&self) -> bool {
        matches!(self.peek(), TokKind::LParen)
            && matches!(self.peek_at(1), TokKind::Ident(_))
            && matches!(self.peek_at(2), TokKind::Eq)
    }
}
