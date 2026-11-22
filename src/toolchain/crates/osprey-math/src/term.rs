//! The term grammar of [MATH-CODE-FRAGMENT], checked as it is read: every
//! name resolves and every operand, argument and branch has its type.

use crate::lex::Tok;
use crate::literal;
use crate::model::{Expr, Op, Tail, Ty};
use crate::names::{arity, method_of, Function, Head};
use crate::reader::Reader;
use crate::refusal::{Code, Refusal};

type Typed<T> = Result<(T, Ty), Refusal>;

/// Lean's binary operators, as `Init/Notation.lean` declares them: the
/// fragment's operator, how tightly it binds and the least binding its right
/// operand may have. `||` and `&&` are left-associative, `∨`, `∧` and `^`
/// right-associative, and comparisons neither.
const INFIX: &[(&str, Infix, u8, u8)] = &[
    ("||", Infix::Op(Op::Or), 30, 31),
    ("∨", Infix::Op(Op::Or), 30, 30),
    ("&&", Infix::Op(Op::And), 35, 36),
    ("∧", Infix::Op(Op::And), 35, 35),
    ("<", Infix::Op(Op::Lt), 50, 51),
    ("<=", Infix::Op(Op::Le), 50, 51),
    ("≤", Infix::Op(Op::Le), 50, 51),
    (">", Infix::Op(Op::Gt), 50, 51),
    (">=", Infix::Op(Op::Ge), 50, 51),
    ("≥", Infix::Op(Op::Ge), 50, 51),
    ("==", Infix::Op(Op::Eq), 50, 51),
    ("!=", Infix::Op(Op::Ne), 50, 51),
    ("≠", Infix::Unequal, 50, 51),
    ("+", Infix::Op(Op::Add), 65, 66),
    ("-", Infix::Op(Op::Sub), 65, 66),
    ("*", Infix::Op(Op::Mul), 70, 71),
    ("/", Infix::Op(Op::Div), 70, 71),
    ("^", Infix::Pow, 80, 80),
];

/// The binding of Lean's prefix `-`, and of the operand it reads.
const NEGATION: u8 = 75;
/// The binding a comparison's operands need: comparisons are `infix:50`.
const COMPARED: u8 = 51;

#[derive(Clone, Copy)]
enum Infix {
    Op(Op),
    Pow,
    /// `≠` states a proposition, which Lean decides for `Bool` and not for `Float`.
    Unequal,
}

/// What to write where a term should stand and none does.
const MISSING_TERM: &str = "a term is missing here: write a number, a name or a parenthesised term";

fn mismatch(place: (usize, usize), found: Ty, wanted: Ty, what: &str) -> Refusal {
    let message = format!("{what} is `{}`, found `{}`", wanted.lean(), found.lean());
    Refusal::at(Code::TypeMismatch, place, message)
}

pub(crate) fn require(
    place: (usize, usize),
    found: Ty,
    wanted: Ty,
    what: &str,
) -> Result<(), Refusal> {
    if found == wanted {
        Ok(())
    } else {
        Err(mismatch(place, found, wanted, what))
    }
}

/// `-term`: a negative literal, otherwise multiplication by `-1.0`, which
/// flips the sign of every non-NaN value exactly ([MATH-CODE-MEANING]).
fn negated(term: Expr) -> Expr {
    match term {
        Expr::Num(text) => match text.strip_prefix('-') {
            Some(positive) => Expr::Num(positive.to_owned()),
            None => Expr::Num(format!("-{text}")),
        },
        other => Expr::Binary(
            Op::Mul,
            Box::new(Expr::Num("-1.0".to_owned())),
            Box::new(other),
        ),
    }
}

impl Reader<'_> {
    /// A term where an `if` or a `let` may stand.
    pub(crate) fn tail(&mut self) -> Typed<Tail> {
        if self.word("if") {
            self.conditional()
        } else if self.word("let") {
            self.binding()
        } else {
            self.expr(0).map(|(term, ty)| (Tail::Expr(term), ty))
        }
    }

    fn conditional(&mut self) -> Typed<Tail> {
        self.at += 1;
        let place = self.place();
        let (test, test_ty) = self.expr(0)?;
        require(place, test_ty, Ty::Bool, "the test of an `if`")?;
        self.keyword("then")?;
        let (yes, ty) = self.tail()?;
        self.keyword("else")?;
        let place = self.place();
        let (no, no_ty) = self.tail()?;
        require(place, no_ty, ty, "the first branch of this `if`")?;
        Ok((Tail::If(test, Box::new(yes), Box::new(no)), ty))
    }

    fn keyword(&mut self, word: &str) -> Result<(), Refusal> {
        if !self.word(word) {
            return self.refuse(
                Code::UnexpectedToken,
                format!("an `if` reads `if test then value else value`; `{word}` is missing here"),
            );
        }
        self.at += 1;
        Ok(())
    }

    fn binding(&mut self) -> Typed<Tail> {
        let column = self.place().1;
        self.at += 1;
        let name = self.local_name()?;
        let declared = if self.eat(":") {
            Some(self.ty()?)
        } else {
            None
        };
        self.expect(":=", "a `let` binds with `:=`")?;
        let place = self.place();
        let (value, ty) = self.bound_value(column)?;
        require(
            place,
            ty,
            declared.unwrap_or(ty),
            "the declared type of this `let`",
        )?;
        self.binding_end()?;
        self.locals.push((name.clone(), ty));
        let body = self.tail();
        let _ = self.locals.pop();
        let (body, body_ty) = body?;
        Ok((Tail::Let(name, Box::new(value), Box::new(body)), body_ty))
    }

    fn local_name(&mut self) -> Result<String, Refusal> {
        self.new_name(|_| false, "a `let`")
    }

    /// A binder's name, refused when it shadows a name in scope or one `also_taken` names.
    pub(crate) fn new_name(
        &mut self,
        also_taken: impl Fn(&str) -> bool,
        binder: &str,
    ) -> Result<String, Refusal> {
        let shadowed = self
            .ident()
            .filter(|name| self.in_scope(name) || also_taken(name));
        if let Some(name) = shadowed {
            let fix = format!("`{name}` is already in scope; {binder} takes a new name");
            return self.refuse(Code::ShadowedName, fix);
        }
        self.fresh_name(crate::model::reserved)
    }

    /// The value of a `let` at `column`: an argument may not start at or left
    /// of that column on a later line, exactly as Lean ends the binding there.
    fn bound_value(&mut self, column: usize) -> Typed<Tail> {
        if self.word("let") {
            let fix = "a `let` inside a `let` value; bind the inner one on its own line first";
            return self.refuse(Code::UnsupportedTerm, fix);
        }
        self.let_columns.push(column);
        let value = self.tail();
        let _ = self.let_columns.pop();
        value
    }

    fn binding_end(&mut self) -> Result<(), Refusal> {
        if self.eat(";") || self.on_new_line() {
            return Ok(());
        }
        self.refuse(
            Code::UnexpectedToken,
            "a `let` ends with a line break or `;`",
        )
    }

    /// A value built from operators, by Lean's precedence: `floor` is the
    /// least binding an operator may have to continue this term.
    pub(crate) fn expr(&mut self, floor: u8) -> Typed<Expr> {
        let mut left = self.prefix(floor)?;
        let mut compared = false;
        while let Some((infix, next)) = self.infix(floor) {
            let place = self.place();
            if compared && next == COMPARED {
                let fix = "Lean's comparisons do not chain; parenthesise one of them";
                return self.refuse(Code::UnexpectedToken, fix);
            }
            compared = next == COMPARED;
            self.at += 1;
            let right = self.expr(next)?;
            if matches!(infix, Infix::Pow) {
                self.uses_builtin(place, "pow")?;
            }
            left = combine(place, infix, left, right)?;
        }
        if self.sym("=") {
            let fix = "`=` states a proposition; compare values with `==`";
            return self.refuse(Code::UnsupportedTerm, fix);
        }
        Ok(left)
    }

    fn infix(&self, floor: u8) -> Option<(Infix, u8)> {
        let Some(Tok::Sym(symbol)) = self.peek().map(|token| &token.tok) else {
            return None;
        };
        INFIX
            .iter()
            .find(|(text, _, bind, _)| text == symbol && *bind >= floor)
            .map(|(_, infix, _, next)| (*infix, *next))
    }

    /// Lean reads the operand of `-` at 75 and of `!` and `¬` at 40. A `-`
    /// term binds at 75 itself, so it cannot be the bare operand of `^`.
    fn prefix(&mut self, floor: u8) -> Typed<Expr> {
        let place = self.place();
        if self.sym("-") && floor > NEGATION {
            let fix = "Lean does not read a leading `-` as the operand of `^`; parenthesise it";
            return self.refuse(Code::UnexpectedToken, fix);
        }
        if self.eat("-") {
            let (term, ty) = self.expr(NEGATION)?;
            require(place, ty, Ty::Float, "the operand of `-`")?;
            return Ok((negated(term), Ty::Float));
        }
        if self.eat("!") || self.eat("¬") {
            let (term, ty) = self.expr(40)?;
            require(place, ty, Ty::Bool, "the operand of a negation")?;
            return Ok((Expr::Not(Box::new(term)), Ty::Bool));
        }
        self.application()
    }

    fn application(&mut self) -> Typed<Expr> {
        let place = self.place();
        let term = match self.head()? {
            Head::Value(term, ty) => (term, ty),
            Head::Function(function) => self.call(place, function)?,
        };
        self.same_line()?;
        Ok(term)
    }

    /// Lean continues an application onto a later line; the fragment keeps
    /// arguments on the head's line, so a later-line argument is refused
    /// unless it is where Lean itself ends an enclosing `let`, or it starts
    /// its line in column 0, where the next command begins.
    fn same_line(&mut self) -> Result<(), Refusal> {
        let column = self.place().1;
        if !self.at_atom() || !self.on_new_line() || column == 0 {
            return Ok(());
        }
        if self
            .let_columns
            .last()
            .is_some_and(|binding| column <= *binding)
        {
            return Ok(());
        }
        let fix = "Lean reads this as one more argument; keep arguments on the function's line";
        self.refuse(Code::MultilineApplication, fix)
    }

    fn at_atom(&self) -> bool {
        self.at_name()
            || self.sym("(")
            || matches!(self.peek().map(|token| &token.tok), Some(Tok::Num(_)))
    }

    fn call(&mut self, place: (usize, usize), function: Function) -> Typed<Expr> {
        let Function {
            callee,
            bound: mut arguments,
            params,
            result,
        } = function;
        let mut wanted = params.iter();
        while self.at_atom() && !self.on_new_line() {
            let at = self.place();
            let (argument, ty) = self.argument()?;
            let Some(want) = wanted.next() else {
                return Err(arity(place, &callee, params.len()));
            };
            require(at, ty, *want, "this argument")?;
            arguments.push(argument);
        }
        if wanted.next().is_some() {
            self.same_line()?;
            return Err(arity(place, &callee, params.len()));
        }
        Ok((Expr::Call(callee, arguments), result))
    }

    fn argument(&mut self) -> Typed<Expr> {
        let place = self.place();
        match self.head()? {
            Head::Value(term, ty) => Ok((term, ty)),
            Head::Function(function) => Err(arity(place, &function.callee, function.params.len())),
        }
    }

    fn head(&mut self) -> Result<Head, Refusal> {
        let Some(token) = self.peek() else {
            return self.refuse(Code::UnexpectedToken, MISSING_TERM);
        };
        let place = (token.line, token.column);
        match &token.tok {
            Tok::Num(text) => self.number(text),
            Tok::Ident(name) if name == "true" || name == "false" => {
                self.at += 1;
                Ok(Head::Value(Expr::Bool(name == "true"), Ty::Bool))
            }
            Tok::Ident(name) if self.at_name() => {
                self.at += 1;
                self.resolve(place, name)
            }
            Tok::Sym("(") => self.parenthesised(),
            _ => self.unreadable(),
        }
    }

    fn number(&mut self, text: &str) -> Result<Head, Refusal> {
        let decimal = literal::decimal(text).or_else(|code| {
            let fix = match code {
                Code::LiteralOutOfRange => "is not a finite binary64 value",
                _ => "is not a decimal literal: digits, an optional fraction, an optional exponent",
            };
            self.refuse(code, format!("`{text}` {fix}"))
        })?;
        self.at += 1;
        Ok(Head::Value(Expr::Num(decimal), Ty::Float))
    }

    fn unreadable<T>(&self) -> Result<T, Refusal> {
        if self.word("if") || self.word("let") {
            let fix = "an `if` or `let` cannot stand as an operand; bind it with `let` first";
            return self.refuse(Code::NestedConditional, fix);
        }
        if self.word("fun") || self.word("match") || self.word("do") || self.word("by") {
            return self.refuse(
                Code::UnsupportedTerm,
                "this term form is outside the fragment",
            );
        }
        self.refuse(Code::UnexpectedToken, MISSING_TERM)
    }

    fn parenthesised(&mut self) -> Result<Head, Refusal> {
        self.at += 1;
        let (term, ty) = self.expr(0)?;
        if self.sym(":") {
            let fix =
                "a type ascription is outside the fragment; the types of the definition decide";
            return self.refuse(Code::UnsupportedTerm, fix);
        }
        self.expect(")", "close the parenthesis")?;
        match self
            .peek()
            .map(|token| (&token.tok, (token.line, token.column)))
        {
            Some((Tok::Field(method), place)) => {
                self.at += 1;
                let function = method_of(place, method, term, ty)?;
                self.called(place, function)
            }
            _ => Ok(Head::Value(term, ty)),
        }
    }
}
fn combine(
    place: (usize, usize),
    infix: Infix,
    left: (Expr, Ty),
    right: (Expr, Ty),
) -> Typed<Expr> {
    let ((left, left_ty), (right, right_ty)) = (left, right);
    let op = match infix {
        Infix::Op(op) => op,
        Infix::Unequal if left_ty == Ty::Bool => Op::Ne,
        Infix::Unequal => {
            let fix = "Lean cannot decide `≠` between `Float` values; compare them with `!=`";
            return Err(Refusal::at(Code::UnsupportedTerm, place, fix));
        }
        Infix::Pow => {
            require(place, left_ty, Ty::Float, "the base of `^`")?;
            require(place, right_ty, Ty::Float, "the exponent of `^`")?;
            return Ok((Expr::Call("pow".to_owned(), vec![left, right]), Ty::Float));
        }
    };
    let (operand, result) = match op {
        Op::Or | Op::And => (Ty::Bool, Ty::Bool),
        Op::Eq | Op::Ne => (left_ty, Ty::Bool),
        Op::Lt | Op::Le | Op::Gt | Op::Ge => (Ty::Float, Ty::Bool),
        Op::Add | Op::Sub | Op::Mul | Op::Div => (Ty::Float, Ty::Float),
    };
    let what = format!("an operand of `{}`", op.written().0);
    require(place, left_ty, operand, &what)?;
    require(place, right_ty, operand, &what)?;
    Ok((Expr::Binary(op, Box::new(left), Box::new(right)), result))
}
