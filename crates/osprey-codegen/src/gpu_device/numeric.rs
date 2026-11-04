//! Native binary32 expressions, conversions and word boundaries
//! [GPU-DEVICE-NATIVE]. Sized integers compute at their own width through the
//! checked helpers of `gpu_device_lib.metal`; `float16` computes in binary32
//! and narrows once at the store boundary.
use super::emit::{Emitted, Emitter};
use super::Ty;
use osprey_ast::Expr;
use osprey_types::numeric::Numeric;

/// A float literal travels at the target's compute width.
pub(super) fn fit_literal(expr: &Expr, ty: Ty, value: String, target: Ty) -> (Ty, String) {
    if ty == Ty::FLOAT && target == Ty::FLOAT32 && super::dialect::is_literal(expr) {
        (target, format!("gpu_float32_from_bits(narrow32({value}))"))
    } else {
        (ty, value)
    }
}

/// The checked-helper suffix of an integer type: `_i8`, `_u16`, empty for `int`.
pub(super) fn suffix(n: Numeric) -> &'static str {
    match (n.is_signed(), n.bits) {
        (true, 8) => "_i8",
        (true, 16) => "_i16",
        (true, 32) => "_i32",
        (true, _) => "",
        (false, 8) => "_u8",
        (false, 16) => "_u16",
        (false, _) => "_u32",
    }
}

pub(super) fn uniform_value(ty: Ty, word: &str) -> String {
    match ty {
        Ty::FLOAT32 => format!("gpu_float32_from_bits((uint)({word}))"),
        Ty::Num(n) if n.is_float() && n.bits == 16 => {
            format!("gpu_f16_to_f32((ushort)({word}))")
        }
        _ => ty.value_of(word),
    }
}

pub(super) fn state_word(ty: Ty, value: &str) -> String {
    match ty {
        Ty::FLOAT32 => format!("(ulong)gpu_float32_bits({value})"),
        Ty::Num(n) if n.is_float() && n.bits == 16 => {
            format!("(ulong)gpu_f32_to_f16({value})")
        }
        _ => ty.word_of(value),
    }
}

/// One kernel conversion [BUILTIN-CONVERT-FLOAT] [BUILTIN-CONVERT-INT]:
/// `toFloat32` of a `float` is `narrow32`, `toFloat` of a `float32` its
/// `widen32`; every other pair is the device's cast, and a `float16` target
/// rounds through binary32 once ([GPU-DEVICE-NATIVE]). Returns the compute
/// type the converted value travels at.
pub(super) fn convert(target: Ty, source: Ty, value: &str) -> Option<(Ty, String)> {
    let Ty::Num(t) = target else { return None };
    let Ty::Num(s) = source else { return None };
    let compute = target.compute();
    let text = if t == s {
        value.to_owned()
    } else {
        match (t, s) {
            (Numeric::FLOAT32, Numeric::FLOAT) => format!("gpu_float32_from_bits(narrow32({value}))"),
            (Numeric::FLOAT, Numeric::FLOAT32) => format!("widen32(gpu_float32_bits({value}))"),
            (Numeric::FLOAT16, Numeric::FLOAT32) => format!("gpu_f32_narrow16({value})"),
            (Numeric::FLOAT16, Numeric::FLOAT) => {
                format!("gpu_f16_to_f32(gpu_f64_to_f16({value}))")
            }
            (Numeric::FLOAT16, _) if s.is_integer() => {
                format!("gpu_f16_to_f32(gpu_f64_to_f16(from_long((long)({value}))))")
            }
            (Numeric::FLOAT, _) if s.is_integer() => format!("from_long((long)({value}))"),
            (Numeric::FLOAT32, _) if s.is_integer() => format!("((float)({value}))"),
            (_, _) if t.is_integer() && s.is_integer() => format!("({})({value})", t.device()),
            _ => return None,
        }
    };
    Some((compute, text))
}

/// A conversion named for a numeric type, usable as a kernel by name.
fn conversion(name: &str) -> Option<Numeric> {
    Numeric::conversion_target(name)
}

/// Unary builtins whose device form exists for every scalar type: the named
/// conversions and the binary32 math entries.
pub(super) fn builtin(name: &str, values: &[(Ty, String)]) -> Option<Emitted> {
    let [(ty, value)] = values else { return None };
    if let Some(target) = conversion(name) {
        return convert(Ty::Num(target), *ty, value).map(|(ty, text)| Ok((ty, text)));
    }
    if matches!(*ty, Ty::Num(Numeric::FLOAT32)) && matches!(name, "sqrt" | "exp" | "ln" | "tanh" | "abs") {
        return Some(Ok((*ty, format!("gpu32_{name}({value})"))));
    }
    None
}

impl Emitter<'_> {
    pub(super) fn with_float_context(
        &mut self,
        target: Ty,
        work: impl FnOnce(&mut Self) -> Emitted,
    ) -> Emitted {
        let previous = std::mem::replace(&mut self.default_float, target);
        let result = work(self);
        self.default_float = previous;
        result
    }

    pub(super) fn builtin(&mut self, name: &str, arguments: &[Expr]) -> Emitted {
        let values = arguments
            .iter()
            .map(|argument| self.expression(argument))
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(result) = builtin(name, &values) {
            return result;
        }
        let unary = |ty: Ty, text: String| Ok((ty, text));
        match (name, values.as_slice()) {
            ("intDiv", [(Ty::Num(n), a), (Ty::Num(m), b)]) if n == m && n.is_integer() => {
                unary(Ty::Num(*n), format!("osprey_div{}({a}, {b}, fault)", suffix(*n)))
            }
            ("sqrt" | "exp" | "ln" | "tanh", [(Ty::FLOAT, a)]) => {
                unary(Ty::FLOAT, format!("{name}64({a})"))
            }
            // The unary float functions of [BUILTIN-MATH] and its sections:
            // one call into the device math library, total and pure.
            (
                "sin" | "cos" | "tan" | "asin" | "acos" | "atan" | "sinh" | "cosh" | "asinh"
                | "acosh" | "atanh" | "log2" | "log10" | "log1p" | "expm1" | "erf" | "erfc"
                | "floor" | "ceil" | "trunc" | "round",
                [(Ty::FLOAT, a)],
            ) => unary(Ty::FLOAT, format!("{name}64({a})")),
            // Binary float functions [BUILTIN-MATH-TRIG] [BUILTIN-MATH-LOG]
            // [BUILTIN-MATH-ROUNDING].
            ("pow" | "atan2" | "hypot" | "fmod" | "copysign", [(Ty::FLOAT, a), (Ty::FLOAT, b)]) => {
                unary(Ty::FLOAT, format!("{name}64({a}, {b})"))
            }
            // [BUILTIN-FLOAT-BITS] the bit-level views. A device float IS its
            // binary64 word, so floatBits is the identity and floatFromBits a
            // cast of the word.
            ("floatBits", [(Ty::FLOAT, a)]) => unary(Ty::INT, a.clone()),
            ("floatFromBits", [(Ty::INT, a)]) => unary(Ty::FLOAT, a.clone()),
            ("abs", [(Ty::Num(n), a)]) if n.is_integer() => {
                unary(Ty::Num(*n), format!("osprey_abs{}({a}, fault)", suffix(*n)))
            }
            ("abs", [(Ty::FLOAT, a)]) => unary(Ty::FLOAT, format!("abs64({a})")),
            // [BUILTIN-BITWISE] the word operations; the shifts are total on
            // the device exactly as on the host.
            ("bitAnd", [(Ty::INT, a), (Ty::INT, b)]) => unary(Ty::INT, format!("(({a}) & ({b}))")),
            ("bitOr", [(Ty::INT, a), (Ty::INT, b)]) => unary(Ty::INT, format!("(({a}) | ({b}))")),
            ("bitXor", [(Ty::INT, a), (Ty::INT, b)]) => unary(Ty::INT, format!("(({a}) ^ ({b}))")),
            ("bitNot", [(Ty::INT, a)]) => unary(Ty::INT, format!("(~({a}))")),
            ("shiftLeft", [(Ty::INT, a), (Ty::INT, b)]) => {
                unary(Ty::INT, format!("shl64({a}, {b})"))
            }
            ("shiftRight", [(Ty::INT, a), (Ty::INT, b)]) => {
                unary(Ty::INT, format!("shr64({a}, {b})"))
            }
            ("shiftRightLogical", [(Ty::INT, a), (Ty::INT, b)]) => {
                unary(Ty::INT, format!("shru64({a}, {b})"))
            }
            ("wrapAdd", [(Ty::INT, a), (Ty::INT, b)]) => {
                unary(Ty::INT, format!("(long)((ulong)({a}) + (ulong)({b}))"))
            }
            ("wrapSub", [(Ty::INT, a), (Ty::INT, b)]) => {
                unary(Ty::INT, format!("(long)((ulong)({a}) - (ulong)({b}))"))
            }
            ("wrapMul", [(Ty::INT, a), (Ty::INT, b)]) => {
                unary(Ty::INT, format!("(long)((ulong)({a}) * (ulong)({b}))"))
            }
            _ => Err(format!("`{name}` has no device form")),
        }
    }
}
