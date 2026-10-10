//! Source candidates for compiler-proved module repairs.
//! Implements [LSP-CODE-ACTIONS-MODULES].

use super::Candidate;
use osprey_ast::{ImportSelection, NamespaceName, Stmt};
use osprey_project::SourceFile;

pub(super) fn candidates(file: &SourceFile) -> Vec<Candidate> {
    let mut edits = paths(&file.source);
    for import in imports(&file.program.statements) {
        if import.alias.is_none()
            && import.selection == ImportSelection::Whole
            && import.target.path.segments.is_empty()
            && matches!(import.target.namespace, NamespaceName::Quoted(_))
        {
            edits.extend(alias_candidate(file, import));
        }
    }
    edits
}

fn imports(statements: &[Stmt]) -> Vec<&osprey_ast::ImportDecl> {
    statements
        .iter()
        .flat_map(|statement| match statement {
            Stmt::Import(import) => vec![import],
            Stmt::Namespace { body, .. } => imports(body),
            _ => Vec::new(),
        })
        .collect()
}

fn alias_candidate(file: &SourceFile, import: &osprey_ast::ImportDecl) -> Option<Candidate> {
    let start = import_offset(&file.source, import.position?)?;
    let suffix = file.source.get(start..)?.strip_prefix("import")?;
    let target = suffix.trim_start();
    let end = start + "import".len() + suffix.len() - target.len() + quoted_length(target)?;
    Some(Candidate {
        title: "Add explicit import alias",
        bytes: end..end,
        selection: start..end,
        text: format!(" as {}", alias_name(import)),
    })
}

fn import_offset(source: &str, position: osprey_ast::Position) -> Option<usize> {
    let line = usize::try_from(position.line.checked_sub(1)?).ok()?;
    let column = usize::try_from(position.column).ok()?;
    let text = source.lines().nth(line)?;
    let _ = text.get(..column)?;
    Some(
        source
            .split_inclusive('\n')
            .take(line)
            .map(str::len)
            .sum::<usize>()
            + column,
    )
}

fn quoted_length(target: &str) -> Option<usize> {
    let mut escaped = false;
    for (index, character) in target.char_indices().skip(1) {
        if character == '"' && !escaped {
            return Some(index + 1);
        }
        escaped = character == '\\' && !escaped;
    }
    None
}

fn alias_name(import: &osprey_ast::ImportDecl) -> String {
    let label = import.target.namespace.label();
    let mut alias: String = label
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(capitalize)
        .collect();
    if alias.is_empty() || alias.starts_with(|c: char| c.is_ascii_digit()) {
        alias.insert_str(0, "Imported");
    }
    alias
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|c| c.to_ascii_uppercase())
        .into_iter()
        .chain(chars)
        .collect()
}

fn paths(source: &str) -> Vec<Candidate> {
    source
        .split_inclusive(|c| !path_character(c))
        .scan(0, |offset, text| {
            let start = *offset;
            *offset += text.len();
            Some((start, text.trim_end_matches(|c| !path_character(c))))
        })
        .filter_map(|(start, path)| path_candidate(start, path))
        .collect()
}

fn path_character(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | ':' | '.')
}

fn path_candidate(start: usize, path: &str) -> Option<Candidate> {
    if !path.contains('.') || !path.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let bytes = start..start + path.len();
    Some(Candidate {
        title: "Use explicit module path",
        selection: bytes.clone(),
        bytes,
        text: path.replace('.', "::"),
    })
}
