//! The ML-flavor lexer: a hand-written scanner that turns source text into a
//! flat [`Token`] stream, then derives the layout markers (`Indent`, `Dedent`,
//! `Newline`) from the offside rule ([FLAVOR-ML-LAYOUT]). Line and nested block
//! comment handling implements [FLAVOR-ML-COMMENTS].
//!
//! Two phases keep each piece small and testable: [`scan`] produces content
//! tokens with positions (no layout), and [`insert_layout`] walks those tokens
//! and inserts the layout markers from each line's first-token column, with
//! bracket depth suppressing layout inside `( … )`.
//!
//! ESCAPE HATCH: if this hand-written layout frontend becomes onerous or
//! accrues parsing bugs we cannot tame, we fall back to a `tree-sitter-osprey-ml`
//! grammar with an external INDENT/DEDENT/NEWLINE scanner.c — the boundary law
//! makes the parser mechanism a flavor-internal swap (docs/specs/language/0023-LanguageFlavors.md).

use super::doc_text::strip_doc_lines;
use super::token::{keyword_or_ident, TokKind, Token};
use crate::SyntaxError;
use osprey_ast::Position;

mod layout;
mod operators;
#[cfg(test)]
mod tests;

use layout::insert_layout;
use operators::{single_char_operator, two_char_operator};

/// Lex `source` into a layout-resolved token stream terminated by
/// [`TokKind::Eof`], plus any lexical errors.
pub(crate) fn lex(source: &str) -> (Vec<Token>, Vec<SyntaxError>) {
    let mut scanner = Scanner::new(source);
    let (content, mut errors) = scanner.scan();
    let (tokens, layout_errors) = insert_layout(content);
    errors.extend(layout_errors);
    (tokens, errors)
}

/// Phase-1 scanner over the raw characters.
struct Scanner {
    chars: Vec<char>,
    i: usize,
    byte_offset: usize,
    line: u32,
    col: u32,
    errors: Vec<SyntaxError>,
}

impl Scanner {
    fn new(source: &str) -> Self {
        Scanner {
            chars: source.chars().collect(),
            i: 0,
            byte_offset: 0,
            line: 1,
            col: 0,
            errors: Vec::new(),
        }
    }

    fn pos(&self) -> Position {
        Position {
            line: self.line,
            column: self.col,
        }
    }

    fn peek(&self, ahead: usize) -> Option<char> {
        self.chars.get(self.i + ahead).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.chars.get(self.i).copied()?;
        self.i += 1;
        self.byte_offset += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 0;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn error(&mut self, pos: Position, message: impl Into<String>) {
        self.errors.push(SyntaxError {
            message: message.into(),
            position: pos,
        });
    }

    /// Skip inline whitespace, newlines (layout is position-derived later),
    /// `// …` line comments, and `(* … *)` block comments. The block form is
    /// the ML-family convention (SML / OCaml / F#) and **nests**, so a
    /// commented-out region containing another `(* *)` closes correctly.
    fn skip_trivia(&mut self) {
        while let Some(c) = self.peek(0) {
            match c {
                ' ' | '\t' | '\r' | '\n' => {
                    let _ = self.bump();
                }
                // `//!` is an inner doc comment, not trivia — it breaks out so
                // `scan_token` emits it ([DOC-SIGIL-INNER]). A plain `// …`
                // line comment is still skipped.
                '/' if self.peek(1) == Some('/') => {
                    if self.at_inner_doc_comment() {
                        break;
                    }
                    while !matches!(self.peek(0), Some('\n') | None) {
                        let _ = self.bump();
                    }
                }
                // A `(**`-with-content doc comment stops trivia-skipping so
                // `scan_token` can emit it as a `Doc` token ([DOC-SIGIL-ML]);
                // an ordinary `(* … *)` (or an empty/banner `(**)`/`(*****)`)
                // is skipped as trivia.
                '(' if self.peek(1) == Some('*') => {
                    if self.at_doc_comment() {
                        break;
                    }
                    self.skip_block_comment();
                }
                _ => break,
            }
        }
    }

    /// True when the cursor is at a `(**`-opened doc comment: the opener is
    /// `(**` and the char after the second `*` is neither another `*` (banner
    /// `(*****)`) nor `)` (empty `(**)`). Matches odoc's rule ([DOC-SIGIL-ML]).
    fn at_doc_comment(&self) -> bool {
        self.peek(0) == Some('(')
            && self.peek(1) == Some('*')
            && self.peek(2) == Some('*')
            && !matches!(self.peek(3), Some('*' | ')') | None)
    }

    /// True when the cursor is at a `//!` inner doc comment. Its Default-flavor
    /// twin is the `_inner_doc_comment_line` grammar token ([DOC-SIGIL-INNER]).
    fn at_inner_doc_comment(&self) -> bool {
        self.peek(0) == Some('/') && self.peek(1) == Some('/') && self.peek(2) == Some('!')
    }

    /// Scan one or more consecutive `//!` lines (cursor at the first opener)
    /// into their joined text, each line stripped of its sigil and one optional
    /// following space — the same shape the Default lowerer produces, so both
    /// flavors hand the shared body parser identical input ([DOC-SIGIL-INNER]).
    fn scan_inner_doc_comment(&mut self) -> TokKind {
        let mut lines: Vec<String> = Vec::new();
        while self.at_inner_doc_comment() {
            let _ = self.bump(); // /
            let _ = self.bump(); // /
            let _ = self.bump(); // !
            let mut line = String::new();
            while !matches!(self.peek(0), Some('\n') | None) {
                if let Some(c) = self.bump() {
                    line.push(c);
                }
            }
            lines.push(
                line.strip_prefix(' ')
                    .unwrap_or(&line)
                    .trim_end()
                    .to_owned(),
            );
            self.skip_to_next_inner_doc_line();
        }
        TokKind::InnerDoc(lines.join("\n").trim().to_owned())
    }

    /// Consume the newline and indentation between two `//!` lines, leaving the
    /// cursor untouched when what follows is not another one.
    fn skip_to_next_inner_doc_line(&mut self) {
        let mark = self.i;
        let byte_mark = self.byte_offset;
        let (line, col) = (self.line, self.col);
        while matches!(self.peek(0), Some(' ' | '\t' | '\r' | '\n')) {
            let _ = self.bump();
        }
        if !self.at_inner_doc_comment() {
            self.i = mark;
            self.byte_offset = byte_mark;
            self.line = line;
            self.col = col;
        }
    }

    /// Scan a `(** … *)` doc comment (cursor at the opener) into its raw inner
    /// text with the sigil and one optional surrounding space stripped. Reuses
    /// the nesting/termination discipline of [`skip_block_comment`].
    fn scan_doc_comment(&mut self) -> TokKind {
        let start = self.pos();
        let _ = self.bump(); // (
        let _ = self.bump(); // *
        let _ = self.bump(); // *
        let mut text = String::new();
        let mut depth = 1;
        while depth > 0 {
            match (self.peek(0), self.peek(1)) {
                (Some('('), Some('*')) => {
                    text.push('(');
                    text.push('*');
                    let _ = self.bump();
                    let _ = self.bump();
                    depth += 1;
                }
                (Some('*'), Some(')')) => {
                    let _ = self.bump();
                    let _ = self.bump();
                    depth -= 1;
                    if depth > 0 {
                        text.push('*');
                        text.push(')');
                    }
                }
                (Some(c), _) => {
                    text.push(c);
                    let _ = self.bump();
                }
                (None, _) => {
                    self.error(start, "unterminated `(** … *)` doc comment");
                    break;
                }
            }
        }
        TokKind::Doc(strip_doc_lines(&text))
    }

    /// Consume a nesting `(* … *)` block comment (opener already at the cursor).
    /// An unterminated comment records a lexical error rather than looping.
    fn skip_block_comment(&mut self) {
        let start = self.pos();
        let _ = self.bump(); // (
        let _ = self.bump(); // *
        let mut depth = 1;
        while depth > 0 {
            match (self.peek(0), self.peek(1)) {
                (Some('('), Some('*')) => {
                    let _ = self.bump();
                    let _ = self.bump();
                    depth += 1;
                }
                (Some('*'), Some(')')) => {
                    let _ = self.bump();
                    let _ = self.bump();
                    depth -= 1;
                }
                (Some(_), _) => {
                    let _ = self.bump();
                }
                (None, _) => {
                    self.error(start, "unterminated `(* … *)` block comment");
                    return;
                }
            }
        }
    }

    fn scan(&mut self) -> (Vec<Token>, Vec<SyntaxError>) {
        let mut out = Vec::new();
        loop {
            let before = self.i;
            self.skip_trivia();
            if self.i >= self.chars.len() {
                break;
            }
            // Glued = no trivia was skipped, so this token abuts the previous
            // one. Meaningless for the very first token (nothing precedes it).
            let glued = self.i == before && !out.is_empty();
            let pos = self.pos();
            let start = self.byte_offset;
            if let Some(kind) = self.scan_token(pos) {
                out.push(Token {
                    kind,
                    pos,
                    glued,
                    range: start..self.byte_offset,
                });
            }
        }
        (out, std::mem::take(&mut self.errors))
    }

    fn scan_token(&mut self, pos: Position) -> Option<TokKind> {
        if self.at_doc_comment() {
            return Some(self.scan_doc_comment());
        }
        if self.at_inner_doc_comment() {
            return Some(self.scan_inner_doc_comment());
        }
        let c = self.peek(0)?;
        match c {
            '0'..='9' => Some(self.scan_number(pos)),
            '"' => Some(self.scan_string(pos)),
            c if c.is_alphabetic() || c == '_' => Some(self.scan_ident()),
            _ => self.scan_operator(pos),
        }
    }

    fn scan_number(&mut self, pos: Position) -> TokKind {
        let start = self.i;
        while matches!(self.peek(0), Some('0'..='9')) {
            let _ = self.bump();
        }
        let is_float = self.peek(0) == Some('.') && matches!(self.peek(1), Some('0'..='9'));
        if is_float {
            let _ = self.bump();
            while matches!(self.peek(0), Some('0'..='9')) {
                let _ = self.bump();
            }
        }
        let text: String = self
            .chars
            .get(start..self.i)
            .unwrap_or_default()
            .iter()
            .collect();
        if is_float {
            // [FLOAT-LITERAL-RANGE] A successful f64 parse can still be infinity.
            match text.parse::<f64>() {
                Ok(number) if number.is_finite() => TokKind::Float(number),
                _ => {
                    self.error(
                        pos,
                        format!("float literal `{text}` is outside the finite 64-bit range"),
                    );
                    TokKind::Float(0.0)
                }
            }
        } else {
            text.parse::<i64>().map_or_else(
                |_| {
                    // `i64::MIN`'s magnitude is one larger than `i64::MAX`, so
                    // it parses only as the operand of a unary minus. The
                    // parser folds `-<magnitude>` into the literal and rejects
                    // every other position, exactly as the Default frontend
                    // does ([ARITH-NEG-LITERAL], [FLAVOR-IR-EQUIV]).
                    if text == crate::I64_MIN_MAGNITUDE {
                        return TokKind::IntMinMagnitude;
                    }
                    self.error(pos, format!("invalid integer literal '{text}'"));
                    TokKind::Int(0)
                },
                TokKind::Int,
            )
        }
    }

    /// Scan a `"…"` literal, tracking `${…}` interpolation-brace depth so a
    /// nested string inside a fragment (`"v=${f "abc"}"`) does not end the outer
    /// token at the inner quote. Mirrors the brace-depth scan in
    /// [`crate::strings::lower_interpolation`]: a `"` only terminates the outer
    /// string at interpolation depth 0; at depth > 0 it opens a nested string
    /// that is consumed to its matching close quote ([FLAVOR-FRONTEND]).
    fn scan_string(&mut self, pos: Position) -> TokKind {
        let _ = self.bump(); // opening quote
        let mut raw = String::new();
        let mut interp_depth = 0i32;
        loop {
            match self.peek(0) {
                None => {
                    self.error(pos, "unterminated string literal");
                    break;
                }
                Some('"') if interp_depth == 0 => {
                    let _ = self.bump();
                    break;
                }
                // Inside `${…}` a quote opens a nested string; consume it whole
                // (honouring escapes) so its content stays in the raw token.
                Some('"') => self.scan_nested_string(pos, &mut raw),
                Some('\\') => self.copy_escape(&mut raw),
                Some(c) => {
                    if c == '{' && raw.ends_with('$') {
                        interp_depth += 1;
                    } else if c == '}' && interp_depth > 0 {
                        interp_depth -= 1;
                    }
                    let _ = self.bump();
                    raw.push(c);
                }
            }
        }
        TokKind::Str(raw)
    }

    /// Consume a nested `"…"` string appearing inside a `${…}` fragment, copying
    /// its delimiters and body (escapes preserved) verbatim into `raw`. The
    /// nested quotes never affect the outer literal's termination.
    fn scan_nested_string(&mut self, pos: Position, raw: &mut String) {
        let _ = self.bump(); // opening quote of the nested string
        raw.push('"');
        loop {
            match self.peek(0) {
                None | Some('\n') => {
                    self.error(pos, "unterminated string literal");
                    break;
                }
                Some('"') => {
                    let _ = self.bump();
                    raw.push('"');
                    break;
                }
                Some('\\') => self.copy_escape(raw),
                Some(c) => {
                    let _ = self.bump();
                    raw.push(c);
                }
            }
        }
    }

    /// Consume an escape sequence while preserving its source spelling.
    fn copy_escape(&mut self, raw: &mut String) {
        let _ = self.bump();
        raw.push('\\');
        if let Some(escaped) = self.bump() {
            raw.push(escaped);
        }
    }

    fn scan_ident(&mut self) -> TokKind {
        let start = self.i;
        while matches!(self.peek(0), Some(c) if c.is_alphanumeric() || c == '_') {
            let _ = self.bump();
        }
        let text: String = self
            .chars
            .get(start..self.i)
            .unwrap_or_default()
            .iter()
            .collect();
        keyword_or_ident(&text)
    }

    fn scan_operator(&mut self, pos: Position) -> Option<TokKind> {
        let c = self.peek(0)?;
        let next = self.peek(1);
        if let Some(kind) = two_char_operator(c, next) {
            let _ = self.bump();
            let _ = self.bump();
            return Some(kind);
        }
        let kind = single_char_operator(c);
        let _ = self.bump();
        if kind.is_none() {
            self.error(pos, format!("unexpected character '{c}'"));
        }
        kind
    }
}
