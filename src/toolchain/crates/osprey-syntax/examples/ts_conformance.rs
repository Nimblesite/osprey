//! The TypeScript parser conformance check ([FLAVOR-TS-CST]): for each TypeScript file
//! named on stdin, print whether the pinned `oxc_parser` accepts it and
//! whether the TypeScript-flavor frontend lowered it without panicking, as one
//! tab-separated line. `scripts/typescript/conformance.mjs` compares the
//! verdicts with `tsc` 7 and writes the agreement table.
use std::io::{BufRead, Write};

fn verdict(path: &str) -> String {
    let Ok(source) = std::fs::read_to_string(path) else {
        return format!("{path}\tunreadable\tunreadable");
    };
    let allocator = oxc_allocator::Allocator::default();
    let parsed = oxc_parser::Parser::new(&allocator, &source, oxc_span::SourceType::ts()).parse();
    let oxc = if parsed.diagnostics.errors().next().is_none() {
        "accept"
    } else {
        "reject"
    };
    let frontend = match std::panic::catch_unwind(|| {
        osprey_syntax::parse_program_with_flavor(&source, osprey_syntax::Flavor::TypeScript)
    }) {
        Ok(_) => "ok",
        Err(_) => "panic",
    };
    format!("{path}\t{oxc}\t{frontend}")
}

fn main() -> std::io::Result<()> {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for line in std::io::stdin().lock().lines() {
        writeln!(out, "{}", verdict(line?.trim()))?;
    }
    Ok(())
}
