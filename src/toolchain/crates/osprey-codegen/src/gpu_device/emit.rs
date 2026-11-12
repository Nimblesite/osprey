//! One kernel body from its AST to the device dialect [GPU-BACKEND-DEVICE].
//!
//! Every expression becomes a pure C expression over `long` (int), `ulong`
//! (a float's bits) and `bool`; a construct that needs statements — a `let`,
//! a `match`, a `fold` over a `range`, a bounds-checked `gpuGet` — hoists them
//! into the enclosing block ahead of a temporary. Helper functions are inlined
//! at the AST level, so the kernel is one self-contained entry point.
//! Integer arithmetic goes through the library's checked helpers, which record
//! a fault the runtime aborts on ([GPU-DEVICE-FAULT]); float arithmetic through
//! the binary64 library. Anything else declines with the reason.

use super::dialect::{
    bool_arms, callee, describe, elvis_gather, float_binary, float_literal, int_binary, int_literal,
};
use super::loops::uniform_local;
use super::{DeviceBody, DeviceFailure, Ty, Uniform};
use crate::builder::Codegen;
use osprey_ast::{Expr, MatchArm, NamedArgument, Parameter, Stmt};
use osprey_types::numeric::Numeric;
use std::collections::HashMap;
use std::rc::Rc;

pub(super) type Emitted = Result<(Ty, String), String>;

/// A name in scope: a scalar held in a C variable, a buffer uniform with its
/// pointer and length variables, or a function passed rather than called.
#[derive(Clone)]
pub(super) enum Local {
    Scalar(Ty, String),
    Buffer(String, String),
    Function(Rc<Closure>),
}

/// A lambda or helper handed to a function: inlined wherever it is called, in
/// the scopes it closed over, so a helper's parameters never shadow its names.
pub(super) struct Closure {
    pub(super) parameters: Vec<Parameter>,
    pub(super) body: Expr,
    scopes: Vec<HashMap<String, Local>>,
}

pub(super) struct Emitter<'a> {
    pub(super) cg: &'a Codegen,
    /// The statement block being written, innermost last.
    blocks: Vec<Vec<String>>,
    temps: usize,
    scopes: Vec<HashMap<String, Local>>,
    pub(super) depth: usize,
    foreign_call: Option<String>,
    pub(super) default_float: Ty,
}

/// Bound helper expansion, including recursive and deep acyclic call chains.
pub(super) const MAX_INLINE_DEPTH: usize = 24;

/// Compile `body` with the kernel's parameters bound to `p0`, `p1`, … and
/// its captures to `u0`, `u1`, …. `params` are compute types, `storages` the
/// buffer element types they read from; `result_storage` types the `out`.
pub(super) fn body(
    cg: &Codegen,
    parameters: &[Parameter],
    body: &Expr,
    params: &[Ty],
    storages: &[Ty],
    captures: &[(String, Uniform)],
    result_storage: Option<Ty>,
) -> Result<DeviceBody, DeviceFailure> {
    if parameters.len() != params.len() {
        return Err("the kernel's arity disagrees with the combinator's slots".into());
    }
    let mut emitter = Emitter {
        cg,
        blocks: vec![Vec::new()],
        temps: 0,
        scopes: vec![HashMap::new()],
        depth: 0,
        foreign_call: None,
        default_float: if result_storage.is_some_and(|ty| ty.compute() == Ty::FLOAT32) {
            Ty::FLOAT32
        } else {
            Ty::FLOAT
        },
    };
    for (i, (parameter, ty)) in parameters.iter().zip(params).enumerate() {
        emitter.bind(&parameter.name, Local::Scalar(*ty, format!("p{i}")));
    }
    for (i, (name, uniform)) in captures.iter().enumerate() {
        emitter.bind(name, uniform_local(i, uniform));
    }
    let (result_type, result) = emitter.checked_expression(body)?;
    Ok(DeviceBody {
        lines: emitter.blocks.pop().unwrap_or_default(),
        result,
        // A boxed host return may look like an integer word. The emitted
        // scalar determines its representation; storage can refine it (half)
        // only when both have the same compute type [GPU-DEVICE-NATIVE].
        result_storage: result_storage
            .filter(|storage| storage.compute() == result_type)
            .unwrap_or(result_type),
        params: params.to_vec(),
        param_storage: storages.to_vec(),
        uniforms: captures
            .iter()
            .map(|(_, uniform)| uniform.clone())
            .collect(),
    })
}

impl Emitter<'_> {
    fn checked_expression(&mut self, expression: &Expr) -> Result<(Ty, String), DeviceFailure> {
        self.expression(expression).map_err(|reason| {
            self.foreign_call
                .take()
                .map_or(DeviceFailure::Unsupported(reason), DeviceFailure::Foreign)
        })
    }

    pub(super) fn line(&mut self, text: String) {
        if let Some(block) = self.blocks.last_mut() {
            block.push(text);
        }
    }

    pub(super) fn temp(&mut self) -> String {
        self.temps += 1;
        format!("t{}", self.temps)
    }

    pub(super) fn declare(&mut self, ty: Ty, value: &str) -> String {
        let name = self.temp();
        self.line(format!("{} {name} = {value};", ty.compute_spelling()));
        name
    }

    pub(super) fn bind(&mut self, name: &str, local: Local) {
        if let Some(scope) = self.scopes.last_mut() {
            let _ = scope.insert(name.to_string(), local);
        }
    }

    pub(super) fn lookup(&self, name: &str) -> Option<Local> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
    }

    /// Run `emit` in its own block and scope; its statements come back with
    /// its value, for the caller to place under a condition or a loop.
    pub(super) fn nested(
        &mut self,
        emit: impl FnOnce(&mut Self) -> Emitted,
    ) -> Result<(Vec<String>, Ty, String), String> {
        self.blocks.push(Vec::new());
        self.scopes.push(HashMap::new());
        let result = emit(self);
        let _ = self.scopes.pop();
        let lines = self.blocks.pop().unwrap_or_default();
        let (ty, value) = result?;
        Ok((lines, ty, value))
    }

    pub(super) fn indented(&mut self, lines: Vec<String>) {
        for line in lines {
            self.line(format!("  {line}"));
        }
    }

    pub(super) fn expression(&mut self, expression: &Expr) -> Emitted {
        match expression {
            Expr::Integer(n) => Ok((Ty::INT, int_literal(*n))),
            Expr::Float(f) => Ok((
                self.default_float,
                super::numeric::convert(self.default_float, Ty::FLOAT, &float_literal(*f))
                    .map(|(_, text)| text)
                    .ok_or_else(|| "float literal has no device representation".to_string())?,
            )),
            Expr::Bool(b) => Ok((Ty::Bool, if *b { "true" } else { "false" }.into())),
            Expr::Identifier(name) => self.identifier(name),
            Expr::Binary {
                op, left, right, ..
            } => self.binary(op, left, right),
            Expr::Unary { op, operand } => self.unary(op, operand),
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => self.call(callee(function), arguments, named_arguments),
            Expr::Pipe { left, right } => self.pipe(left, right),
            Expr::Match { value, arms } => self.matching(value, arms),
            Expr::Block {
                statements, value, ..
            } => self.block(statements, value.as_deref()),
            Expr::TypeApply { function, .. } => self.expression(function),
            other => Err(format!("{} has no device form", describe(other))),
        }
    }

    fn identifier(&mut self, name: &str) -> Emitted {
        match self.lookup(name) {
            Some(Local::Scalar(ty, value)) => Ok((ty, value)),
            Some(Local::Buffer(..)) => Err(format!("`{name}` is a buffer, not a kernel value")),
            Some(Local::Function(_)) => Err(format!("`{name}` is a function, not a kernel value")),
            None => match self.cg.device.constants.get(name).cloned() {
                Some((target, literal)) => {
                    let (ty, value) = self
                        .with_float_context(target.map_or(Ty::FLOAT, Ty::compute), |emitter| {
                            emitter.expression(&literal)
                        })?;
                    match target {
                        Some(storage) if storage.compute() != ty => {
                            super::numeric::convert(storage, ty, &value)
                                .ok_or_else(|| "constant has no device conversion".into())
                        }
                        _ => Ok((ty, value)),
                    }
                }
                None => Err(format!("`{name}` is not a value a device kernel can reach")),
            },
        }
    }

    fn binary(&mut self, op: &str, left: &Expr, right: &Expr) -> Emitted {
        if op == "&&" || op == "||" {
            return self.short_circuit(op, left, right);
        }
        let (left_ty, l) = self.expression(left)?;
        let (right_ty, r) = self.expression(right)?;
        let (left_ty, l) = super::numeric::fit_literal(left, left_ty, l, right_ty);
        let (right_ty, r) = super::numeric::fit_literal(right, right_ty, r, left_ty);
        match (left_ty, right_ty) {
            (Ty::Num(a), Ty::Num(b)) if a == b && a.is_integer() => int_binary(op, a, &l, &r),
            (Ty::Num(a), Ty::Num(b)) if a == b && a.is_float() => float_binary(op, a, &l, &r),
            (Ty::Bool, Ty::Bool) if op == "==" || op == "!=" => {
                Ok((Ty::Bool, format!("({l} {op} {r})")))
            }
            _ => Err(format!(
                "`{op}` over {} and {} operands",
                left_ty.name(),
                right_ty.name()
            )),
        }
    }

    /// `&&`/`||` keep the host's short circuit, so a fallible operation on the
    /// right never runs when the left decides the answer.
    fn short_circuit(&mut self, op: &str, left: &Expr, right: &Expr) -> Emitted {
        let (ty, condition) = self.expression(left)?;
        if ty != Ty::Bool {
            return Err(format!("`{op}` over a {} operand", ty.name()));
        }
        if op == "&&" {
            self.branch(&condition, right, &Expr::Bool(false))
        } else {
            self.branch(&condition, &Expr::Bool(true), right)
        }
    }

    fn unary(&mut self, op: &str, operand: &Expr) -> Emitted {
        let (ty, value) = self.expression(operand)?;
        match (op, ty) {
            ("-", Ty::Num(n)) if n.is_integer() => Ok((
                ty,
                format!("osprey_neg{}({value}, fault)", super::numeric::suffix(n)),
            )),
            ("-", Ty::Num(n)) if n.is_float() => match n.float_compute() {
                Numeric::FLOAT => Ok((Ty::FLOAT, format!("neg64({value})"))),
                _ => Ok((Ty::Num(n.float_compute()), format!("(-({value}))"))),
            },
            ("!" | "not", Ty::Bool) => Ok((Ty::Bool, format!("(!{value})"))),
            _ => Err(format!("unary `{op}` over a {} operand", ty.name())),
        }
    }

    fn call(&mut self, function: &Expr, arguments: &[Expr], named: &[NamedArgument]) -> Emitted {
        if !named.is_empty() {
            return Err("named arguments have no device form".into());
        }
        let Expr::Identifier(name) = function else {
            return match self.passed(function) {
                Some(Local::Function(closure)) => self.invoke("lambda", &closure, arguments),
                _ => Err("a computed callee has no device form".into()),
            };
        };
        match self.lookup(name) {
            Some(Local::Function(closure)) => self.invoke(name, &closure, arguments),
            Some(_) => Err(format!(
                "`{name}` is a kernel value, not a function the device can call"
            )),
            None => self.named_call(name, arguments),
        }
    }

    fn named_call(&mut self, name: &str, arguments: &[Expr]) -> Emitted {
        if self.cg.extern_params.contains_key(name) {
            self.foreign_call = Some(name.to_owned());
            return Err(format!("foreign function `{name}` has no device form"));
        }
        match name {
            "fold" => self.fold(arguments),
            "gpuLength" => self.length(arguments),
            _ if self.cg.device.defs.contains_key(name) => self.inline(name, arguments),
            _ => self.builtin(name, arguments),
        }
    }

    /// Inline a helper: each argument is bound once, in order, then the body
    /// is emitted in a scope holding the parameters.
    fn inline(&mut self, name: &str, arguments: &[Expr]) -> Emitted {
        let Some((parameters, body)) = self.cg.device.defs.get(name).cloned() else {
            return Err(format!("`{name}` is not a function the device can inline"));
        };
        let bindings = self.bindings(name, &parameters, arguments)?;
        let target = self
            .cg
            .prog
            .return_type(name)
            .and_then(Numeric::of_type)
            .filter(|n| n.is_float())
            .map_or(self.default_float, Ty::Num);
        // Arguments belong to the caller; a named helper body belongs to its
        // module, just like a named helper passed as a function value.
        let closure = Closure {
            parameters,
            body,
            scopes: Vec::new(),
        };
        self.with_float_context(target, |emitter| emitter.closed(&closure, bindings))
            .map_err(|reason| {
                let prefix = format!("{name}: ");
                if reason.starts_with(&prefix) {
                    reason
                } else {
                    format!("{prefix}{reason}")
                }
            })
    }

    /// Inline a function argument the way a helper is inlined, in its own
    /// scopes.
    fn invoke(&mut self, name: &str, closure: &Closure, arguments: &[Expr]) -> Emitted {
        let bindings = self.bindings(name, &closure.parameters, arguments)?;
        self.closed(closure, bindings)
    }

    /// Emit `closure`'s body under `bindings` in the scopes it closed over.
    pub(super) fn closed(&mut self, closure: &Closure, bindings: Vec<(String, Local)>) -> Emitted {
        let outer = std::mem::replace(&mut self.scopes, closure.scopes.clone());
        let result = self.apply(&closure.body, bindings);
        self.scopes = outer;
        result
    }

    /// Each argument bound once, in order, to its parameter.
    fn bindings(
        &mut self,
        name: &str,
        parameters: &[Parameter],
        arguments: &[Expr],
    ) -> Result<Vec<(String, Local)>, String> {
        if parameters.len() != arguments.len() {
            return Err(format!(
                "`{name}` is called with {} of {} arguments",
                arguments.len(),
                parameters.len()
            ));
        }
        if self.depth >= MAX_INLINE_DEPTH {
            return Err(format!(
                "device inline depth limit ({MAX_INLINE_DEPTH}) reached at `{name}`"
            ));
        }
        parameters
            .iter()
            .zip(arguments)
            .map(|(parameter, argument)| Ok((parameter.name.clone(), self.argument(argument)?)))
            .collect()
    }

    /// Emit `body` under `bindings` one inlining level deeper.
    pub(super) fn apply(&mut self, body: &Expr, bindings: Vec<(String, Local)>) -> Emitted {
        self.depth += 1;
        self.scopes.push(bindings.into_iter().collect());
        let result = self.expression(body);
        let _ = self.scopes.pop();
        self.depth -= 1;
        result
    }

    /// A call argument as a local: a buffer or function passes unevaluated,
    /// a scalar is evaluated once into a temporary.
    pub(super) fn argument(&mut self, argument: &Expr) -> Result<Local, String> {
        if let Some(local) = self.passed(argument) {
            return Ok(local);
        }
        let (ty, value) = self.expression(argument)?;
        Ok(Local::Scalar(ty, self.declare(ty, &value)))
    }

    /// An unevaluated argument: a buffer by alias, a lambda over its lexical
    /// scopes, or a named helper over module scope.
    pub(super) fn passed(&self, argument: &Expr) -> Option<Local> {
        let closure = |parameters: &[Parameter], body: &Expr, scopes| {
            Local::Function(Rc::new(Closure {
                parameters: parameters.to_vec(),
                body: body.clone(),
                scopes,
            }))
        };
        match callee(argument) {
            Expr::Lambda {
                parameters, body, ..
            } => Some(closure(parameters, body, self.scopes.clone())),
            Expr::Identifier(name) => match self.lookup(name) {
                Some(Local::Scalar(..)) => None,
                Some(local) => Some(local),
                None => (self.cg.device.defs.get(name))
                    // Named helpers are defined at module scope; only a
                    // lambda closes over the caller's lexical bindings.
                    .map(|(parameters, body)| closure(parameters, body, Vec::new())),
            },
            _ => None,
        }
    }

    fn length(&mut self, arguments: &[Expr]) -> Emitted {
        match arguments {
            [Expr::Identifier(name)] => match self.lookup(name) {
                Some(Local::Buffer(_, len)) => Ok((Ty::INT, format!("(long){len}"))),
                _ => Err(format!(
                    "gpuLength of `{name}`, which is not a captured buffer"
                )),
            },
            _ => Err("gpuLength of a computed buffer has no device form".into()),
        }
    }

    fn pipe(&mut self, left: &Expr, right: &Expr) -> Emitted {
        match callee(right) {
            Expr::Call {
                function,
                arguments,
                named_arguments,
            } => {
                let mut all = vec![left.clone()];
                all.extend(arguments.iter().cloned());
                self.call(callee(function), &all, named_arguments)
            }
            function @ Expr::Identifier(_) => self.call(function, std::slice::from_ref(left), &[]),
            other => Err(format!(
                "a pipe into {} has no device form",
                describe(other)
            )),
        }
    }

    fn matching(&mut self, value: &Expr, arms: &[MatchArm]) -> Emitted {
        if let Some((buffer, index, fallback)) = elvis_gather(value, arms) {
            return self.gather(buffer, index, fallback);
        }
        let Some((then, otherwise)) = bool_arms(arms) else {
            return Err("a match other than `true`/`false` arms or `gpuGet(...) ?: default` has no device form".into());
        };
        let (ty, condition) = self.expression(value)?;
        if ty != Ty::Bool {
            return Err(format!(
                "a match over a {} scrutinee with boolean arms",
                ty.name()
            ));
        }
        self.branch(&condition, then, otherwise)
    }

    /// `if (condition) then else otherwise` into one temporary.
    fn branch(&mut self, condition: &str, then: &Expr, otherwise: &Expr) -> Emitted {
        let (then_lines, then_ty, then_value) = self.nested(|emitter| emitter.expression(then))?;
        let (else_lines, else_ty, else_value) =
            self.nested(|emitter| emitter.expression(otherwise))?;
        if then_ty != else_ty {
            return Err(format!(
                "arms of a {} and a {}",
                then_ty.name(),
                else_ty.name()
            ));
        }
        let result = self.temp();
        self.line(format!("{} {result};", then_ty.compute_spelling()));
        self.line(format!("if ({condition}) {{"));
        self.indented(then_lines);
        self.line(format!("  {result} = {then_value};"));
        self.line("} else {".into());
        self.indented(else_lines);
        self.line(format!("  {result} = {else_value};"));
        self.line("}".into());
        Ok((then_ty, result))
    }

    fn block(&mut self, statements: &[Stmt], value: Option<&Expr>) -> Emitted {
        self.scopes.push(HashMap::new());
        let result = self.block_contents(statements, value);
        let _ = self.scopes.pop();
        result
    }
}
