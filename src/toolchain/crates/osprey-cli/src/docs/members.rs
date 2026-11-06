//! Member listings and the API index: what each module, namespace and effect
//! holds, with a summary or signature per member. Implements [DOC-EXPORT].

use super::model::Page;

const CONTAINERS: [&str; 3] = ["Module", "Namespace", "Effect"];
const TABLE_HEAD: &str = "| Name | Kind | Description |\n| --- | --- | --- |\n";

/// The `## Members` table a container page ends with, empty for other pages.
pub(super) fn table(owner: &Page, pages: &[Page]) -> String {
    let rows = rows(owner, pages);
    if rows.is_empty() {
        return String::new();
    }
    format!("\n## Members\n\n{TABLE_HEAD}{rows}\n")
}

/// The API landing page: every container with its summary and members, then
/// the pages no container owns, so a reader sees what each export does.
pub(super) fn index(pages: &[Page]) -> Option<Page> {
    let api: Vec<_> = pages
        .iter()
        .filter(|page| page.slug.starts_with("api/"))
        .cloned()
        .collect();
    if api.is_empty() {
        return None;
    }
    let sections: String = api
        .iter()
        .filter(|page| is_container(page))
        .map(|owner| section(owner, &api))
        .collect();
    let loose: String = api
        .iter()
        .filter(|page| !is_container(page) && !owned(page, &api))
        .map(|page| row(page, &page.title))
        .collect();
    let loose = if loose.is_empty() {
        String::new()
    } else {
        format!("## Other declarations\n\n{TABLE_HEAD}{loose}\n")
    };
    Some(Page {
        slug: "api/index".into(),
        title: "API Reference".into(),
        group: "Overview".into(),
        summary: "Modules and declarations in this project.".into(),
        signature: String::new(),
        markdown: format!("# API Reference\n\n{sections}{loose}"),
    })
}

fn section(owner: &Page, pages: &[Page]) -> String {
    let summary = if owner.summary.is_empty() {
        String::new()
    } else {
        format!("{}\n\n", owner.summary)
    };
    let rows = rows(owner, pages);
    let members = if rows.is_empty() {
        String::new()
    } else {
        format!("{TABLE_HEAD}{rows}\n")
    };
    format!(
        "## [{}]({}.md)\n\n{summary}{members}",
        owner.title,
        link(owner)
    )
}

fn is_container(page: &Page) -> bool {
    CONTAINERS.contains(&page.group.as_str())
}

fn owned(page: &Page, pages: &[Page]) -> bool {
    pages
        .iter()
        .filter(|owner| is_container(owner))
        .any(|owner| member_name(owner, page).is_some())
}

fn member_name<'a>(owner: &Page, page: &'a Page) -> Option<&'a str> {
    let name = page.title.strip_prefix(&owner.title)?.strip_prefix("::")?;
    (!name.contains("::")).then_some(name)
}

fn rows(owner: &Page, pages: &[Page]) -> String {
    if !is_container(owner) {
        return String::new();
    }
    pages
        .iter()
        .filter_map(|page| member_name(owner, page).map(|name| row(page, name)))
        .collect()
}

fn row(page: &Page, name: &str) -> String {
    format!(
        "| [{name}]({}.md) | {} | {} |\n",
        link(page),
        page.group,
        cell(page, name)
    )
}

fn link(page: &Page) -> &str {
    page.slug.strip_prefix("api/").unwrap_or(&page.slug)
}

/// What a member listing says about one member: its summary where the author
/// wrote one, and otherwise its signature. A column of blank cells tells a
/// reader nothing, while the signature is the fact they came for.
fn cell(page: &Page, name: &str) -> String {
    let description = if page.summary.is_empty() {
        format!("`{}`", unqualified(&page.signature, name))
    } else {
        page.summary.clone()
    };
    description.replace('|', "\\|").replace('\n', " ")
}

/// A member's signature with the qualification taken off its head.
///
/// The Name column already carries it, and a table repeating it in every row
/// pushes the part a reader came for off the edge of a phone screen. The
/// qualification a signature carries is the compiler's finalized one — an
/// assembled project prefixes a namespace nobody wrote — so the head is found
/// by the member's own name rather than by the name of the page. An effect
/// operation is qualified with dots rather than `::`, so both are tried.
fn unqualified(signature: &str, name: &str) -> String {
    ["::", "."]
        .into_iter()
        .find_map(|separator| strip_head(signature, &format!("{separator}{name}"), name))
        .unwrap_or_else(|| signature.to_owned())
}

/// `signature` with the qualified name ending in `tail` reduced to `name`, or
/// `None` where it does not carry one.
fn strip_head(signature: &str, tail: &str, name: &str) -> Option<String> {
    let at = signature.find(tail)?;
    let head = signature.get(..at)?;
    let start = head
        .char_indices()
        .rev()
        .find(|(_, character)| !qualifier(*character))
        .map_or(0, |(at, character)| at.saturating_add(character.len_utf8()));
    Some(format!(
        "{}{name}{}",
        head.get(..start)?,
        signature.get(at.saturating_add(tail.len())..)?
    ))
}

/// A character that can appear inside a qualified name, and so belongs to the
/// head being removed rather than to the keyword or spacing in front of it.
fn qualifier(character: char) -> bool {
    character.is_alphanumeric() || matches!(character, '_' | ':' | '.')
}
