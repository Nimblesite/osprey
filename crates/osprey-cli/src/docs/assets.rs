//! User-authored Markdown and stylesheets join the generated reference.
//! Implements [DOC-EXPORT-PAGES] and [DOC-EXPORT-CSS].

use super::model::{Page, Stylesheet};
use std::io;
use std::path::{Path, PathBuf};

pub(super) fn pages(inputs: &[PathBuf]) -> io::Result<Vec<Page>> {
    let mut pages = Vec::new();
    for input in inputs {
        let mut files = Vec::new();
        discover(input, &mut files)?;
        files.sort();
        let root = if input.is_dir() {
            input.as_path()
        } else {
            input.parent().unwrap_or_else(|| Path::new("."))
        };
        for file in files {
            pages.push(page(&file, root)?);
        }
    }
    Ok(pages)
}

fn discover(path: &Path, files: &mut Vec<PathBuf>) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::other(
            "documentation page symlinks are not supported",
        ));
    }
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path)? {
            discover(&entry?.path(), files)?;
        }
    } else if path.extension().is_some_and(|extension| extension == "md") {
        files.push(path.into());
    } else if !path.is_file() {
        return Err(io::Error::other(format!(
            "not a documentation file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn page(path: &Path, root: &Path) -> io::Result<Page> {
    let markdown = std::fs::read_to_string(path)?;
    let relative = path
        .strip_prefix(root)
        .map_err(io::Error::other)?
        .with_extension("");
    let slug = relative
        .components()
        .map(|component| super::safe_slug(&component.as_os_str().to_string_lossy()))
        .collect::<Vec<_>>()
        .join("/");
    let title = markdown
        .lines()
        .find_map(|line| line.strip_prefix("# "))
        .map_or_else(
            || {
                path.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
            str::to_string,
        );
    Ok(Page {
        slug: format!("guides/{slug}"),
        title,
        group: "Guides".into(),
        summary: String::new(),
        markdown,
    })
}

pub(super) fn stylesheets(paths: &[PathBuf]) -> io::Result<Vec<Stylesheet>> {
    paths
        .iter()
        .enumerate()
        .map(|(index, path)| {
            let css = std::fs::read_to_string(path)?;
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            Ok(Stylesheet {
                name: format!("custom-{index}-{}.css", super::safe_slug(&stem)),
                css,
            })
        })
        .collect()
}
