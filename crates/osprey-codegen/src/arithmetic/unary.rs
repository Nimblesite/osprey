//! The unary float functions at each width [BUILTIN-MATH].

use super::single_numeric;
use crate::builder::Codegen;
use crate::conv::as_num;
use crate::error::Result;
use crate::llty::Value;
use osprey_types::numeric::Numeric;

/// The unary float functions [BUILTIN-MATH] and its sections. `sqrt`, `exp`,
/// `ln`, `sin`, `cos`, `log2`, `log10`, `floor`, `ceil`, `trunc` and `round`
/// are LLVM intrinsics (`ln` is `llvm.log`; the name `log` is left to programs,
/// whose loggers want it; `round` is `llvm.roundeven`, which rounds halfway
/// cases to even — Python's `round` and numpy's — never C's half-away `round`);
/// the rest are the C library's symbols (their intrinsics are younger than
/// the oldest toolchain the targets build with). Each is one `double ->
/// double` call a device emitter maps to its own math library.
const FLOAT_FUNCTIONS: [(&str, &str); 25] = [
    ("sqrt", "llvm.sqrt.f64"),
    ("exp", "llvm.exp.f64"),
    ("ln", "llvm.log.f64"),
    ("tanh", "tanh"),
    ("sin", "llvm.sin.f64"),
    ("cos", "llvm.cos.f64"),
    ("tan", "tan"),
    ("asin", "asin"),
    ("acos", "acos"),
    ("atan", "atan"),
    ("sinh", "sinh"),
    ("cosh", "cosh"),
    ("asinh", "asinh"),
    ("acosh", "acosh"),
    ("atanh", "atanh"),
    ("log2", "llvm.log2.f64"),
    ("log10", "llvm.log10.f64"),
    ("log1p", "log1p"),
    ("expm1", "expm1"),
    ("erf", "erf"),
    ("erfc", "erfc"),
    ("floor", "llvm.floor.f64"),
    ("ceil", "llvm.ceil.f64"),
    ("trunc", "llvm.trunc.f64"),
    ("round", "llvm.roundeven.f64"),
];

pub(crate) fn is_float_function(name: &str) -> bool {
    FLOAT_FUNCTIONS.iter().any(|(n, _)| *n == name)
}

/// Apply the float function `name` to `value` at the argument's own width
/// [BUILTIN-MATH]: a `float` argument — including a literal at its default
/// `double` spelling — calls the table's `double` symbol; a `float32`
/// argument calls the `f32` form (the intrinsic's `.f32` spelling or libm's
/// `f` suffix); a `float16` argument widens to binary32 and rounds back once.
pub(crate) fn float_function(cg: &mut Codegen, name: &str, value: Value) -> Result<Value> {
    let Some((_, symbol)) = FLOAT_FUNCTIONS.iter().find(|(n, _)| *n == name) else {
        return Err(crate::error::CodegenError::invalid(format!(
            "{name} is not a float function"
        )));
    };
    let n = single_numeric(&value);
    if n == Numeric::FLOAT16 {
        let wide = as_num(cg, value, Numeric::FLOAT32)?;
        let applied = call_unary_float(cg, &float_symbol_at(symbol, Numeric::FLOAT32), &wide);
        return as_num(cg, applied, n);
    }
    let x = as_num(cg, value, n)?;
    Ok(call_unary_float(cg, &float_symbol_at(symbol, n), &x))
}

/// The symbol a unary or binary float function uses at `n`: the table's
/// `double` form, its `.f32` intrinsic spelling, or libm's `f`-suffixed entry.
pub(crate) fn float_symbol_at(f64_symbol: &str, n: Numeric) -> String {
    if n != Numeric::FLOAT32 {
        return f64_symbol.to_string();
    }
    match f64_symbol.strip_suffix(".f64") {
        Some(intrinsic) => format!("{intrinsic}.f32"),
        None => format!("{f64_symbol}f"),
    }
}

fn call_unary_float(cg: &mut Codegen, symbol: &str, x: &Value) -> Value {
    let ty = x.ty.as_str();
    cg.add_extern(format!("declare {ty} @{symbol}({ty})"));
    let operand = cg.emit_reg(format!("call {ty} @{symbol}({ty} {})", x.operand));
    Value::new(operand, x.ty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llty::LType;

    #[test]
    fn every_unary_float_function_emits_one_double_call() {
        let mut cg = Codegen::new();
        cg.begin_function("math", None);
        for (name, _) in FLOAT_FUNCTIONS {
            let _ = float_function(&mut cg, name, Value::new("1.0", LType::F64)).unwrap();
        }
        cg.emit("ret i64 0");
        cg.finish_function("i64", "math", &[]);
        let ir = cg.render();
        // Each intrinsic or libm symbol is declared and called once per name.
        for (name, symbol) in FLOAT_FUNCTIONS {
            assert!(
                ir.contains(&format!("declare double @{symbol}(double)")),
                "{name}: missing declaration in\n{ir}"
            );
            assert!(
                ir.contains(&format!("call double @{symbol}(double ")),
                "{name}: missing call in\n{ir}"
            );
        }
    }

    /// [BUILTIN-MATH] at `float32`: an intrinsic drops its `.f64` for `.f32`
    /// and a libm name gains the `f` suffix, one `float` call each.
    #[test]
    fn every_unary_float_function_emits_its_f32_entry_point() {
        let mut cg = Codegen::new();
        cg.begin_function("math32", None);
        for (name, _) in FLOAT_FUNCTIONS {
            let _ = float_function(
                &mut cg,
                name,
                Value::new("1.0", LType::Num(Numeric::FLOAT32)),
            )
            .unwrap();
        }
        cg.emit("ret i64 0");
        cg.finish_function("i64", "math32", &[]);
        let ir = cg.render();
        for (name, symbol) in FLOAT_FUNCTIONS {
            let expected = match symbol.strip_suffix(".f64") {
                Some(intrinsic) => format!("{intrinsic}.f32"),
                None => format!("{symbol}f"),
            };
            assert!(
                ir.contains(&format!("declare float @{expected}(float)")),
                "{name}: missing {expected} declaration in\n{ir}"
            );
        }
        assert!(ir.contains("call float @tanhf(float "), "{ir}");
        assert!(ir.contains("call float @llvm.sqrt.f32(float "), "{ir}");
        assert!(!ir.contains("double"), "{ir}");
    }
}
