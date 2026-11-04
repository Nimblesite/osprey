//! Flavor-neutral documentation pages consumed by Markdown and HTML writers.
//! Implements [DOC-EXPORT].

#[derive(Clone)]
pub(super) struct Page {
    /// Relative output path without its extension, e.g. `api/math-add`.
    pub(super) slug: String,
    pub(super) title: String,
    pub(super) group: String,
    pub(super) summary: String,
    /// The declaration's signature line, empty on a page that documents no
    /// single declaration. A member listing falls back to it where the member
    /// carries no summary of its own.
    pub(super) signature: String,
    /// Markdown body, potentially including existing website front matter.
    pub(super) markdown: String,
}

pub(super) struct Stylesheet {
    pub(super) name: String,
    pub(super) css: String,
}
