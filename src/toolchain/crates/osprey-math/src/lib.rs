//! Math to code ([MATH-CODE], `docs/specs/tooling/MathToCode.md`).
//!
//! Mathematics is written as Lean 4 definitions; this crate reads a fragment
//! of Lean and writes the Osprey module that computes the same values. It
//! refuses, by name, everything it cannot translate exactly.

mod emit;
mod lex;
mod literal;
mod model;
mod names;
mod pin;
mod read;
mod reader;
mod refusal;
mod term;

pub use refusal::{Code, Refusal};

/// The extension of a math source.
pub const EXTENSION: &str = "lean";

/// Generate the Osprey module for one Lean math source. The file's name is
/// quoted in the header and names the module of a source with no `namespace`.
///
/// # Errors
/// The first refusal of [MATH-CODE-REFUSALS]; nothing is generated.
pub fn generate(file: impl AsRef<std::path::Path>, source: &str) -> Result<String, Refusal> {
    let name = file
        .as_ref()
        .file_name()
        .map_or_else(Default::default, |name| name.to_string_lossy());
    let tokens = lex::lex(source)?;
    let stem = name.strip_suffix(".lean").unwrap_or(&name);
    let module = read::read(stem, &tokens)?;
    Ok(emit::emit(&name, &module))
}
