//! CI source-style gates for the complete Default and ML language-test corpus.
//!
//! Both flavors synthesize `main` from bare top-level statements and lower them
//! to byte-identical IR ([FLAVOR-IR-EQUIV], docs/specs/0023, 0024), so a
//! zero-argument `fn main()` (Default) or `main ()` / `main :` (ML) is pure
//! boilerplate: the program reads exactly the same written as bare top-level
//! statements. This gate fails if any language test carries that boilerplate,
//! so the rule is enforced forever instead of by review.
//!
//! The *only* sanctioned exception is a program that genuinely needs `argv` or a
//! non-zero exit code. A `main` that takes parameters is never flagged (it is
//! consuming `argv`); a zero-argument `main` kept for its exit code must opt out
//! explicitly with a `// osprey: keep-main <reason>` marker, which both
//! documents the intent and silences the gate. Declaration-leading prose must
//! likewise use the flavor's documentation sigil so editor hover can surface it
//! ([DOC-SIGIL-DEFAULT], [DOC-SIGIL-ML], [DOC-ATTACH]).

use osprey_ast::{walk_program, AstVisitor, Position, Stmt};
use osprey_syntax::Flavor;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize().unwrap_or(root)
}

/// `tests`, resolved from the crate manifest so the gate runs the same on a
/// development machine and in CI.
fn tested_dir() -> PathBuf {
    repository_root().join("tests")
}

/// The opt-out marker: a zero-argument `main` kept on purpose (a meaningful
/// non-zero exit code) carries this so the gate records the intent and passes.
const KEEP_MARKER: &str = "osprey: keep-main";

/// Every `.osp`/`.ospml` under `dir`, found by a recursive walk and sorted for
/// deterministic reporting.
fn example_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, &mut out);
    out.sort();
    out
}

fn documentation_files(root: &Path) -> Vec<PathBuf> {
    let mut files = [
        root.join("tests"),
        root.join("examples/projects/modules/test"),
        root.join("vscode-extension/test"),
    ]
    .iter()
    .flat_map(|directory| example_files(directory))
    .collect::<Vec<_>>();
    files.sort();
    files
}

/// Recurse into `dir`, pushing every Osprey source file into `out`.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x == "osp" || x == "ospml")
        {
            out.push(path);
        }
    }
}

/// The text between the first `(` and its matching `)` on a `main` header line,
/// or `None` when the line is not a `main` declaration. Used to tell a
/// zero-argument `main ()` (boilerplate) from `main (argv)` (consuming argv).
fn main_param_text(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    // Default `fn main(...)` or ML `main (...)` — the binding head, not a call.
    let after = rest
        .strip_prefix("fn main")
        .or_else(|| rest.strip_prefix("main"))?
        .trim_start();
    let inner = after.strip_prefix('(')?;
    inner.split_once(')').map(|(params, _)| params)
}

/// True when `line` declares a needless zero-argument `main`: `fn main()`,
/// `main ()`, or the ML signature `main :` (which only ever types such a main).
fn declares_needless_main(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("main :") || trimmed.starts_with("main:") {
        return true;
    }
    main_param_text(line).is_some_and(|params| params.trim().is_empty())
}

#[derive(Debug)]
struct Declaration {
    name: String,
    line: usize,
    function: bool,
}

#[derive(Default)]
struct DeclarationCollector(Vec<Declaration>);

impl AstVisitor for DeclarationCollector {
    fn statement(&mut self, statement: &Stmt) {
        let Some((name, position, undocumented, function)) = declaration(statement) else {
            return;
        };
        if let (true, Some(position)) = (undocumented, position) {
            self.push(name, position, function);
        }
    }
}

impl DeclarationCollector {
    fn push(&mut self, name: &str, position: Position, function: bool) {
        if let Some(line) = usize::try_from(position.line)
            .ok()
            .and_then(|line| line.checked_sub(1))
        {
            self.0.push(Declaration {
                name: name.to_owned(),
                line,
                function,
            });
        }
    }
}

fn declaration(statement: &Stmt) -> Option<(&str, Option<Position>, bool, bool)> {
    match statement {
        Stmt::Function {
            name,
            position,
            doc,
            ..
        } => Some((name, *position, doc.is_none(), true)),
        Stmt::Let {
            name,
            position,
            doc,
            ..
        }
        | Stmt::Extern {
            name,
            position,
            doc,
            ..
        }
        | Stmt::Type {
            name,
            position,
            doc,
            ..
        }
        | Stmt::Effect {
            name,
            position,
            doc,
            ..
        }
        | Stmt::Signature {
            name,
            position,
            doc,
            ..
        } => Some((name, *position, doc.is_none(), false)),
        Stmt::Module {
            path,
            position,
            doc,
            ..
        } => Some((
            path.last().unwrap_or("module"),
            *position,
            doc.is_none(),
            false,
        )),
        _ => None,
    }
}

fn signature_line(lines: &[&str], declaration: &Declaration, flavor: Flavor) -> usize {
    let previous = declaration.line.checked_sub(1);
    if flavor == Flavor::Ml
        && declaration.function
        && previous.is_some_and(|line| is_signature(lines.get(line).copied(), &declaration.name))
    {
        previous.unwrap_or(declaration.line)
    } else {
        declaration.line
    }
}

fn is_signature(line: Option<&str>, name: &str) -> bool {
    line.and_then(|line| line.trim_start().strip_prefix(name))
        .is_some_and(|rest| rest.trim_start().starts_with(':'))
}

fn leading_width(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count()
}

fn ordinary_doc_candidate(line: &str, flavor: Flavor) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//")
        && (flavor == Flavor::Ml || (!trimmed.starts_with("///") && !trimmed.starts_with("//!")))
}

fn offender_line(lines: &[&str], declaration: &Declaration, flavor: Flavor) -> Option<usize> {
    let lead = signature_line(lines, declaration, flavor);
    let previous = lead.checked_sub(1)?;
    let comment = *lines.get(previous)?;
    let declaration_line = *lines.get(lead)?;
    (leading_width(comment) == leading_width(declaration_line)
        && ordinary_doc_candidate(comment, flavor))
    .then_some(previous)
}

fn documentation_offenders(path: &Path, source: &str) -> Vec<(usize, String)> {
    let parsed = osprey_syntax::parse_program_for_path(&path.to_string_lossy(), source);
    let mut declarations = DeclarationCollector::default();
    walk_program(&parsed.program, &mut declarations);
    let lines: Vec<_> = source.lines().collect();
    declarations
        .0
        .into_iter()
        .filter_map(|declaration| {
            offender_line(&lines, &declaration, parsed.flavor)
                .map(|line| (line + 1, declaration.name))
        })
        .collect()
}

#[test]
fn declaration_comments_use_flavor_documentation_syntax() {
    let root = repository_root();
    let mut offenders = Vec::new();
    for path in documentation_files(&root) {
        let source = std::fs::read_to_string(&path).expect("read example source");
        let relative = path.strip_prefix(&root).unwrap_or(&path);
        offenders.extend(
            documentation_offenders(&path, &source)
                .into_iter()
                .map(|(line, name)| format!("{}:{line}: {name}", relative.display())),
        );
    }
    let preview = offenders.iter().take(80).cloned().collect::<Vec<_>>();
    assert!(
        offenders.is_empty(),
        "{} declarations use ordinary comments where hover documentation requires `///` in Default or `(** ... *)` in ML:\n  {}",
        offenders.len(),
        preview.join("\n  ")
    );
}

#[test]
fn no_example_wraps_a_trivial_program_in_main() {
    let dir = tested_dir();
    let mut offenders: Vec<String> = Vec::new();

    for path in example_files(&dir) {
        let src = std::fs::read_to_string(&path).expect("read example source");
        if src.contains(KEEP_MARKER) {
            continue; // explicitly sanctioned (argv / non-zero exit code)
        }
        if src.lines().any(declares_needless_main) {
            let rel = path
                .strip_prefix(&dir)
                .unwrap_or(&path)
                .display()
                .to_string();
            offenders.push(rel);
        }
    }

    assert!(
        offenders.is_empty(),
        "{} example(s) wrap a trivial program in a needless `main` — write bare \
         top-level statements instead (both flavors synthesize `main` with \
         identical IR). If a zero-arg `main` is kept for argv/exit-code, mark it \
         `// {KEEP_MARKER} <reason>`:\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}

#[test]
fn test_corpus_does_not_call_public_http_services() {
    let dir = tested_dir();
    let offenders = example_files(&dir)
        .into_iter()
        .filter(|path| {
            std::fs::read_to_string(path).is_ok_and(|source| source.contains("httpbin.org"))
        })
        .map(|path| {
            path.strip_prefix(&dir)
                .unwrap_or(&path)
                .display()
                .to_string()
        })
        .collect::<Vec<_>>();
    assert!(
        offenders.is_empty(),
        "test corpus depends on the public network:\n  {}",
        offenders.join("\n  ")
    );
}

#[test]
fn detector_classifies_main_headers_correctly() {
    // Needless: zero-argument mains in both flavors and the ML signature line.
    assert!(declares_needless_main("fn main() = {"));
    assert!(declares_needless_main("fn main () ="));
    assert!(declares_needless_main("main () ="));
    assert!(declares_needless_main("main :"));
    assert!(declares_needless_main("main : Unit -> int"));
    // Allowed: a `main` that consumes argv is never boilerplate.
    assert!(!declares_needless_main("fn main(args) ="));
    assert!(!declares_needless_main("main argv ="));
    // Unrelated lines, and calls to other functions, are never flagged.
    assert!(!declares_needless_main("let mainResult = run()"));
    assert!(!declares_needless_main("print(\"main done\")"));
    assert!(!declares_needless_main("fn mainLoop() ="));
}
