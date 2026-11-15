//! What a name means: a local, a definition of the source, or one of Lean's
//! `Float` functions with the Osprey builtin that computes the same value.

use crate::model::{function, Expr, Ty};
use crate::reader::Reader;
use crate::refusal::{Code, Refusal};
use crate::term::require;

/// What a name at the head of an application is.
pub(crate) enum Head {
    /// A value: nothing may be applied to it.
    Value(Expr, Ty),
    /// A function awaiting arguments, with any receiver already bound.
    Function(Function),
}

pub(crate) struct Function {
    pub(crate) callee: String,
    pub(crate) bound: Vec<Expr>,
    pub(crate) params: Vec<Ty>,
    pub(crate) result: Ty,
}

impl Function {
    /// A function with every argument bound is a value.
    pub(crate) fn head(self) -> Head {
        if self.params.is_empty() {
            return Head::Value(Expr::Call(self.callee, self.bound), self.result);
        }
        Head::Function(self)
    }
}

impl Reader<'_> {
    /// What `name` means: a local, a definition, `Float.f`, or `value.f`.
    pub(crate) fn resolve(&mut self, place: (usize, usize), name: &str) -> Result<Head, Refusal> {
        if let Some((_, ty)) = self.locals.iter().find(|(local, _)| local == name) {
            return Ok(Head::Value(Expr::Name(name.to_owned()), *ty));
        }
        if let Some(head) = self.defined(name) {
            return Ok(head);
        }
        let opened = self.opened.iter().any(|opened| opened == name);
        if let Some(method) = name.strip_prefix("Float.").or(opened.then_some(name)) {
            let function = float_function(place, method, Vec::new())?;
            return self.called(place, function);
        }
        let Some((receiver, method)) = name.rsplit_once('.') else {
            let fix = format!("`{name}` is not defined above; definitions are read in order");
            return Err(Refusal::at(Code::UnknownName, place, fix));
        };
        match self.resolve(place, receiver)? {
            Head::Value(term, ty) => {
                let function = method_of(place, method, term, ty)?;
                self.called(place, function)
            }
            Head::Function(function) => Err(arity(place, &function.callee, function.params.len())),
        }
    }

    /// A `Float` function as a head, noted as a builtin the generated body calls.
    pub(crate) fn called(
        &mut self,
        place: (usize, usize),
        function: Function,
    ) -> Result<Head, Refusal> {
        self.uses_builtin(place, &function.callee)?;
        Ok(function.head())
    }

    /// A definition of this file, by its name or its namespace-qualified name.
    fn defined(&self, name: &str) -> Option<Head> {
        let local = self
            .namespace
            .as_deref()
            .and_then(|namespace| name.strip_prefix(namespace)?.strip_prefix('.'))
            .unwrap_or(name);
        let def = self.defs.iter().find(|def| def.name == local)?;
        let (callee, params) = (def.name.clone(), def.params.clone());
        Some(
            Function {
                callee,
                bound: Vec::new(),
                params,
                result: def.result,
            }
            .head(),
        )
    }
}

pub(crate) fn arity(place: (usize, usize), callee: &str, wanted: usize) -> Refusal {
    let message = format!("`{callee}` takes exactly {wanted} argument(s), all on its line");
    Refusal::at(Code::ArityMismatch, place, message)
}

/// `receiver.method`: Lean's `Float.method receiver`.
pub(crate) fn method_of(
    place: (usize, usize),
    method: &str,
    receiver: Expr,
    ty: Ty,
) -> Result<Function, Refusal> {
    require(place, ty, Ty::Float, "the receiver of a `Float` function")?;
    float_function(place, method, vec![receiver])
}

/// Lean's `Float.<method>` with `bound` as its leading arguments.
pub(crate) fn float_function(
    place: (usize, usize),
    method: &str,
    bound: Vec<Expr>,
) -> Result<Function, Refusal> {
    let Some((callee, count)) = function(method) else {
        let message = format!(
            "no Osprey operation computes the same value as `Float.{method}`; write the formula with the functions of [MATH-CODE-MEANING]"
        );
        return Err(Refusal::at(Code::UnsupportedFunction, place, message));
    };
    let params = vec![Ty::Float; count.saturating_sub(bound.len())];
    Ok(Function {
        callee: callee.to_owned(),
        bound,
        params,
        result: Ty::Float,
    })
}
