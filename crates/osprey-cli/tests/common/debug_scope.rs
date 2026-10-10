//! Verified DWARF lexical ancestry shared by compiler conformance gates.

pub(crate) fn scope_parent(metadata: &str) -> Result<&str, String> {
    metadata
        .split_once("scope: ")
        .and_then(|(_, scope)| scope.split([',', ')']).next())
        .ok_or_else(|| format!("missing scope parent in {metadata}"))
}

/// Follow verified lexical parents, rejecting missing nodes and cycles.
pub(crate) fn scope_chain<'a>(ir: &'a str, scope: &'a str) -> Result<Vec<&'a str>, String> {
    let mut chain = vec![scope];
    let mut current = scope;
    loop {
        let metadata = ir
            .lines()
            .find(|line| line.starts_with(&format!("{current} = ")))
            .ok_or_else(|| format!("missing scope metadata {current}"))?;
        if metadata.contains("!DISubprogram(") {
            return Ok(chain);
        }
        assert!(
            metadata.contains("!DILexicalBlock("),
            "invalid scope: {metadata}"
        );
        let parent = scope_parent(metadata)?;
        assert!(!chain.contains(&parent), "cyclic scope: {parent}");
        chain.push(parent);
        current = parent;
    }
}
