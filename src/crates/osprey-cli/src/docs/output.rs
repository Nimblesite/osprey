//! Validate the entire output set before publishing; clean only manifest-owned files.
//! Implements [DOC-EXPORT], [DOC-EXPORT-HTML].

use std::collections::BTreeSet;
use std::io;
use std::path::{Component, Path};

pub(super) fn publish(
    directory: &Path,
    files: &[(String, Vec<u8>)],
    manifest: &str,
) -> io::Result<()> {
    let current = filenames(files)?;
    check_destination(directory, manifest)?;
    let previous = previous(directory, manifest)?;
    for file in current.union(&previous) {
        check_destination(directory, file)?;
    }
    for (file, bytes) in files {
        write(directory, file, bytes)?;
    }
    for obsolete in previous.difference(&current) {
        match std::fs::remove_file(directory.join(obsolete)) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    write(
        directory,
        manifest,
        &serde_json::to_vec_pretty(&current).map_err(io::Error::other)?,
    )
}

fn filenames(files: &[(String, Vec<u8>)]) -> io::Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    let mut folded = BTreeSet::new();
    for (file, _) in files {
        validate(file)?;
        if !folded.insert(file.to_lowercase()) {
            return Err(io::Error::other(format!(
                "duplicate documentation path: {file}"
            )));
        }
        let _ = names.insert(file.clone());
    }
    Ok(names)
}

fn previous(directory: &Path, manifest: &str) -> io::Result<BTreeSet<String>> {
    match std::fs::read(directory.join(manifest)) {
        Ok(bytes) => {
            let paths: BTreeSet<String> =
                serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            for path in &paths {
                validate(path)?;
            }
            Ok(paths)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(error) => Err(error),
    }
}

fn validate(path: &str) -> io::Result<()> {
    let relative = !path.is_empty()
        && Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
    let owned = path == "index.html"
        || path
            .split('/')
            .next()
            .is_some_and(|root| matches!(root, "functions" | "api" | "guides" | "assets"));
    if relative && owned && !path.contains('\\') {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "invalid generated documentation path: {path}"
    )))
}

fn check_destination(directory: &Path, file: &str) -> io::Result<()> {
    check_component(directory, true)?;
    let mut path = directory.to_path_buf();
    let components: Vec<_> = Path::new(file).components().collect();
    for (index, component) in components.iter().enumerate() {
        path.push(component.as_os_str());
        check_component(&path, index + 1 < components.len())?;
    }
    Ok(())
}

fn check_component(path: &Path, directory: bool) -> io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(io::Error::other(format!(
            "documentation output contains a symlink: {}",
            path.display()
        ))),
        Ok(metadata) if metadata.is_dir() != directory => Err(io::Error::other(format!(
            "documentation output has incompatible path: {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn write(directory: &Path, file: &str, bytes: &[u8]) -> io::Result<()> {
    let path = directory.join(file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::publish;

    fn directory(tag: &str) -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("osprey_publish_{}_{}", std::process::id(), tag));
        std::fs::create_dir_all(&path).expect("output fixture");
        path
    }

    #[test]
    fn invalid_previous_manifests_leave_existing_pages_untouched() {
        for (index, manifest) in [
            "invalid json",
            "[\"../outside\"]",
            "[\"/absolute\"]",
            "[\"other/file\"]",
            "[\"api\\\\escape\"]",
        ]
        .iter()
        .enumerate()
        {
            let path = directory(&format!("manifest_{index}"));
            std::fs::write(path.join(".manifest.json"), manifest).expect("manifest");
            std::fs::write(path.join("index.html"), "existing").expect("existing page");
            assert!(publish(
                &path,
                &[("index.html".into(), b"replacement".to_vec())],
                ".manifest.json"
            )
            .is_err());
            assert_eq!(
                std::fs::read_to_string(path.join("index.html")).expect("retained"),
                "existing"
            );
        }
    }

    #[test]
    fn incompatible_destination_types_fail_before_any_write() {
        for (index, manifest_directory) in [true, false].iter().enumerate() {
            let path = directory(&format!("types_{index}"));
            let blocker = if *manifest_directory {
                ".manifest.json"
            } else {
                "api/page.html"
            };
            std::fs::create_dir_all(path.join(blocker)).expect("blocked destination");
            let files = [
                ("index.html".into(), b"new".to_vec()),
                ("api/page.html".into(), b"page".to_vec()),
            ];
            assert!(publish(&path, &files, ".manifest.json").is_err());
            assert!(!path.join("index.html").exists());
        }
    }
}
