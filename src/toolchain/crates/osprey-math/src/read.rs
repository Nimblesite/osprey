//! The command grammar of [MATH-CODE-FRAGMENT]: definitions, one namespace,
//! and the `#` commands Lean checks but that generate nothing.

use crate::lex::{Tok, Token};
use crate::model::{Def, Module, Tail, Ty};
use crate::reader::{Reader, Signature};
use crate::refusal::{Code, Refusal};

/// `#` commands that Lean checks and that generate nothing.
const CHECKED_BY_LEAN: &[&str] = &["eval", "check", "guard", "print"];

/// How far the one namespace block has got.
#[derive(PartialEq, Eq)]
enum Block {
    Absent,
    Open,
    Closed,
}

/// Read a whole math source into the module it generates.
///
/// # Errors
/// The first refusal of [MATH-CODE-REFUSALS].
pub(crate) fn read(file_stem: &str, tokens: &[Token]) -> Result<Module, Refusal> {
    let mut reader = Reader::new(tokens);
    let mut defs = Vec::new();
    let mut block = Block::Absent;
    let mut doc = None;
    while let Some(token) = reader.peek() {
        let documented = doc.take();
        match &token.tok {
            Tok::Doc(text) => {
                doc = Some(text.clone());
                reader.at += 1;
            }
            Tok::Hash(name) if CHECKED_BY_LEAN.contains(&name.as_str()) => reader.skip_command(),
            Tok::Ident(word) => match word.as_str() {
                "def" | "private" => defs.push(reader.definition(documented, &block)?),
                "namespace" => reader.open(&mut block, defs.is_empty())?,
                "end" => reader.close(&mut block)?,
                "open" => reader.opening()?,
                _ => return reader.unsupported_command(),
            },
            _ => return reader.unsupported_command(),
        }
    }
    module(file_stem, reader.namespace.as_deref(), defs)
}

/// The Osprey namespace and module for a Lean namespace, or for the file.
fn module(file_stem: &str, namespace: Option<&str>, defs: Vec<Def>) -> Result<Module, Refusal> {
    let (outer, name) = match namespace.map(|namespace| namespace.rsplit_once('.')) {
        Some(Some((outer, name))) => (Some(outer), name),
        Some(None) => (None, namespace.unwrap_or(file_stem)),
        None => (None, file_stem),
    };
    if !module_name(name) || !outer.is_none_or(namespace_name) {
        let message = format!(
            "the module would be `{name}`: wrap the definitions in `namespace Name … end Name` or `namespace app.Name … end app.Name`, a capitalised ASCII module and an optional lowercase namespace"
        );
        return Err(Refusal::at(Code::NamespaceShape, (1, 0), message));
    }
    Ok(Module {
        namespace: outer.map(str::to_owned),
        name: name.to_owned(),
        defs,
    })
}

fn module_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn namespace_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|first| first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

impl Reader<'_> {
    /// Pass a command the fragment does not generate from: it ends where the
    /// next command begins.
    fn skip_command(&mut self) {
        self.at += 1;
        while !self.at_command() {
            self.at += 1;
        }
    }

    fn unsupported_command<T>(&self) -> Result<T, Refusal> {
        let found = self
            .ident()
            .map_or_else(|| "this".to_owned(), |word| format!("`{word}`"));
        let fix = format!(
            "{found} does not begin a command of the fragment: a math source holds `def`, one `namespace` and `#eval`, `#check`, `#guard`; a statement about its definitions belongs in a proof file that imports it"
        );
        self.refuse(Code::UnsupportedCommand, fix)
    }

    /// `open Float (exp log)`: the listed `Float` functions, unqualified from
    /// here on. The list is required, so no other name changes its meaning.
    fn opening(&mut self) -> Result<(), Refusal> {
        self.at += 1;
        let float = self.word("Float");
        self.at += usize::from(float);
        if !float || !self.eat("(") {
            let fix =
                "open only the `Float` functions the source uses, by name: `open Float (exp log)`";
            return self.refuse(Code::UnsupportedCommand, fix);
        }
        while let Some(name) = self.ident().filter(|_| self.at_name()) {
            let _ = crate::names::float_function(self.place(), name, Vec::new())?;
            if self.in_scope(name) {
                return self.refuse(
                    Code::DuplicateName,
                    format!("`{name}` already has a meaning above"),
                );
            }
            self.opened.push(name.to_owned());
            self.at += 1;
        }
        self.expect(")", "close the list of opened functions")
    }

    fn open(&mut self, block: &mut Block, first: bool) -> Result<(), Refusal> {
        if *block != Block::Absent || !first {
            let fix = "a math source has one `namespace` block, around every definition";
            return self.refuse(Code::NamespaceShape, fix);
        }
        self.at += 1;
        let Some(name) = self.ident().filter(|name| name.matches('.').count() <= 1) else {
            let fix = "name the namespace `Name` or `app.Name`";
            return self.refuse(Code::NamespaceShape, fix);
        };
        self.namespace = Some(name.to_owned());
        self.at += 1;
        *block = Block::Open;
        Ok(())
    }

    fn close(&mut self, block: &mut Block) -> Result<(), Refusal> {
        self.at += 1;
        if *block != Block::Open || self.ident() != self.namespace.as_deref() {
            let fix = "`end` closes the open namespace by its full name";
            return self.refuse(Code::NamespaceShape, fix);
        }
        self.at += 1;
        *block = Block::Closed;
        Ok(())
    }

    fn definition(&mut self, doc: Option<String>, block: &Block) -> Result<Def, Refusal> {
        if *block == Block::Closed {
            let fix = "this definition is outside the namespace; move it before `end`";
            return self.refuse(Code::NamespaceShape, fix);
        }
        let public = !self.word("private");
        self.at += usize::from(!public);
        if !self.word("def") {
            return self.unsupported_command();
        }
        self.at += 1;
        let place = self.place();
        let name = self.definition_name()?;
        let params = self.parameters()?;
        let result = self.result_type()?;
        let body = self.body(&params, result)?;
        if self.builtins.contains(&name) {
            return Err(crate::reader::captured(place, &name));
        }
        let types = params.iter().map(|(_, ty)| *ty).collect();
        self.defs.push(Signature {
            name: name.clone(),
            params: types,
            result,
        });
        Ok(Def {
            name,
            doc,
            public,
            params,
            result,
            body,
        })
    }

    fn definition_name(&mut self) -> Result<String, Refusal> {
        if let Some(name) = self.ident().filter(|name| self.in_scope(name)) {
            return self.refuse(
                Code::DuplicateName,
                format!("`{name}` is already defined above"),
            );
        }
        self.fresh_name(crate::model::keyword)
    }

    /// Zero or more `(a b : T)` groups.
    fn parameters(&mut self) -> Result<Vec<(String, Ty)>, Refusal> {
        let mut params: Vec<(String, Ty)> = Vec::new();
        while self.eat("(") {
            let first = params.len();
            while !self.sym(":") {
                let name = self.parameter_name(&params)?;
                params.push((name, Ty::Float));
            }
            self.at += 1;
            let ty = self.ty()?;
            params
                .iter_mut()
                .skip(first)
                .for_each(|(_, declared)| *declared = ty);
            self.expect(")", "close the parameter group")?;
        }
        Ok(params)
    }

    fn parameter_name(&mut self, params: &[(String, Ty)]) -> Result<String, Refusal> {
        let earlier = |name: &str| params.iter().any(|(other, _)| other == name);
        self.new_name(earlier, "a parameter")
    }

    fn result_type(&mut self) -> Result<Ty, Refusal> {
        if self.eat(":") {
            return self.ty();
        }
        if self.sym(":=") {
            let fix = "state the result type before `:=`, as `: Float` or `: Bool`";
            return self.refuse(Code::UnsupportedType, fix);
        }
        let fix = "write each parameter as `(name : Float)` and then the result type";
        self.refuse(Code::UnexpectedToken, fix)
    }

    fn body(&mut self, params: &[(String, Ty)], result: Ty) -> Result<Tail, Refusal> {
        self.expect(":=", "a definition's body follows `:=`")?;
        self.locals = params.to_vec();
        let place = self.place();
        let read = self.tail();
        self.locals.clear();
        let (body, ty) = read?;
        if ty != result {
            let message = format!(
                "the body is `{}`, the declared result is `{}`",
                ty.lean(),
                result.lean()
            );
            return Err(Refusal::at(Code::TypeMismatch, place, message));
        }
        if !self.at_command() {
            return self.refuse(
                Code::UnexpectedToken,
                "the definition ends here; expected the next command",
            );
        }
        Ok(body)
    }
}
