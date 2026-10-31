//! The compiler knows no discipline ([TYPES-DISCIPLINE-REGISTRY]): no code
//! under `crates/` names a registered discipline in a string literal or
//! mentions the theory words `tactic`, `theorem`, `refinement`, `lean4`.
//! Registered names come from the reference packages under
//! `examples/disciplines/` and the first consumer of spec 0049, so adding a
//! discipline extends the search without touching this test.
//!
//! Each file is scanned as one piece of text, never line by line, so nothing
//! hides behind punctuation inside a literal. Line comments (`//…`, to the
//! newline) and block comments (`/* … */`, nested in Rust) are dropped; the
//! text of `"…"`, `r"…"`, `r#"…"#`, `b"…"`, `br#"…"#`, of char literals and
//! of lifetimes (`'a`) is code, escapes honoured. Per source line the scan
//! yields that code plus the body of every string literal starting on it:
//! theory words match as whole words in the code, string contents included,
//! while registered names match only in bodies. `kernel` is exempt: an
//! Osprey keyword ([STAGE-KERNEL]) used throughout the GPU pipeline, with the
//! proof kernel of plan 0041 under `examples/`, never under `crates/`.
//! Names are matched in literals only because `bounded` and `passthrough` are
//! ordinary English, and a branch on a discipline compares a literal.
//! Comments and test modules are skipped: a comment cannot branch, and test
//! fixtures are allowed by the spec.

#![expect(
    clippy::expect_used,
    reason = "test assertions: an unreadable tree is a test failure, not a production panic"
)]

use std::path::{Path, PathBuf};

const THEORY_WORDS: [&str; 4] = ["tactic", "theorem", "refinement", "lean4"];
const FIRST_CONSUMER: &str = "lean4";
const SOURCE_EXTENSIONS: [&str; 3] = ["rs", "c", "h"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root")
}

/// `name = "<word>"` of every `[discipline]` table under `examples/disciplines`.
fn registered_names(root: &Path) -> Vec<String> {
    let mut names = vec![FIRST_CONSUMER.to_string()];
    let packages = root.join("examples/disciplines");
    for entry in std::fs::read_dir(&packages).into_iter().flatten().flatten() {
        let manifest = entry.path().join("osprey.toml");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            names.extend(discipline_name(&text));
        }
    }
    names
}

fn discipline_name(manifest: &str) -> Option<String> {
    let table = manifest.split("[discipline]").nth(1)?;
    table
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("name"))
        .and_then(|rest| rest.split('"').nth(1))
        .map(ToString::to_string)
}

fn is_test_file(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains("/tests/") || text.ends_with("tests.rs") || text.contains("_tests/")
}

fn is_rust(path: &Path) -> bool {
    path.extension().and_then(|extension| extension.to_str()) == Some("rs")
}

fn source_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                source_files(&path, out);
            }
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| SOURCE_EXTENSIONS.contains(&extension))
            && !is_test_file(&path)
        {
            out.push(path);
        }
    }
}

/// One scanned source line: code with comments removed, and its literals.
struct ScannedLine {
    number: usize,
    code: String,
    literals: Vec<String>,
}

/// A scan in progress: the code text so far, literals with their start line.
struct Scan {
    code: Vec<u8>,
    literals: Vec<(usize, String)>,
    line: usize,
}

impl Scan {
    /// Keep code bytes, counting the newlines among them.
    fn push(&mut self, bytes: &[u8]) {
        for byte in bytes {
            if *byte == b'\n' {
                self.line += 1;
            }
        }
        self.code.extend_from_slice(bytes);
    }

    /// Keep a comment byte only if it is a newline, so line numbers survive.
    fn keep_newline(&mut self, byte: u8) {
        if byte == b'\n' {
            self.push(&[byte]);
        }
    }

    /// Record a literal body against the line the literal starts on.
    fn push_literal(&mut self, body: &[u8]) {
        let body = String::from_utf8_lossy(body).into_owned();
        self.literals.push((self.line, body));
    }

    /// Group the scan into source lines, numbered from one.
    fn lines(self) -> Vec<ScannedLine> {
        let code = String::from_utf8_lossy(&self.code).into_owned();
        code.lines()
            .enumerate()
            .map(|(index, text)| ScannedLine {
                number: index + 1,
                code: text.to_string(),
                literals: self
                    .literals
                    .iter()
                    .filter(|(line, _)| *line == index + 1)
                    .map(|(_, body)| body.clone())
                    .collect(),
            })
            .collect()
    }
}

fn at(bytes: &[u8], index: usize) -> Option<u8> {
    bytes.get(index).copied()
}

/// The bytes of `range`, or an empty slice when it is out of bounds.
fn slice(bytes: &[u8], range: (usize, usize)) -> &[u8] {
    bytes.get(range.0..range.1).unwrap_or_default()
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn ident_end(bytes: &[u8], start: usize) -> usize {
    let rest = bytes.get(start..).unwrap_or_default();
    start
        + rest
            .iter()
            .take_while(|byte| is_ident_continue(**byte))
            .count()
}

/// Whether a token rather than an identifier continuation begins at `index`.
fn is_token_start(bytes: &[u8], index: usize) -> bool {
    index
        .checked_sub(1)
        .and_then(|previous| at(bytes, previous))
        .is_none_or(|byte| !is_ident_continue(byte))
}

fn hashes(bytes: &[u8], start: usize) -> usize {
    let rest = bytes.get(start..).unwrap_or_default();
    rest.iter().take_while(|byte| **byte == b'#').count()
}

/// Scan a whole file into numbered lines, after its first `#[cfg(test)]`.
fn scan(text: &str, rust: bool) -> Vec<ScannedLine> {
    let source = text.split("#[cfg(test)]").next().unwrap_or("").as_bytes();
    let mut state = Scan {
        code: Vec::new(),
        literals: Vec::new(),
        line: 1,
    };
    let mut index = 0;
    while index < source.len() {
        index = dispatch(&mut state, source, index, rust);
    }
    state.lines()
}

/// Handle the construct at `index`; ordinary code is copied byte by byte.
fn dispatch(scan: &mut Scan, bytes: &[u8], index: usize, rust: bool) -> usize {
    if let Some(end) = comment(scan, bytes, index, rust) {
        return end;
    }
    if let Some(end) = literal(scan, bytes, index, rust) {
        return end;
    }
    scan.push(slice(bytes, (index, index + 1)));
    index + 1
}

/// Skip a comment beginning at `index`, keeping only its newlines.
fn comment(scan: &mut Scan, bytes: &[u8], index: usize, rust: bool) -> Option<usize> {
    match (at(bytes, index), at(bytes, index + 1)) {
        (Some(b'/'), Some(b'/')) => {
            let newline = slice(bytes, (index, bytes.len()))
                .iter()
                .position(|byte| *byte == b'\n');
            Some(newline.map_or(bytes.len(), |offset| index + offset))
        }
        (Some(b'/'), Some(b'*')) => Some(block_comment(scan, bytes, index + 2, rust)),
        _ => None,
    }
}

/// Skip a block comment body from `start`; Rust block comments nest.
fn block_comment(scan: &mut Scan, bytes: &[u8], start: usize, rust: bool) -> usize {
    let mut index = start;
    let mut depth = 1;
    while let Some(byte) = at(bytes, index) {
        if byte == b'*' && at(bytes, index + 1) == Some(b'/') {
            depth -= 1;
            index += 2;
            if depth == 0 {
                return index;
            }
        } else if rust && byte == b'/' && at(bytes, index + 1) == Some(b'*') {
            depth += 1;
            index += 2;
        } else {
            scan.keep_newline(byte);
            index += 1;
        }
    }
    index
}

/// Record the literal starting at `index`, or `None` for ordinary code.
fn literal(scan: &mut Scan, bytes: &[u8], index: usize, rust: bool) -> Option<usize> {
    match at(bytes, index)? {
        b'"' => {
            let (body_end, end) = plain_end(bytes, index + 1);
            Some(record(scan, bytes, index, (index + 1, body_end), end))
        }
        b'\'' => {
            let end = char_end(bytes, index, rust)?;
            scan.push(slice(bytes, (index, end)));
            Some(end)
        }
        b'r' | b'b' if rust && is_token_start(bytes, index) && starts_raw(bytes, index) => {
            Some(raw_literal(scan, bytes, index))
        }
        b'b' if rust && is_token_start(bytes, index) => byte_literal(scan, bytes, index),
        _ => None,
    }
}

/// Keep a literal's text in the code and record its body as a string literal.
fn record(scan: &mut Scan, bytes: &[u8], start: usize, body: (usize, usize), end: usize) -> usize {
    scan.push_literal(slice(bytes, body));
    scan.push(slice(bytes, (start, end)));
    end
}

/// Record a byte string (`b"…"`) at `index`; `None` for any other `b` token.
fn byte_literal(scan: &mut Scan, bytes: &[u8], index: usize) -> Option<usize> {
    if at(bytes, index + 1) != Some(b'"') {
        return None;
    }
    let (body_end, end) = plain_end(bytes, index + 2);
    Some(record(scan, bytes, index, (index + 2, body_end), end))
}

/// The body end and the end of a `"`-delimited literal, `\` escaping one byte.
fn plain_end(bytes: &[u8], start: usize) -> (usize, usize) {
    let mut index = start;
    while let Some(byte) = at(bytes, index) {
        match byte {
            b'\\' => index += 2,
            b'"' => return (index, index + 1),
            _ => index += 1,
        }
    }
    (bytes.len(), bytes.len())
}

/// Whether a Rust raw string (`r"…"`, `r#"…"#`, `br#"…"#`) starts at `index`.
fn starts_raw(bytes: &[u8], index: usize) -> bool {
    let prefix = if at(bytes, index) == Some(b'b') { 2 } else { 1 };
    at(bytes, index + prefix + hashes(bytes, index + prefix)) == Some(b'"')
}

/// Record a raw string at `index`, whose body may hold `//` and bare quotes.
fn raw_literal(scan: &mut Scan, bytes: &[u8], index: usize) -> usize {
    let prefix = if at(bytes, index) == Some(b'b') { 2 } else { 1 };
    let wanted = hashes(bytes, index + prefix);
    let body_start = index + prefix + wanted + 1;
    let (body_end, end) = raw_end(bytes, body_start, wanted);
    record(scan, bytes, index, (body_start, body_end), end)
}

/// The body end and the end of a raw string closed by `wanted` `#`s.
fn raw_end(bytes: &[u8], start: usize, wanted: usize) -> (usize, usize) {
    let mut index = start;
    while let Some(byte) = at(bytes, index) {
        if byte == b'"' && hashes(bytes, index + 1) >= wanted {
            return (index, index + 1 + wanted);
        }
        index += 1;
    }
    (bytes.len(), bytes.len())
}

/// The end of the char literal at `index`, or `None` for a Rust lifetime.
fn char_end(bytes: &[u8], index: usize, rust: bool) -> Option<usize> {
    if at(bytes, index + 1) == Some(b'\\') {
        return escaped_end(bytes, index);
    }
    if !rust {
        return c_char_end(bytes, index);
    }
    if is_ident_start(at(bytes, index + 1)?) {
        let after = ident_end(bytes, index + 1);
        return (at(bytes, after) == Some(b'\'')).then_some(after + 1);
    }
    (at(bytes, index + 2) == Some(b'\'')).then_some(index + 3)
}

/// The end of a C char constant, which may hold several characters.
fn c_char_end(bytes: &[u8], index: usize) -> Option<usize> {
    let mut index = index + 1;
    while let Some(byte) = at(bytes, index) {
        match byte {
            b'\'' => return Some(index + 1),
            b'\n' => return None,
            _ => index += 1,
        }
    }
    None
}

/// The end of an escaped char literal such as `'\''` or `'\u{1F600}'`.
fn escaped_end(bytes: &[u8], index: usize) -> Option<usize> {
    let mut index = index + 3;
    let escaped = |byte: u8| is_ident_continue(byte) || matches!(byte, b'{' | b'}');
    while at(bytes, index).is_some_and(escaped) {
        index += 1;
    }
    (at(bytes, index) == Some(b'\'')).then_some(index + 1)
}

fn contains_word(text: &str, word: &str) -> bool {
    let boundary = |byte: Option<u8>| byte.is_none_or(|byte| !is_ident_continue(byte));
    text.match_indices(word).any(|(start, _)| {
        boundary(text.as_bytes().get(start.wrapping_sub(1)).copied())
            && boundary(text.as_bytes().get(start + word.len()).copied())
    })
}

/// Why a line is a hit: a theory word in code, or a name in a literal body.
fn offending<Name: AsRef<str>>(line: &ScannedLine, names: &[Name]) -> Option<String> {
    if let Some(word) = THEORY_WORDS
        .iter()
        .find(|word| contains_word(&line.code, word))
    {
        return Some(format!("theory word `{word}`"));
    }
    for name in names {
        let name = name.as_ref();
        if line
            .literals
            .iter()
            .any(|literal| contains_word(literal, name))
        {
            return Some(format!("discipline name `{name}` in a string literal"));
        }
    }
    None
}

/// The source line numbers of `text`, scanned as Rust, that name `name`.
fn findings(text: &str, name: &str) -> Vec<usize> {
    scan(text, true)
        .iter()
        .filter(|line| offending(line, &[name]).is_some())
        .map(|line| line.number)
        .collect()
}

/// Every hit under `files`, with the number of lines and literals scanned.
fn scan_tree(files: &[PathBuf], names: &[String]) -> (Vec<String>, (usize, usize)) {
    let mut hits = Vec::new();
    let mut seen = (0, 0);
    for path in files {
        let text = std::fs::read_to_string(path).expect("read source");
        for line in scan(&text, is_rust(path)) {
            seen = (seen.0 + 1, seen.1 + line.literals.len());
            if let Some(reason) = offending(&line, names) {
                hits.push(format!("{}:{}: {reason}", path.display(), line.number));
            }
        }
    }
    (hits, seen)
}

#[test]
fn no_code_under_crates_names_a_discipline_or_its_theory() {
    let root = repo_root();
    let names = registered_names(&root);
    assert!(names.len() >= 3, "registries found: {names:?}");
    let mut files = Vec::new();
    source_files(&root.join("crates"), &mut files);
    assert!(files.len() > 100, "the walk found {} files", files.len());
    let (hits, (lines, literals)) = scan_tree(&files, &names);
    assert!(
        lines > 10_000 && literals > 1_000,
        "saw {lines} lines, {literals} literals"
    );
    assert!(
        hits.is_empty(),
        "discipline knowledge below the seam:\n{}",
        hits.join("\n")
    );
}

#[test]
fn the_scan_sees_a_literal_and_ignores_comments_and_english() {
    assert_eq!(findings("let x = select(\"bounded\");", "bounded"), vec![1]);
    let english = "let bounded = workers.len();";
    assert_eq!(findings(english, "bounded"), Vec::new());
    assert_eq!(findings("prove(theorem_count)", "bounded"), Vec::new());
    assert_eq!(findings("let theorem = 1;", "bounded"), vec![1]);
    let text = "fn a() {} // a theorem here\n#[cfg(test)]\nmod t { const N: &str = \"bounded\"; }";
    assert_eq!(findings(text, "bounded"), Vec::new());
}

#[test]
fn the_scan_sees_a_theory_word_after_a_slash_slash_in_a_string() {
    let url = "let url = \"https://example.test/tactic\";";
    assert_eq!(findings(url, "bounded"), vec![1]);
    let byte = "let b = b\"https://example.test/tactic\";";
    assert_eq!(findings(byte, "bounded"), vec![1]);
}

#[test]
fn the_scan_sees_a_name_after_escaped_quotes() {
    let text = "let s = \"say \\\"bounded\\\" now\";";
    assert_eq!(findings(text, "bounded"), vec![1]);
}

#[test]
fn the_scan_sees_a_name_after_a_char_holding_a_quote() {
    let text = "let c = '\"'; let s = \"bounded\";";
    assert_eq!(findings(text, "bounded"), vec![1]);
}

#[test]
fn the_scan_sees_a_name_after_an_escaped_quote_char() {
    let text = "let c = '\\''; let s = \"bounded\"; // tactic";
    assert_eq!(findings(text, "bounded"), vec![1]);
}

#[test]
fn the_scan_sees_a_name_in_a_raw_string_holding_a_slash_slash() {
    let text = "let a = r#\"https://example.test/bounded\"#;\nlet b = br#\"bounded // x\"#;";
    assert_eq!(findings(text, "bounded"), vec![1, 2]);
}

#[test]
fn the_scan_ignores_a_block_comment_holding_a_name() {
    let text = "/* \"bounded\" */\nlet x = 1;\n/* outer /* \"bounded\" */ tactic */";
    assert_eq!(findings(text, "bounded"), Vec::new());
}

#[test]
fn the_scan_sees_a_name_after_a_lifetime() {
    let text = "fn f<'a>(x: &'a str) -> &'a str { \"bounded\" }";
    assert_eq!(findings(text, "bounded"), vec![1]);
}

#[test]
fn the_scan_ignores_a_theory_word_in_a_comment() {
    let text = "// tactic\nlet x = 1; /* theorem */\n/* refinement */ // lean4";
    assert_eq!(findings(text, "bounded"), Vec::new());
    assert_eq!(findings("let tactic = 1;", "bounded"), vec![1]);
}
