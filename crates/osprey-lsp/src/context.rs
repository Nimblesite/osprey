//! Where the cursor is, syntactically.
//!
//! Completion used to answer the same list everywhere: a type annotation was
//! offered the `fn` snippet, and an argument slot was offered `namespace`. Both
//! insert source the parser rejects, which is worse than offering nothing. This
//! module classifies the cursor from the text before it so
//! [`crate::complete::completion`] can offer only what is legal there.
//! Implements [LSP-COMPLETION-CONTEXT].
//!
//! The classification is **lexical, not semantic** — it reads the scrubbed
//! prefix rather than the tree, because the buffer under an editing cursor is
//! usually not parsable. Every rule below is therefore a documented heuristic,
//! and each falls back to [`Cursor::Declaration`] (the unfiltered answer) rather
//! than to silence.

use lspkit_vfs::PositionEncoding;

use crate::text::prefix_to;

/// What may legally be written at the cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cursor {
    /// Directly after `receiver.` — only that value's fields belong here.
    Member(String),
    /// Inside a written type, after `:` or `->`.
    Type,
    /// Left of a match arm's `=>` — constructors, not values.
    Pattern,
    /// A declaration's parameter name. A fresh binder has nothing to complete.
    Binder,
    /// A value expression: after `=`, `=>`, `(`, `,`, or an operator.
    Value,
    /// Statement/declaration position — everything the flavor has.
    Declaration,
}

/// Classify the cursor at `(line, character)` in `text`.
#[must_use]
pub fn at(text: &str, line: u32, character: u32, encoding: PositionEncoding) -> Cursor {
    classify(&scrub(&document_prefix(text, line, character, encoding)))
}

/// Everything before the cursor: whole earlier lines plus the current line up
/// to the caret. Completion is decided by what is already written, never by the
/// half-typed text to the right of it.
fn document_prefix(text: &str, line: u32, character: u32, encoding: PositionEncoding) -> String {
    let index = usize::try_from(line).unwrap_or(usize::MAX);
    let mut prefix: Vec<&str> = text.lines().take(index).collect();
    prefix.push(prefix_to(
        text.lines().nth(index).unwrap_or(""),
        character,
        encoding,
    ));
    prefix.join("\n")
}

/// The prefix with string literals and comments blanked out, so their
/// punctuation cannot be mistaken for structure: a `:` inside a string is not
/// an ascription, and a `.` inside a comment is not a field access. Newlines
/// survive, because the layout rules below read them.
fn scrub(prefix: &str) -> String {
    let src: Vec<char> = prefix.chars().collect();
    let mut out = String::with_capacity(prefix.len());
    let mut at = 0usize;
    while at < src.len() {
        at = match (src.get(at).copied(), src.get(at.saturating_add(1)).copied()) {
            (Some('/'), Some('/')) => skip_line(&src, at, &mut out),
            (Some('('), Some('*')) => skip_block(&src, at),
            (Some('"'), _) => skip_string(&src, at),
            (Some(c), _) => {
                out.push(c);
                at.saturating_add(1)
            }
            (None, _) => src.len(),
        };
    }
    out
}

/// Drop a `//` comment, keeping the newline that ends it.
fn skip_line(src: &[char], from: usize, out: &mut String) -> usize {
    match src.iter().skip(from).position(|c| *c == '\n') {
        Some(offset) => {
            out.push('\n');
            from.saturating_add(offset).saturating_add(1)
        }
        None => src.len(),
    }
}

/// Drop a `(* … *)` comment. ML nests them, so this counts depth rather than
/// stopping at the first `*)`.
fn skip_block(src: &[char], from: usize) -> usize {
    let mut depth = 0usize;
    let mut at = from;
    while at < src.len() {
        match (src.get(at), src.get(at.saturating_add(1))) {
            (Some('('), Some('*')) => {
                depth = depth.saturating_add(1);
                at = at.saturating_add(2);
            }
            (Some('*'), Some(')')) => {
                depth = depth.saturating_sub(1);
                at = at.saturating_add(2);
                if depth == 0 {
                    return at;
                }
            }
            _ => at = at.saturating_add(1),
        }
    }
    at
}

/// Drop a `"…"` literal, honouring `\` escapes. Interpolation holes go with it:
/// a half-typed `"${` is not a position this module claims to understand.
fn skip_string(src: &[char], from: usize) -> usize {
    let mut at = from.saturating_add(1);
    while at < src.len() {
        match src.get(at) {
            Some('\\') => at = at.saturating_add(2),
            Some('"') => return at.saturating_add(1),
            _ => at = at.saturating_add(1),
        }
    }
    at
}

fn classify(code: &str) -> Cursor {
    // The partial word under the caret is what is being completed, not a cue.
    let head = code.trim_end_matches(is_word);
    if let Some(receiver) = member_receiver(head) {
        return Cursor::Member(receiver);
    }
    match last_cue(head) {
        Cue::Type => Cursor::Type,
        // A `match` arm opens under either cue: Default writes `match x {`
        // (a statement cue) and ML writes a bare `match x` whose arms follow
        // the layout, so the last cue is whatever preceded the scrutinee.
        Cue::Value | Cue::Statement if in_match_arm(head) => Cursor::Pattern,
        Cue::Value if in_parameter_list(head) => Cursor::Binder,
        Cue::Value => Cursor::Value,
        Cue::Statement => Cursor::Declaration,
    }
}

/// The receiver of a field access when the cursor follows `receiver.`. A float
/// literal (`1.`) and a `..` range have no receiver, so neither is a member
/// position. Implements [LSP-COMPLETION-MEMBER].
fn member_receiver(head: &str) -> Option<String> {
    let body = head.strip_suffix('.')?;
    let receiver: String = trailing_word(body);
    let leads = receiver.chars().next()?;
    leads.is_alphabetic().then_some(receiver)
}

/// The identifier that `text` ends with, empty when it ends in anything else.
fn trailing_word(text: &str) -> String {
    let mut word: Vec<char> = text.chars().rev().take_while(|c| is_word(*c)).collect();
    word.reverse();
    word.into_iter().collect()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The syntactic cue nearest the cursor — the last token that decides what may
/// follow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cue {
    /// `:` or `->` — a type follows.
    Type,
    /// `=`, `=>`, `(`, `,`, `)` or an operator — an expression follows.
    Value,
    /// `{`, `}`, or a newline starting an unindented line — a declaration or
    /// statement follows. Also the state before anything has been written.
    Statement,
}

fn last_cue(head: &str) -> Cue {
    let src: Vec<char> = head.chars().collect();
    let mut cue = Cue::Statement;
    let mut at = 0usize;
    while at < src.len() {
        let (found, step) = cue_at(&src, at);
        cue = found.unwrap_or(cue);
        at = at.saturating_add(step);
    }
    cue
}

/// The cue starting at `at`, and how many characters it spans. `None` leaves
/// the running cue alone — most characters decide nothing.
fn cue_at(src: &[char], at: usize) -> (Option<Cue>, usize) {
    let next = src.get(at.saturating_add(1)).copied();
    match (src.get(at).copied(), next) {
        (Some('-'), Some('>')) => (Some(Cue::Type), 2),
        // `::` is a path separator and `==`/`=>` are operators — none of the
        // three is the `:` ascription or the `=` that opens a body.
        (Some(':'), Some(':')) => (None, 2),
        (Some('='), Some('=' | '>')) => (Some(Cue::Value), 2),
        (Some(':'), _) => (Some(Cue::Type), 1),
        (Some('{' | '}' | ';'), _) => (Some(Cue::Statement), 1),
        // A newline only re-opens declaration position when the line it starts
        // is unindented. Both flavors indent a continuation — ML by the offside
        // rule — so an indented line is still inside whatever cue preceded it.
        (Some('\n'), next) if next.is_none_or(|c| !c.is_whitespace()) => (Some(Cue::Statement), 1),
        // Openers and operators: an expression may follow any of them.
        (Some('=' | '(' | ')' | ',' | '[' | ']'), _) => (Some(Cue::Value), 1),
        (Some('+' | '-' | '*' | '/' | '%' | '<' | '>' | '!' | '&' | '|' | '?'), _) => {
            (Some(Cue::Value), 1)
        }
        _ => (None, 1),
    }
}

/// True when the cursor sits where a `match` arm's **pattern** goes: the
/// nearest enclosing line is a `match` and no `=>` has been written yet, so the
/// arm's left-hand side is still open. Indentation carries the nesting in both
/// flavors — braces are optional in Default and absent in ML — so this reads
/// the layout rather than the delimiters.
fn in_match_arm(head: &str) -> bool {
    let line = current_line(head);
    if line.contains("=>") {
        return false;
    }
    let indent = indent_of(line);
    head.lines()
        .rev()
        .skip(1)
        .filter(|previous| !previous.trim().is_empty())
        .find(|previous| indent_of(previous) < indent)
        .is_some_and(|opener| opener.contains("match ") || opener.contains("select"))
}

/// True when the cursor is inside a declaration head's parameter list, where
/// the author is naming a fresh binder rather than referring to anything.
fn in_parameter_list(head: &str) -> bool {
    let line = current_line(head).trim_start();
    let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
    let declares = ["fn ", "extern fn ", "type ", "effect "]
        .iter()
        .any(|keyword| line.starts_with(keyword));
    declares && open_parens(line) > 0
}

fn open_parens(line: &str) -> usize {
    line.chars().fold(0usize, |depth, c| match c {
        '(' => depth.saturating_add(1),
        ')' => depth.saturating_sub(1),
        _ => depth,
    })
}

fn current_line(head: &str) -> &str {
    head.rsplit('\n').next().unwrap_or(head)
}

fn indent_of(line: &str) -> usize {
    line.len().saturating_sub(line.trim_start().len())
}

#[cfg(test)]
mod tests {
    use super::*;
    const U16: PositionEncoding = PositionEncoding::Utf16;

    /// Classify the cursor at the end of `src` — the position an editor asks
    /// about while the author is typing.
    fn end_of(src: &str) -> Cursor {
        // `split` — not `lines` — so a trailing newline puts the cursor on the
        // fresh line it opened, which is exactly where an author types next.
        let rows: Vec<&str> = src.split('\n').collect();
        let line = u32::try_from(rows.len().saturating_sub(1)).unwrap_or(0);
        let column = u32::try_from(rows.last().unwrap_or(&"").chars().count()).unwrap_or(0);
        at(src, line, column, U16)
    }

    #[test]
    fn a_type_annotation_is_a_type_position_not_a_declaration_one() {
        // The bug this encodes: completion offered the `fn` snippet here, which
        // expands to a whole function declaration inside a type annotation.
        assert_eq!(end_of("fn add(a: "), Cursor::Type);
        assert_eq!(end_of("fn add(a: int, b: int) -> "), Cursor::Type);
        assert_eq!(end_of("let total: "), Cursor::Type);
        // A partially typed type name is still a type position.
        assert_eq!(end_of("let total: in"), Cursor::Type);
    }

    #[test]
    fn a_body_and_an_argument_slot_are_value_positions() {
        assert_eq!(end_of("fn add(a: int, b: int) -> int = "), Cursor::Value);
        assert_eq!(end_of("let total = add("), Cursor::Value);
        assert_eq!(end_of("let total = add(1, "), Cursor::Value);
        assert_eq!(end_of("let total = 1 + "), Cursor::Value);
        // A value position stays one however much declaration text precedes it
        // — including a `type … = A | B` union, whose `|` must not read as an
        // unclosed cue. Regression: the whole file was reclassified as a
        // declaration position and completion offered `fn` after `let m = `.
        assert_eq!(
            end_of(concat!(
                "type Shape = Circle | Square\n",
                "fn area(r) = r * r\n",
                "fn perimeter(r) = r + r + r + r\n",
                "let radius = 5\n",
                "let a = area(radius)\n",
                "let b = area(10)\n",
                "let c = perimeter(radius)\n",
                "let names = List()\n",
                "let count = listLength(names)\n",
                "let m = p"
            )),
            Cursor::Value
        );
    }

    #[test]
    fn a_fresh_line_at_column_zero_reopens_declaration_position() {
        // Without this the `=` from the previous declaration would still be the
        // nearest cue, and the next line would never be offered `fn` again.
        assert_eq!(end_of("fn f() = 1\n"), Cursor::Declaration);
        assert_eq!(end_of("fn f() = 1\nl"), Cursor::Declaration);
        // An *indented* line continues whatever the previous cue opened, which
        // is how a wrapped expression and every ML layout block are written.
        assert_eq!(end_of("let total =\n    "), Cursor::Value);
    }

    #[test]
    fn a_field_access_is_a_member_position_but_a_float_is_not() {
        assert_eq!(
            end_of("let x = origin."),
            Cursor::Member("origin".to_owned())
        );
        assert_eq!(
            end_of("let x = origin.na"),
            Cursor::Member("origin".to_owned())
        );
        // `1.` is a float literal, and `::` is the module path separator —
        // neither is a receiver, so neither may swallow the completion list.
        assert_eq!(end_of("let x = 1."), Cursor::Value);
        assert_eq!(end_of("let x = sales::Tax::ra"), Cursor::Value);
    }

    #[test]
    fn a_match_arm_starts_in_pattern_position_and_ends_in_value_position() {
        // Indentation carries the nesting: Default's braces are optional and ML
        // has none at all, so both spellings must classify the same way.
        assert_eq!(end_of("fn f(x) = match x {\n    "), Cursor::Pattern);
        assert_eq!(end_of("f x = match x\n    "), Cursor::Pattern);
        // Once `=>` is written the arm's right-hand side is a value.
        assert_eq!(
            end_of("fn f(x) = match x {\n    Some(n) => "),
            Cursor::Value
        );
        // A block that is not a `match` is ordinary statement position.
        assert_eq!(end_of("fn f() = {\n    "), Cursor::Declaration);
    }

    #[test]
    fn a_declarations_parameter_name_has_nothing_to_complete() {
        // The author is inventing a name here; every suggestion is noise.
        assert_eq!(end_of("fn add("), Cursor::Binder);
        assert_eq!(end_of("fn add(a: int, "), Cursor::Binder);
        // A *call* argument at the same depth is a value position, because the
        // line is not a declaration head.
        assert_eq!(end_of("let t = add("), Cursor::Value);
    }

    #[test]
    fn punctuation_inside_strings_and_comments_never_moves_the_cursor() {
        // A `:` in a string used to read as an ascription, and a `.` in a
        // comment as a field access — both hijacked the whole completion list.
        assert_eq!(end_of("let s = \"a: b\"\nfn f() = "), Cursor::Value);
        assert_eq!(end_of("// note: origin.field\nl"), Cursor::Declaration);
        assert_eq!(end_of("(* ml: origin.field *)\nl"), Cursor::Declaration);
        // An escaped quote must not end the literal early.
        assert_eq!(end_of("let s = \"a\\\"b: c\"\nx = "), Cursor::Value);
    }

    #[test]
    fn a_cursor_past_the_end_of_the_document_still_classifies() {
        // Editors do ask about positions the buffer no longer has.
        assert_eq!(at("fn f() = 1\n", 99, 0, U16), Cursor::Declaration);
        assert_eq!(at("", 0, 0, U16), Cursor::Declaration);
    }
}
