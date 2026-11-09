//! Operator spellings the scanner recognises.

use crate::ml::token::TokKind;

/// Match a two-character operator/punctuation lexeme.
pub(super) fn two_char_operator(c: char, next: Option<char>) -> Option<TokKind> {
    let next = next?;
    let kind = match (c, next) {
        (':', '=') => TokKind::ColonEq,
        (':', ':') => TokKind::ColonColon,
        ('-', '>') => TokKind::Arrow,
        ('=', '>') => TokKind::FatArrow,
        ('=', '=') => TokKind::Op("==".to_owned()),
        ('!', '=') => TokKind::Op("!=".to_owned()),
        ('<', '=') => TokKind::Op("<=".to_owned()),
        ('>', '=') => TokKind::Op(">=".to_owned()),
        ('&', '&') => TokKind::Op("&&".to_owned()),
        ('|', '|') => TokKind::Op("||".to_owned()),
        ('|', '>') => TokKind::Op("|>".to_owned()),
        // Matched here, ahead of the bare `:` of a type signature, so `?:`
        // keeps maximal munch ([PATTERN-RESULT-DEFAULT]). A lone `?` still has
        // no ML meaning and stays a lexical error.
        ('?', ':') => TokKind::Op(crate::ml::parser::ELVIS_OP.to_owned()),
        _ => return None,
    };
    Some(kind)
}

/// Match a single-character operator/punctuation lexeme.
pub(super) fn single_char_operator(c: char) -> Option<TokKind> {
    let kind = match c {
        '=' => TokKind::Eq,
        ':' => TokKind::Colon,
        '\\' => TokKind::Backslash,
        '(' => TokKind::LParen,
        ')' => TokKind::RParen,
        '[' => TokKind::LBracket,
        ']' => TokKind::RBracket,
        '{' => TokKind::LBrace,
        '}' => TokKind::RBrace,
        ',' => TokKind::Comma,
        '.' => TokKind::Dot,
        // Reached only after `two_char_operator` has declined, so `||` and `|>`
        // keep maximal munch ([FLAVOR-ML-UNION-INLINE]).
        '|' => TokKind::Pipe,
        '+' | '-' | '*' | '/' | '%' | '<' | '>' | '!' => TokKind::Op(c.to_string()),
        _ => return None,
    };
    Some(kind)
}
