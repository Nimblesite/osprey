//! Lean numeric literals as the decimals Osprey reads ([MATH-CODE-LITERALS]).

use crate::refusal::Code;

/// The largest exponent magnitude read; binary64 ends near 1e308.
const EXPONENT_LIMIT: i64 = 400;

/// The plain decimal for a Lean literal: the same digits, with an exponent
/// applied by moving the point and `.0` appended to an integer.
///
/// # Errors
/// `UnsupportedLiteral` for a form outside the fragment, `LiteralOutOfRange`
/// for a value that is not finite in binary64.
pub(crate) fn decimal(text: &str) -> Result<String, Code> {
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, parse_exponent(exponent)?),
        None => (text, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) {
        return Err(Code::UnsupportedLiteral);
    }
    let point = i64::try_from(whole.len()).map_err(|_error| Code::LiteralOutOfRange)? + exponent;
    finite(&trimmed(&pointed(&format!("{whole}{fraction}"), point)))
}

fn parse_exponent(text: &str) -> Result<i64, Code> {
    let exponent = text
        .parse::<i64>()
        .map_err(|_error| Code::UnsupportedLiteral)?;
    if exponent.abs() > EXPONENT_LIMIT {
        return Err(Code::LiteralOutOfRange);
    }
    Ok(exponent)
}

/// `digits` with the decimal point after its first `point` digits.
fn pointed(digits: &str, point: i64) -> String {
    let zeros = |count: i64| "0".repeat(usize::try_from(count).unwrap_or(0));
    let length = i64::try_from(digits.len()).unwrap_or(i64::MAX);
    if point <= 0 {
        return format!("0.{}{digits}", zeros(-point));
    }
    if point >= length {
        return format!("{digits}{}.0", zeros(point - length));
    }
    let (whole, fraction) = digits.split_at(usize::try_from(point).unwrap_or(0));
    format!("{whole}.{fraction}")
}

/// Without redundant leading zeros: `007.5` is `7.5` and `000.5` is `0.5`.
fn trimmed(decimal: &str) -> String {
    let rest = decimal.trim_start_matches('0');
    if rest.starts_with('.') {
        format!("0{rest}")
    } else {
        rest.to_owned()
    }
}

fn finite(decimal: &str) -> Result<String, Code> {
    match decimal.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(decimal.to_owned()),
        _ => Err(Code::LiteralOutOfRange),
    }
}

/// Whether a literal the emitter wrote is zero, so a division by it keeps
/// Lean's IEEE meaning only through `floatDiv`.
pub(crate) fn is_zero(decimal: &str) -> bool {
    decimal.parse::<f64>().map_or(true, |value| value == 0.0)
}
