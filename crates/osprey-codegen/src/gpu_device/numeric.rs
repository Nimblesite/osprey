//! Native binary32 expressions and word boundaries [GPU-DEVICE-NATIVE].
use super::emit::{Emitted, Emitter};
use super::Ty;
use osprey_ast::Expr;

pub(super) fn fit_literal(expr: &Expr, ty: Ty, value: String, target: Ty) -> (Ty, String) {
    if ty == Ty::FLOAT && target == Ty::FLOAT32 && super::dialect::is_literal(expr) {
        (target, format!("gpu_float32_from_bits(narrow32({value}))"))
    } else {
        (ty, value)
    }
}

pub(super) fn uniform_value(ty: Ty, word: &str) -> String {
    match ty {
        Ty::FLOAT32 => format!("gpu_float32_from_bits((uint)({word}))"),
        _ => ty.value_of(word),
    }
}

pub(super) fn state_word(ty: Ty, value: &str) -> String {
    match ty {
        Ty::FLOAT32 => format!("(ulong)gpu_float32_bits({value})"),
        _ => ty.word_of(value),
    }
}

pub(super) fn convert(target: Ty, source: Ty, value: &str) -> Option<String> {
    match (target, source) {
        (a, b) if a == b => Some(value.to_owned()),
        (Ty::FLOAT32, Ty::FLOAT) => Some(format!("gpu_float32_from_bits(narrow32({value}))")),
        (Ty::FLOAT, Ty::FLOAT32) => Some(format!("widen32(gpu_float32_bits({value}))")),
        (Ty::FLOAT32, Ty::INT) => Some(format!("((float)({value}))")),
        _ => None,
    }
}

pub(super) fn binary(op: &str, left: &str, right: &str) -> Emitted {
    match op {
        "+" | "-" | "*" => Ok((Ty::FLOAT32, format!("((float)(({left}) {op} ({right})))"))),
        "/" => Ok((
            Ty::FLOAT32,
            format!("osprey_fdiv32({left}, {right}, fault)"),
        )),
        "==" | "!=" | "<" | "<=" | ">" | ">=" => {
            Ok((Ty::Bool, format!("(({left}) {op} ({right}))")))
        }
        _ => Err(format!("`{op}` over float32 has no device form")),
    }
}

pub(super) fn builtin(name: &str, values: &[(Ty, String)]) -> Option<Emitted> {
    let [(ty, value)] = values else { return None };
    if matches!(name, "toFloat" | "toFloat32") {
        let target = if name == "toFloat" {
            Ty::FLOAT
        } else {
            Ty::FLOAT32
        };
        return convert(target, *ty, value).map(|text| Ok((target, text)));
    }
    if *ty == Ty::FLOAT32 && matches!(name, "sqrt" | "exp" | "ln" | "tanh" | "abs") {
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
            ("intDiv", [(Ty::INT, a), (Ty::INT, b)]) => {
                unary(Ty::INT, format!("osprey_div({a}, {b}, fault)"))
            }
            ("toFloat", [(Ty::INT, a)]) => unary(Ty::FLOAT, format!("from_long({a})")),
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
            // cast of the word. The sized-float views (`float32Bits`,
            // `float16Bits` and their inverses) and every conversion named for
            // a sized type have no device arm: the match falls through, the
            // emitter declines by name, and the host kernel path runs (plan
            // 0050 phase 5 owns native device arithmetic).
            ("floatBits", [(Ty::FLOAT, a)]) => unary(Ty::INT, a.clone()),
            ("floatFromBits", [(Ty::INT, a)]) => unary(Ty::FLOAT, a.clone()),
            ("abs", [(Ty::INT, a)]) => unary(Ty::INT, format!("osprey_abs({a}, fault)")),
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
