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
use super::{DeviceBody, Ty, Uniform};
use crate::builder::Codegen;
use osprey_ast::{Expr, MatchArm, NamedArgument, Parameter, Stmt};
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
}

/// Helper inlining deeper than this is treated as recursion.
pub(super) const MAX_INLINE_DEPTH: usize = 24;

/// Compile `body` with the kernel's parameters bound to `p0`, `p1`, … and
/// its captures to `u0`, `u1`, ….
pub(super) fn body(
    cg: &Codegen,
    parameters: &[Parameter],
    body: &Expr,
    params: &[Ty],
    captures: &[(String, Uniform)],
) -> Result<DeviceBody, String> {
    if parameters.len() != params.len() {
        return Err("the kernel's arity disagrees with the combinator's slots".into());
    }
    let mut emitter = Emitter {
        cg,
        blocks: vec![Vec::new()],
        temps: 0,
        scopes: vec![HashMap::new()],
        depth: 0,
    };
    for (i, (parameter, ty)) in parameters.iter().zip(params).enumerate() {
        emitter.bind(&parameter.name, Local::Scalar(*ty, format!("p{i}")));
    }
    for (i, (name, uniform)) in captures.iter().enumerate() {
        emitter.bind(name, uniform_local(i, uniform));
    }
    let (result_ty, result) = emitter.expression(body)?;
    Ok(DeviceBody {
        lines: emitter.blocks.pop().unwrap_or_default(),
        result,
        result_ty,
        params: params.to_vec(),
        uniforms: captures
            .iter()
            .map(|(_, uniform)| uniform.clone())
            .collect(),
    })
}

fn uniform_local(i: usize, uniform: &Uniform) -> Local {
    match uniform {
        Uniform::Scalar(ty) => Local::Scalar(*ty, format!("u{i}_v")),
        Uniform::Buffer => Local::Buffer(format!("u{i}"), format!("u{i}_len")),
    }
}

impl Emitter<'_> {
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
        self.line(format!("{} {name} = {value};", ty.spelling()));
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
            Expr::Integer(n) => Ok((Ty::Int, int_literal(*n))),
            Expr::Float(f) => Ok((Ty::Float, float_literal(*f))),
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
                Some(literal) => self.expression(&literal),
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
        match (left_ty, right_ty) {
            (Ty::Int, Ty::Int) => int_binary(op, &l, &r),
            (Ty::Float, Ty::Float) => float_binary(op, &l, &r),
            (Ty::Bool, Ty::Bool) if op == "==" || op == "!=" => {
                Ok((Ty::Bool, format!("({l} {op} {r})")))
            }
            _ => Err(format!("`{op}` over {left_ty:?} and {right_ty:?} operands")),
        }
    }

    /// `&&`/`||` keep the host's short circuit, so a fallible operation on the
    /// right never runs when the left decides the answer.
    fn short_circuit(&mut self, op: &str, left: &Expr, right: &Expr) -> Emitted {
        let (ty, condition) = self.expression(left)?;
        if ty != Ty::Bool {
            return Err(format!("`{op}` over a {ty:?} operand"));
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
            ("-", Ty::Int) => Ok((Ty::Int, format!("osprey_neg({value}, fault)"))),
            ("-", Ty::Float) => Ok((Ty::Float, format!("neg64({value})"))),
            ("!" | "not", Ty::Bool) => Ok((Ty::Bool, format!("(!{value})"))),
            _ => Err(format!("unary `{op}` over a {ty:?} operand")),
        }
    }

    fn call(&mut self, function: &Expr, arguments: &[Expr], named: &[NamedArgument]) -> Emitted {
        if !named.is_empty() {
            return Err("named arguments have no device form".into());
        }
        let Expr::Identifier(name) = function else {
            return Err("a computed callee has no device form".into());
        };
        match (self.lookup(name), name.as_str()) {
            (Some(Local::Function(closure)), _) => self.invoke(name, &closure, arguments),
            (Some(_), _) => Err(format!(
                "`{name}` is a kernel value, not a function the device can call"
            )),
            (None, "fold") => self.fold(arguments),
            (None, "gpuLength") => self.length(arguments),
            (None, _) if self.cg.device.defs.contains_key(name) => self.inline(name, arguments),
            (None, _) => self.builtin(name, arguments),
        }
    }

    fn builtin(&mut self, name: &str, arguments: &[Expr]) -> Emitted {
        let values = arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect::<Result<Vec<_>, _>>()?;
        let unary = |ty: Ty, text: String| Ok((ty, text));
        match (name, values.as_slice()) {
            ("intDiv", [(Ty::Int, a), (Ty::Int, b)]) => {
                unary(Ty::Int, format!("osprey_div({a}, {b}, fault)"))
            }
            ("toFloat", [(Ty::Int, a)]) => unary(Ty::Float, format!("from_long({a})")),
            ("sqrt" | "exp" | "ln" | "tanh", [(Ty::Float, a)]) => {
                unary(Ty::Float, format!("{name}64({a})"))
            }
            // The unary float functions of [BUILTIN-MATH] and its sections:
            // one call into the device math library, total and pure.
            (
                "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "asinh"
                | "acosh" | "atanh" | "log2" | "log10" | "log1p" | "expm1" | "erf" | "erfc"
                | "floor" | "ceil" | "trunc" | "round",
                [(Ty::Float, a)],
            ) => unary(Ty::Float, format!("{name}64({a})")),
            // Binary float functions [BUILTIN-MATH-TRIG] [BUILTIN-MATH-LOG]
            // [BUILTIN-MATH-ROUNDING].
            ("pow" | "atan2" | "hypot" | "fmod" | "copysign", [(Ty::Float, a), (Ty::Float, b)]) => {
                unary(Ty::Float, format!("{name}64({a}, {b})"))
            }
            // [BUILTIN-FLOAT32] the binary32 emulation; [BUILTIN-FLOAT-BITS]
            // the bit-level views. A device float IS its binary64 word, so
            // floatBits is the identity and floatFromBits a cast of the word.
            ("float32", [(Ty::Float, a)]) => unary(Ty::Float, format!("float32_64({a})")),
            ("floatBits", [(Ty::Float, a)]) => unary(Ty::Int, a.clone()),
            ("floatFromBits", [(Ty::Int, a)]) => unary(Ty::Float, a.clone()),
            ("float32Bits", [(Ty::Float, a)]) => unary(Ty::Int, format!("float32_bits({a})")),
            ("float32FromBits", [(Ty::Int, a)]) => {
                unary(Ty::Float, format!("float32_from_bits({a})"))
            }
            ("abs", [(Ty::Int, a)]) => unary(Ty::Int, format!("osprey_abs({a}, fault)")),
            ("abs", [(Ty::Float, a)]) => unary(Ty::Float, format!("abs64({a})")),
            // [BUILTIN-BITWISE] the word operations; the shifts are total on
            // the device exactly as on the host.
            ("bitAnd", [(Ty::Int, a), (Ty::Int, b)]) => unary(Ty::Int, format!("(({a}) & ({b}))")),
            ("bitOr", [(Ty::Int, a), (Ty::Int, b)]) => unary(Ty::Int, format!("(({a}) | ({b}))")),
            ("bitXor", [(Ty::Int, a), (Ty::Int, b)]) => unary(Ty::Int, format!("(({a}) ^ ({b}))")),
            ("bitNot", [(Ty::Int, a)]) => unary(Ty::Int, format!("(~({a}))")),
            ("shiftLeft", [(Ty::Int, a), (Ty::Int, b)]) => {
                unary(Ty::Int, format!("shl64({a}, {b})"))
            }
            ("shiftRight", [(Ty::Int, a), (Ty::Int, b)]) => {
                unary(Ty::Int, format!("shr64({a}, {b})"))
            }
            ("shiftRightLogical", [(Ty::Int, a), (Ty::Int, b)]) => {
                unary(Ty::Int, format!("shru64({a}, {b})"))
            }
            ("wrapAdd", [(Ty::Int, a), (Ty::Int, b)]) => {
                unary(Ty::Int, format!("(long)((ulong)({a}) + (ulong)({b}))"))
            }
            ("wrapSub", [(Ty::Int, a), (Ty::Int, b)]) => {
                unary(Ty::Int, format!("(long)((ulong)({a}) - (ulong)({b}))"))
            }
            ("wrapMul", [(Ty::Int, a), (Ty::Int, b)]) => {
                unary(Ty::Int, format!("(long)((ulong)({a}) * (ulong)({b}))"))
            }
            _ => Err(format!("`{name}` has no device form")),
        }
    }

    /// Inline a helper: each argument is bound once, in order, then the body
    /// is emitted in a scope holding the parameters.
    fn inline(&mut self, name: &str, arguments: &[Expr]) -> Emitted {
        let Some((parameters, body)) = self.cg.device.defs.get(name).cloned() else {
            return Err(format!("`{name}` is not a function the device can inline"));
        };
        let bindings = self.bindings(name, &parameters, arguments)?;
        self.apply(&body, bindings)
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
            return Err(format!("`{name}` recurses, which a device kernel cannot"));
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
                Some(Local::Buffer(_, len)) => Ok((Ty::Int, format!("(long){len}"))),
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
            return Err(format!("a match over a {ty:?} scrutinee with boolean arms"));
        }
        self.branch(&condition, then, otherwise)
    }

    /// `if (condition) then else otherwise` into one temporary.
    fn branch(&mut self, condition: &str, then: &Expr, otherwise: &Expr) -> Emitted {
        let (then_lines, then_ty, then_value) = self.nested(|emitter| emitter.expression(then))?;
        let (else_lines, else_ty, else_value) =
            self.nested(|emitter| emitter.expression(otherwise))?;
        if then_ty != else_ty {
            return Err(format!("arms of a {then_ty:?} and a {else_ty:?}"));
        }
        let result = self.temp();
        self.line(format!("{} {result};", then_ty.spelling()));
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

    fn block_contents(&mut self, statements: &[Stmt], value: Option<&Expr>) -> Emitted {
        for statement in statements {
            self.statement(statement)?;
        }
        match value {
            Some(value) => self.expression(value),
            None => Err("a block without a value has no device form".into()),
        }
    }

    fn statement(&mut self, statement: &Stmt) -> Result<(), String> {
        let Stmt::Let {
            name,
            mutable: false,
            value,
            ..
        } = statement
        else {
            return Err("a statement other than an immutable `let` has no device form".into());
        };
        let local = self.argument(value)?;
        self.bind(name, local);
        Ok(())
    }
}
