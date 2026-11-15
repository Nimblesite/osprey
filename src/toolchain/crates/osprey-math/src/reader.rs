//! The reader's cursor and scope, shared by the command and term grammars.

use crate::lex::{Tok, Token};
use crate::model::Ty;
use crate::refusal::{Code, Refusal};

/// The declared shape of a definition already read.
#[derive(Debug, Clone)]
pub(crate) struct Signature {
    pub(crate) name: String,
    pub(crate) params: Vec<Ty>,
    pub(crate) result: Ty,
}

/// The words that begin a command of the fragment. Any other command is
/// refused by the word the source itself uses, so no vocabulary beyond
/// definitions lives here ([TYPES-DISCIPLINE-REGISTRY]).
const COMMANDS: &[&str] = &["def", "private", "namespace", "end"];

/// Words that are part of a term's own grammar.
const TERM_WORDS: &[&str] = &[
    "if", "then", "else", "let", "fun", "match", "with", "do", "by",
];

#[derive(Debug)]
pub(crate) struct Reader<'a> {
    pub(crate) tokens: &'a [Token],
    pub(crate) at: usize,
    pub(crate) defs: Vec<Signature>,
    pub(crate) locals: Vec<(String, Ty)>,
    /// The column of each `let` whose value is being read, innermost last.
    pub(crate) let_columns: Vec<usize>,
    /// The Lean namespace the definitions are in, for qualified references.
    pub(crate) namespace: Option<String>,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(tokens: &'a [Token]) -> Self {
        Self {
            tokens,
            at: 0,
            defs: Vec::new(),
            locals: Vec::new(),
            let_columns: Vec::new(),
            namespace: None,
        }
    }

    pub(crate) fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.at)
    }

    /// The token just consumed.
    pub(crate) fn previous(&self) -> Option<&'a Token> {
        self.at.checked_sub(1).and_then(|at| self.tokens.get(at))
    }

    /// Where the next token starts, or just past the last one at end of input.
    pub(crate) fn place(&self) -> (usize, usize) {
        let end = self.previous().map_or((1, 0), |last| last.end);
        self.peek().map_or(end, |token| (token.line, token.column))
    }

    pub(crate) fn refuse<T>(&self, code: Code, message: impl Into<String>) -> Result<T, Refusal> {
        Err(Refusal::at(code, self.place(), message))
    }

    /// The next token's identifier text, if it is one.
    pub(crate) fn ident(&self) -> Option<&'a str> {
        match self.peek().map(|token| &token.tok) {
            Some(Tok::Ident(name)) => Some(name),
            _ => None,
        }
    }

    pub(crate) fn word(&self, word: &str) -> bool {
        self.ident() == Some(word)
    }

    pub(crate) fn sym(&self, symbol: &str) -> bool {
        self.peek()
            .is_some_and(|token| matches!(&token.tok, Tok::Sym(found) if *found == symbol))
    }

    /// Consume `symbol` if it is next.
    pub(crate) fn eat(&mut self, symbol: &str) -> bool {
        let found = self.sym(symbol);
        if found {
            self.at += 1;
        }
        found
    }

    /// Consume `symbol`, or refuse with `fix`.
    pub(crate) fn expect(&mut self, symbol: &str, fix: &str) -> Result<(), Refusal> {
        if self.eat(symbol) {
            return Ok(());
        }
        self.refuse(Code::UnexpectedToken, format!("expected `{symbol}`: {fix}"))
    }

    /// Whether the command being read ends here: at the end of input, a
    /// documentation comment, a `#` command, a command word, or anything
    /// starting a line in column 0.
    pub(crate) fn at_command(&self) -> bool {
        let starts_line = self.on_new_line() && self.place().1 == 0;
        match self.peek().map(|token| &token.tok) {
            Some(Tok::Doc(_) | Tok::Hash(_)) | None => true,
            Some(Tok::Ident(word)) => starts_line || COMMANDS.contains(&word.as_str()),
            Some(_) => starts_line,
        }
    }

    /// Whether the next token is a name a term may use, not a keyword.
    pub(crate) fn at_name(&self) -> bool {
        self.ident()
            .is_some_and(|word| !COMMANDS.contains(&word) && !TERM_WORDS.contains(&word))
    }

    /// Whether the next token starts on a later line than the one before it.
    pub(crate) fn on_new_line(&self) -> bool {
        match (self.previous(), self.peek()) {
            (Some(before), Some(next)) => next.line > before.line,
            _ => true,
        }
    }

    /// A type of the fragment.
    pub(crate) fn ty(&mut self) -> Result<Ty, Refusal> {
        let ty = match self.ident() {
            Some("Float") => Ty::Float,
            Some("Bool") => Ty::Bool,
            other => {
                let found = other.map_or_else(|| "this".to_owned(), |name| format!("`{name}`"));
                let fix = format!("{found} is not a type of the fragment; use `Float` or `Bool`");
                return self.refuse(Code::UnsupportedType, fix);
            }
        };
        self.at += 1;
        Ok(ty)
    }

    /// A new value name Osprey can spell that hides nothing in scope.
    pub(crate) fn fresh_name(&mut self) -> Result<String, Refusal> {
        let Some(name) = self.ident().filter(|_| self.at_name()) else {
            return self.refuse(
                Code::UnexpectedToken,
                "a name is missing here: write a lowercase ASCII name",
            );
        };
        if !spellable(name) {
            let fix = format!("Osprey cannot spell `{name}`; use a lowercase ASCII letter, then letters, digits and `_`");
            return self.refuse(Code::UnsupportedName, fix);
        }
        if crate::model::reserved(name) {
            let fix = format!("`{name}` is an Osprey keyword or builtin; choose another name");
            return self.refuse(Code::ReservedName, fix);
        }
        self.at += 1;
        Ok(name.to_owned())
    }

    /// Whether `name` is a parameter, a `let` name or a definition.
    pub(crate) fn in_scope(&self, name: &str) -> bool {
        self.locals.iter().any(|(local, _)| local == name)
            || self.defs.iter().any(|def| def.name == name)
    }
}

/// A lowercase ASCII letter or `_`, then ASCII letters, digits and `_`.
fn spellable(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_lowercase() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
