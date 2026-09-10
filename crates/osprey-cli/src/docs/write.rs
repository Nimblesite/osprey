//! Markdown pages share output validation and manifest cleanup with HTML.
//! Implements [DOC-EXPORT].

use super::model::Page;
use std::io;
use std::path::Path;

pub(super) fn markdown(directory: &Path, pages: &[Page]) -> io::Result<()> {
    let files: Vec<_> = pages
        .iter()
        .map(|page| (format!("{}.md", page.slug), render(page).into_bytes()))
        .collect();
    super::output::publish(directory, &files, ".osprey-markdown-pages.json")
}

fn render(page: &Page) -> String {
    if page.markdown.starts_with("---\n") {
        page.markdown.clone()
    } else {
        format!(
            "---\nlayout: page\ntitle: \"{}\"\ndescription: \"{}\"\ncategory: \"{}\"\n---\n\n{}",
            super::yaml(&page.title),
            super::yaml(&page.summary),
            super::yaml(&page.group),
            page.markdown
        )
    }
}
