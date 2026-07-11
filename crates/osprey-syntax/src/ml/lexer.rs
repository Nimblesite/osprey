//! The ML-flavor lexer: a hand-written scanner that turns source text into a
//! flat [`Token`] stream, then derives the layout markers (`Indent`, `Dedent`,
//! `Newline`) from the offside rule ([FLAVOR-ML-LAYOUT]).
//!
//! Two phases keep each piece small and testable: [`scan`] produces content
//! tokens with positions (no layout), and [`insert_layout`] walks those tokens
//! and inserts the layout markers from each line's first-token column, with
//! bracket depth suppressing layout inside `( … )`.
//!
//! ESCAPE HATCH: if this hand-written layout frontend becomes onerous or
//! accrues parsing bugs we cannot tame, we fall back to a `tree-sitter-osprey-ml`
//! grammar with an external INDENT/DEDENT/NEWLINE scanner.c — the boundary law
//! makes the parser mechanism a flavor-internal swap (docs/specs/0023).

use super::token::{keyword_or_ident, TokKind, Token};
use crate::SyntaxError;
use osprey_ast::Position;

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
    line: u32,
    col: u32,
    errors: Vec<SyntaxError>,
}

impl Scanner {
    fn new(source: &str) -> Self {
        Scanner {
            chars: source.chars().collect(),
            i: 0,
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
                '/' if self.peek(1) == Some('/') => {
                    while !matches!(self.peek(0), Some('\n') | None) {
                        let _ = self.bump();
                    }
                }
                '(' if self.peek(1) == Some('*') => self.skip_block_comment(),
                _ => break,
            }
        }
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
            if let Some(kind) = self.scan_token(pos) {
                out.push(Token { kind, pos, glued });
            }
        }
        (out, std::mem::take(&mut self.errors))
    }

    fn scan_token(&mut self, pos: Position) -> Option<TokKind> {
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
            text.parse::<f64>().map_or_else(
                |_| {
                    self.error(pos, format!("invalid float literal '{text}'"));
                    TokKind::Float(0.0)
                },
                TokKind::Float,
            )
        } else {
            text.parse::<i64>().map_or_else(
                |_| {
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
                // A newline only ends the outer literal outside interpolation;
                // a `${…}` fragment may legitimately span the line break.
                Some('\n') if interp_depth == 0 => {
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
                Some('\\') => {
                    let _ = self.bump();
                    raw.push('\\');
                    if let Some(escaped) = self.bump() {
                        raw.push(escaped);
                    }
                }
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
                Some('\\') => {
                    let _ = self.bump();
                    raw.push('\\');
                    if let Some(escaped) = self.bump() {
                        raw.push(escaped);
                    }
                }
                Some(c) => {
                    let _ = self.bump();
                    raw.push(c);
                }
            }
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

/// Match a two-character operator/punctuation lexeme.
fn two_char_operator(c: char, next: Option<char>) -> Option<TokKind> {
    let next = next?;
    let kind = match (c, next) {
        (':', '=') => TokKind::ColonEq,
        ('-', '>') => TokKind::Arrow,
        ('=', '>') => TokKind::FatArrow,
        ('=', '=') => TokKind::Op("==".to_owned()),
        ('!', '=') => TokKind::Op("!=".to_owned()),
        ('<', '=') => TokKind::Op("<=".to_owned()),
        ('>', '=') => TokKind::Op(">=".to_owned()),
        ('&', '&') => TokKind::Op("&&".to_owned()),
        ('|', '|') => TokKind::Op("||".to_owned()),
        ('|', '>') => TokKind::Op("|>".to_owned()),
        _ => return None,
    };
    Some(kind)
}

/// Match a single-character operator/punctuation lexeme.
fn single_char_operator(c: char) -> Option<TokKind> {
    let kind = match c {
        '=' => TokKind::Eq,
        ':' => TokKind::Colon,
        '\\' => TokKind::Backslash,
        '(' => TokKind::LParen,
        ')' => TokKind::RParen,
        '[' => TokKind::LBracket,
        ']' => TokKind::RBracket,
        ',' => TokKind::Comma,
        '.' => TokKind::Dot,
        '+' | '-' | '*' | '/' | '%' | '<' | '>' | '!' => TokKind::Op(c.to_string()),
        _ => return None,
    };
    Some(kind)
}

/// Phase 2: insert `Indent`/`Dedent`/`Newline` from each line's first-token
/// column. Layout is suppressed while bracket depth is non-zero so a
/// parenthesised expression may span lines. Implements [FLAVOR-ML-LAYOUT].
fn insert_layout(content: Vec<Token>) -> (Vec<Token>, Vec<SyntaxError>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let mut stack = vec![0u32];
    let mut depth = 0i32;
    let mut prev_line = 0u32;
    let mut started = false;
    for tok in content {
        if depth == 0 && tok.pos.line != prev_line {
            emit_layout(&mut out, &mut stack, &mut errors, tok.pos, started);
        }
        match tok.kind {
            TokKind::LParen | TokKind::LBracket => depth += 1,
            TokKind::RParen | TokKind::RBracket => depth = (depth - 1).max(0),
            _ => {}
        }
        prev_line = tok.pos.line;
        out.push(tok);
        started = true;
    }
    let close = out.last().map_or(Position::default(), |t| t.pos);
    while stack.last().copied().unwrap_or(0) > 0 {
        let _ = stack.pop();
        out.push(layout_tok(TokKind::Dedent, close));
    }
    out.push(layout_tok(TokKind::Eof, close));
    (out, errors)
}

/// Compare a logical line's indentation against the stack, pushing one `Indent`,
/// a run of `Dedent`s, or a separating `Newline`.
fn emit_layout(
    out: &mut Vec<Token>,
    stack: &mut Vec<u32>,
    errors: &mut Vec<SyntaxError>,
    pos: Position,
    started: bool,
) {
    let col = pos.column;
    let top = stack.last().copied().unwrap_or(0);
    if col > top {
        stack.push(col);
        out.push(layout_tok(TokKind::Indent, pos));
        return;
    }
    while col < stack.last().copied().unwrap_or(0) {
        let _ = stack.pop();
        out.push(layout_tok(TokKind::Dedent, pos));
    }
    if col == stack.last().copied().unwrap_or(0) {
        if started {
            out.push(layout_tok(TokKind::Newline, pos));
        }
    } else {
        errors.push(SyntaxError {
            message: "inconsistent indentation does not match any enclosing block".to_owned(),
            position: pos,
        });
        stack.push(col);
        out.push(layout_tok(TokKind::Indent, pos));
    }
}

fn layout_tok(kind: TokKind, pos: Position) -> Token {
    // Synthetic layout markers never abut a content token meaningfully.
    Token {
        kind,
        pos,
        glued: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokKind> {
        let (tokens, errors) = lex(source);
        assert!(errors.is_empty(), "lex errors: {errors:?}");
        tokens.into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexes_binding_with_no_layout() {
        let k = kinds("x = 42\n");
        assert_eq!(
            k,
            vec![
                TokKind::Ident("x".to_owned()),
                TokKind::Eq,
                TokKind::Int(42),
                TokKind::Eof,
            ]
        );
    }

    #[test]
    fn separates_top_level_lines_with_newline() {
        let k = kinds("a = 1\nb = 2\n");
        let newlines = k.iter().filter(|t| **t == TokKind::Newline).count();
        assert_eq!(newlines, 1, "one separator between two top-level bindings");
    }

    #[test]
    fn indents_and_dedents_a_block() {
        let k = kinds("f =\n    g\nh = 1\n");
        assert!(k.contains(&TokKind::Indent), "block opens with Indent");
        assert!(k.contains(&TokKind::Dedent), "block closes with Dedent");
        // The Dedent must precede the sibling `h` binding.
        let dedent = k.iter().position(|t| *t == TokKind::Dedent);
        let h = k.iter().position(|t| *t == TokKind::Ident("h".to_owned()));
        assert!(dedent.is_some(), "expected a Dedent token");
        assert!(h.is_some(), "expected the `h` binding token");
        // Both positions are asserted present above, so this guard always binds;
        // it avoids the forbidden `unwrap()` ([USER-MANDATE-NO-PANIC-IN-TESTS]).
        if let (Some(dedent), Some(h)) = (dedent, h) {
            assert!(dedent < h);
        }
    }

    #[test]
    fn suppresses_layout_inside_parentheses() {
        // A line break inside parens must not start a new statement.
        let k = kinds("x = (1 +\n2)\n");
        assert!(!k.contains(&TokKind::Indent), "no layout inside parens");
    }

    #[test]
    fn ignores_blank_and_comment_lines() {
        let k = kinds("a = 1\n\n// note\nb = 2\n");
        let newlines = k.iter().filter(|t| **t == TokKind::Newline).count();
        assert_eq!(newlines, 1, "blank/comment lines are not separators");
    }

    #[test]
    fn lexes_curried_application_and_operators() {
        let k = kinds("r = add 1 2 == 3\n");
        assert!(k.contains(&TokKind::Op("==".to_owned())));
        assert!(k.contains(&TokKind::Int(1)) && k.contains(&TokKind::Int(2)));
    }

    #[test]
    fn reports_unterminated_string() {
        let (_, errors) = lex("x = \"oops\n");
        assert!(errors.iter().any(|e| e.message.contains("unterminated")));
    }

    #[test]
    fn skips_nesting_block_comments() {
        // `(* … *)` block comments (ML-family convention) are trivia and NEST,
        // so an inner `(* *)` does not close the outer early. The binding
        // survives with no stray tokens.
        let k = kinds("x = (* outer (* inner *) still *) 42\n");
        assert_eq!(
            k,
            vec![
                TokKind::Ident("x".to_owned()),
                TokKind::Eq,
                TokKind::Int(42),
                TokKind::Eof,
            ]
        );
    }

    #[test]
    fn reports_unterminated_block_comment() {
        let (_, errors) = lex("x = (* never closed\n");
        assert!(errors
            .iter()
            .any(|e| e.message.contains("unterminated `(* … *)` block comment")));
    }

    #[test]
    fn nested_string_in_interpolation_is_one_token() {
        // The inner quotes around `a` sit inside a `${…}` fragment; they must not
        // terminate the outer literal, which lexes as a SINGLE string token whose
        // raw still carries the unresolved `${ f "a" }` span.
        let (tokens, errors) = lex("x = \"v=${f \"a\"}\"\n");
        assert!(errors.is_empty(), "lex errors: {errors:?}");
        let strings: Vec<&String> = tokens
            .iter()
            .filter_map(|t| match &t.kind {
                TokKind::Str(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(
            strings,
            vec![&"v=${f \"a\"}".to_owned()],
            "exactly one string token carrying the unresolved fragment",
        );
    }
}
