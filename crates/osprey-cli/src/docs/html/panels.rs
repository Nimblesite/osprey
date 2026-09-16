//! The signature block as a labelled disclosure. Implements [DOC-EXPORT-HTML].

/// The opening and closing of the block a code fence renders as.
const OPEN: &str = "<pre>";
const CLOSE: &str = "</pre>";

/// Wrap a declaration page's signature block in a disclosure the reader can
/// fold away.
///
/// The signature is the one code block this exporter writes itself, above
/// everything an author contributed. A page whose whole body is a single
/// unlabelled block of code never says what the reader is looking at; a named
/// panel does, and folds the code away once it has been read. It opens
/// expanded: a reference that answers no question on arrival is not one.
pub(super) fn signature(slug: &str, body: &str) -> String {
    if !slug.starts_with("api/") {
        return body.to_owned();
    }
    let Some((before, rest)) = body.split_once(OPEN) else {
        return body.to_owned();
    };
    let Some((code, after)) = rest.split_once(CLOSE) else {
        return body.to_owned();
    };
    // A fence below the first section heading is an author's example, not the
    // signature line, and an example is not a signature however it is labelled.
    if before.contains("<h2") {
        return body.to_owned();
    }
    format!("{before}{}{after}", panel(code))
}

fn panel(code: &str) -> String {
    format!(
        "<details class=\"signature-panel\" open>\
<summary>Signature</summary>{OPEN}{code}{CLOSE}</details>"
    )
}
