//! The generated ML-flavor module of [MATH-CODE-EMIT].

use crate::literal::is_zero;
use crate::model::{Def, Expr, Module, Op, Tail, Ty};

const INDENT: &str = "    ";
/// How tightly an application binds, and the binding an operand needs to
/// stand as a bare argument.
const APPLICATION: u8 = 7;
const ATOM: u8 = 8;

/// The module's source text. `file` is the Lean file's name, for the header.
pub(crate) fn emit(file: &str, module: &Module) -> String {
    let mut out = Out(format!(
        "// Generated from {file} by `osprey math` [MATH-CODE-EMIT]. Do not edit: change the Lean source.\n"
    ));
    if let Some(namespace) = &module.namespace {
        out.line(0, &format!("namespace {namespace}"));
        out.0.push('\n');
    }
    out.line(0, &format!("(** Mathematics generated from {file}. *)"));
    out.line(0, &format!("module {}", module.name));
    for (index, def) in module.defs.iter().enumerate() {
        if index > 0 {
            out.0.push('\n');
        }
        out.definition(file, def, module.defs.get(..index).unwrap_or_default());
    }
    out.0
}

struct Out(String);

impl Out {
    fn line(&mut self, depth: usize, text: &str) {
        self.0.push_str(&INDENT.repeat(depth));
        self.0.push_str(text);
        self.0.push('\n');
    }

    fn definition(&mut self, file: &str, def: &Def, earlier: &[Def]) {
        let name = &def.name;
        let doc = def.doc.clone().filter(|doc| !doc.is_empty());
        let generated = format!("`{name}`, generated from {file}.");
        if let Some(doc) = doc.or_else(|| def.public.then_some(generated)) {
            self.line(1, &format!("(** {} *)", doc.replace("*)", "* )")));
        }
        let export = if def.public { "export " } else { "" };
        let pinned = crate::pin::annotated(def, earlier);
        if pinned.is_none() {
            self.line(1, &format!("{export}{name} : {}", signature(def)));
        }
        let inline = pinned.clone().unwrap_or_default();
        let names: Vec<String> = def
            .params
            .iter()
            .enumerate()
            .map(|(index, (param, ty))| {
                if inline.contains(&index) {
                    format!("{param}: {}", ty.osprey())
                } else {
                    param.clone()
                }
            })
            .collect();
        let head = match (names.as_slice(), inline.is_empty()) {
            ([], _) => name.clone(),
            ([one], true) => format!("{name} {one}"),
            (many, _) => format!("{name} ({})", many.join(", ")),
        };
        let visible = if pinned.is_some() { export } else { "" };
        self.tail(&def.body, 1, &format!("{visible}{head} ="));
    }

    /// `lead` and then the term: on one line, as a `match`, or as a block.
    fn tail(&mut self, tail: &Tail, depth: usize, lead: &str) {
        let after = |text: &str| format!("{lead} {text}").trim_start().to_owned();
        match tail {
            Tail::Expr(term) => self.line(depth, &after(&show(term, 0))),
            Tail::If(test, yes, no) => {
                self.line(depth, &after(&format!("match {}", show(test, 0))));
                self.tail(yes, depth + 1, "true =>");
                self.tail(no, depth + 1, "false =>");
            }
            Tail::Let(..) => {
                self.line(depth, lead);
                self.block(tail, depth + 1);
            }
        }
    }

    /// Bindings one per line, then the value they lead to.
    fn block(&mut self, tail: &Tail, depth: usize) {
        match tail {
            Tail::Let(name, value, body) => {
                self.tail(value, depth, &format!("{name} ="));
                self.block(body, depth);
            }
            other => self.tail(other, depth, ""),
        }
    }
}

fn signature(def: &Def) -> String {
    let types: Vec<&str> = def.params.iter().map(|(_, ty)| ty.osprey()).collect();
    let result = Ty::osprey(def.result);
    match types.as_slice() {
        [] => result.to_owned(),
        [one] => format!("{one} -> {result}"),
        many => format!("({}) -> {result}", many.join(", ")),
    }
}

/// `term` as text, parenthesised when it binds less tightly than `floor`.
fn show(term: &Expr, floor: u8) -> String {
    let (text, binding) = written(term);
    if binding < floor {
        format!("({text})")
    } else {
        text
    }
}

/// The text of `term` and how tightly it binds.
fn written(term: &Expr) -> (String, u8) {
    match term {
        Expr::Num(text) if text.starts_with('-') => (text.clone(), 0),
        Expr::Num(text) | Expr::Name(text) => (text.clone(), ATOM),
        Expr::Bool(value) => (value.to_string(), ATOM),
        Expr::Call(callee, arguments) => call(callee, arguments),
        Expr::Not(operand) => (format!("!{}", show(operand, ATOM)), APPLICATION),
        Expr::Binary(Op::Div, left, right) if !nonzero_literal(right) => {
            call("floatDiv", &[(**left).clone(), (**right).clone()])
        }
        Expr::Binary(op, left, right) => {
            let (symbol, binding) = op.written();
            // Comparisons do not chain; every other operator groups to the left.
            let left_floor = if binding == 3 { binding + 1 } else { binding };
            let text = format!(
                "{} {symbol} {}",
                show(left, left_floor),
                show(right, binding + 1)
            );
            (text, binding)
        }
    }
}

fn call(callee: &str, arguments: &[Expr]) -> (String, u8) {
    match arguments {
        [] => (callee.to_owned(), ATOM),
        [one] => (format!("{callee} {}", show(one, ATOM)), APPLICATION),
        many => {
            let list: Vec<String> = many.iter().map(|argument| show(argument, 0)).collect();
            (format!("{callee} ({})", list.join(", ")), APPLICATION)
        }
    }
}

/// Osprey's `/` asks `Arith.divideByZero` at a zero divisor; only a divisor
/// that is visibly nonzero keeps Lean's IEEE meaning under `/`.
fn nonzero_literal(term: &Expr) -> bool {
    matches!(term, Expr::Num(text) if !is_zero(text))
}
