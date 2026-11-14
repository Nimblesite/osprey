//! Compile the generated tree-sitter parser and the hand-written external scanner.

fn main() {
    let src_dir = std::path::Path::new("src");
    println!("cargo:rerun-if-changed=src/parser.c");
    println!("cargo:rerun-if-changed=src/scanner.c");
    cc::Build::new()
        .include(src_dir)
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .file(src_dir.join("parser.c"))
        // The external scanner decides whether a `(` opens an argument list or the
        // next match arm's tuple pattern (src/scanner.c, [PATTERN-TUPLE]).
        .file(src_dir.join("scanner.c"))
        .compile("tree-sitter-osprey");
}
