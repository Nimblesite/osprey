//! Tokens of the Lean fragment. Comments vanish here; documentation comments
//! survive because a definition carries one into the generated module.

use crate::refusal::{Code, Refusal};

/// One token kind. Anything the fragment has no use for is `Other`, so a
/// skipped command (a theorem, an `#eval`) lexes whatever it contains.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tok {
    /// An identifier, dotted components included (`Float.sqrt`, `x.sqrt`).
    Ident(String),
    /// A numeric literal as written.
    Num(String),
    /// The text of a `/-- … -/` documentation comment.
    Doc(String),
    /// A `#` command such as `#eval`.
    Hash(String),
    /// A `.name` written directly after a closing parenthesis.
    Field(String),
    /// An operator or punctuation mark.
    Sym(&'static str),
    /// A string, a character or any other mark.
    Other,
}

/// A token with the line (one-based) and column (zero-based) it starts at,
/// and the place just past it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) tok: Tok,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) end: (usize, usize),
}

/// Marks longest first, so `:=` wins over `:` and `<=` over `<`.
const SYMBOLS: &[&str] = &[
    ":=", "==", "!=", "<=", ">=", "&&", "||", "@[", "(", ")", ":", ",", ";", "+", "-", "*", "/",
    "^", "<", ">", "=", "!", "≤", "≥", "≠", "∧", "∨", "¬",
];

struct Lexer<'a> {
    rest: &'a str,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
}

/// Split a Lean source into the fragment's tokens.
///
/// # Errors
/// An unclosed comment, which leaves the rest of the file unreadable.
pub(crate) fn lex(source: &str) -> Result<Vec<Token>, Refusal> {
    let mut lexer = Lexer {
        rest: source,
        line: 1,
        column: 0,
        tokens: Vec::new(),
    };
    while let Some(first) = lexer.skip_blank()? {
        lexer.token(first);
    }
    Ok(lexer.tokens)
}

fn ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn ident_part(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\''
}

impl<'a> Lexer<'a> {
    fn place(&self) -> (usize, usize) {
        (self.line, self.column)
    }

    /// Consume `text`, which is a prefix of what remains.
    fn advance(&mut self, text: &'a str) {
        for c in text.chars() {
            if c == '\n' {
                self.line += 1;
                self.column = 0;
            } else {
                self.column += 1;
            }
        }
        self.rest = self.rest.strip_prefix(text).unwrap_or(self.rest);
    }

    fn take_while(&mut self, keep: impl Fn(char) -> bool) -> &'a str {
        let end = self.rest.find(|c| !keep(c)).unwrap_or(self.rest.len());
        let (taken, _) = self.rest.split_at(end);
        self.advance(taken);
        taken
    }

    /// Skip whitespace and comments; the character the next token starts
    /// with, or nothing at end of input.
    fn skip_blank(&mut self) -> Result<Option<char>, Refusal> {
        loop {
            let _ = self.take_while(char::is_whitespace);
            if self.rest.starts_with("--") {
                let _ = self.take_while(|c| c != '\n');
            } else if self.rest.starts_with("/-") {
                self.block_comment()?;
            } else {
                return Ok(self.rest.chars().next());
            }
        }
    }

    /// A block comment, nesting as Lean's do. `/-- … -/` becomes a `Doc` token.
    fn block_comment(&mut self) -> Result<(), Refusal> {
        let (place, body) = (self.place(), self.rest);
        let mut depth = 0_usize;
        loop {
            if self.rest.starts_with("/-") {
                depth += 1;
                self.advance("/-");
            } else if self.rest.starts_with("-/") {
                depth -= 1;
                self.advance("-/");
            } else if !self.skip_char() {
                let fix = "this comment never closes; end it with `-/`";
                return Err(Refusal::at(Code::UnexpectedToken, place, fix));
            }
            if depth == 0 {
                self.documentation(place, body);
                return Ok(());
            }
        }
    }

    /// Consume one character; false at end of input.
    fn skip_char(&mut self) -> bool {
        let mut chars = self.rest.chars();
        let found = chars.next().is_some();
        let (head, _) = self.rest.split_at(self.rest.len() - chars.as_str().len());
        self.advance(head);
        found
    }

    fn documentation(&mut self, place: (usize, usize), body: &'a str) {
        let (comment, _) = body.split_at(body.len() - self.rest.len());
        let text = comment
            .strip_prefix("/--")
            .and_then(|text| text.strip_suffix("-/"));
        if let Some(text) = text {
            self.push(place, Tok::Doc(lines(text)));
        }
    }

    fn push(&mut self, place: (usize, usize), tok: Tok) {
        let end = self.place();
        self.tokens.push(Token {
            tok,
            line: place.0,
            column: place.1,
            end,
        });
    }

    fn token(&mut self, first: char) {
        let place = self.place();
        let tok = if ident_start(first) {
            Tok::Ident(self.dotted())
        } else if first.is_ascii_digit() {
            Tok::Num(self.number())
        } else if first == '#' {
            self.advance("#");
            Tok::Hash(self.take_while(ident_part).to_owned())
        } else {
            self.mark(first)
        };
        self.push(place, tok);
    }

    /// An identifier with every `.component` that follows it directly.
    fn dotted(&mut self) -> String {
        let mut name = self.take_while(ident_part).to_owned();
        while self
            .rest
            .strip_prefix('.')
            .is_some_and(|after| after.starts_with(ident_start))
        {
            self.advance(".");
            name.push('.');
            name.push_str(self.take_while(ident_part));
        }
        name
    }

    /// Digits, an optional fraction and an optional exponent, plus any letters
    /// run on behind them so `0x1F` reaches the reader whole and is refused.
    fn number(&mut self) -> String {
        let mut text = self
            .take_while(|c| c.is_ascii_alphanumeric() || c == '_')
            .to_owned();
        if self
            .rest
            .strip_prefix('.')
            .is_some_and(|after| after.starts_with(|c: char| c.is_ascii_digit()))
        {
            self.advance(".");
            text.push('.');
            text.push_str(self.take_while(|c| c.is_ascii_alphanumeric() || c == '_'));
        }
        if text.ends_with(['e', 'E']) && self.rest.starts_with(['+', '-']) {
            text.push_str(self.take_while(|c| c == '+' || c == '-'));
            text.push_str(self.take_while(|c| c.is_ascii_digit()));
        }
        text
    }

    fn mark(&mut self, first: char) -> Tok {
        if first == '.' && self.after_paren() {
            self.advance(".");
            return Tok::Field(self.take_while(ident_part).to_owned());
        }
        if let Some(symbol) = SYMBOLS
            .iter()
            .find(|symbol| self.rest.starts_with(**symbol))
        {
            self.advance(symbol);
            return Tok::Sym(symbol);
        }
        if first == '"' {
            self.string();
        } else {
            let _ = self.skip_char();
        }
        Tok::Other
    }

    /// Whether a `.name` here continues a parenthesised term: no space between.
    fn after_paren(&self) -> bool {
        let field = self
            .rest
            .strip_prefix('.')
            .is_some_and(|after| after.starts_with(ident_start));
        let last = self.tokens.last();
        field
            && last.is_some_and(|token| {
                token.tok == Tok::Sym(")")
                    && token.line == self.line
                    && token.column + 1 == self.column
            })
    }

    /// A string literal, escapes included, so a `--` inside it is not a comment.
    fn string(&mut self) {
        self.advance("\"");
        loop {
            let _ = self.take_while(|c| c != '"' && c != '\\');
            if self.rest.starts_with('\\') {
                self.advance("\\");
                let _ = self.skip_char();
            } else {
                self.advance("\"");
                return;
            }
        }
    }
}

/// A documentation comment's lines without the indentation they share, so a
/// `# Examples` section keeps its fences and code layout.
fn lines(text: &str) -> String {
    let rows: Vec<&str> = text.lines().map(str::trim_end).collect();
    let indent = rows
        .iter()
        .skip(1)
        .filter(|row| !row.is_empty())
        .map(|row| row.len() - row.trim_start().len())
        .min()
        .unwrap_or(0);
    let body: Vec<&str> = rows
        .iter()
        .enumerate()
        .map(|(index, row)| match index {
            0 => row.trim_start(),
            _ => row.get(indent..).unwrap_or_else(|| row.trim_start()),
        })
        .collect();
    body.join("\n").trim().to_string()
}
