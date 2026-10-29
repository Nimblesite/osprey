//! Bounded directory listings and UTF-8 file reads ([GPUI-HOST-CAPABILITIES]).

use std::fs::{self, Metadata};
use std::io::Read;

use serde_json::{json, Value};

use super::{field, number, strings};
use crate::envelope::Request;
use crate::ffi;

/// Writes immediate-child directory facts to the requested slice.
pub fn list_directories(request: &Request) -> Result<Value, String> {
    publish(request, "paths", listing)
}

/// Answers every requested path within the request's limit and writes the answers to the
/// requested slice under `key`.
fn publish(request: &Request, key: &str, answer: fn(&str, u64) -> Value) -> Result<Value, String> {
    let limit = number(request, "limit")?;
    let answers = strings(request, "paths")?
        .into_iter()
        .map(|path| answer(path, limit))
        .collect::<Vec<_>>();
    ffi::put_slice(
        field(request, "slice")?,
        &json!({ key: answers }).to_string(),
    )?;
    Ok(Value::Null)
}

/// Reads one directory without descending and records absence separately from errors.
fn listing(path: &str, limit: u64) -> Value {
    match read_listing(path, limit) {
        Ok((entries, truncated)) => {
            json!({"path": path, "error": "", "missing": false, "truncated": truncated, "entries": entries})
        }
        Err(problem) => {
            let missing = problem.kind() == std::io::ErrorKind::NotFound;
            let error = if missing {
                String::new()
            } else {
                problem.to_string()
            };
            json!({"path": path, "error": error, "missing": missing, "truncated": false, "entries": []})
        }
    }
}

/// Reads at most `limit` children plus one lookahead entry.
fn read_listing(path: &str, limit: u64) -> std::io::Result<(Vec<Value>, bool)> {
    let mut entries = Vec::new();
    for child in fs::read_dir(path)? {
        let child = child?;
        if entries.len() as u64 == limit {
            return Ok((entries, true));
        }
        entries.push(entry(&child));
    }
    Ok((entries, false))
}

/// Converts one directory entry to the framework's mechanical JSON facts.
fn entry(child: &fs::DirEntry) -> Value {
    let path = child.path();
    let name = child.file_name().to_string_lossy().into_owned();
    let metadata = fs::symlink_metadata(&path).ok();
    let directory = fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir());
    let link = metadata
        .as_ref()
        .is_some_and(|metadata| metadata.file_type().is_symlink());
    let hidden = name.starts_with('.') || metadata.as_ref().is_some_and(platform_hidden);
    json!({"name": name, "path": path.to_string_lossy(), "directory": directory, "link": link, "hidden": hidden})
}

/// Reports the Windows hidden attribute; Unix hidden names are handled by their dot prefix.
#[cfg(windows)]
fn platform_hidden(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x2 != 0
}

/// Reports no additional hidden attribute on non-Windows platforms.
#[cfg(not(windows))]
fn platform_hidden(_: &Metadata) -> bool {
    false
}

/// Writes bounded UTF-8 file contents to the requested slice.
pub fn read_texts(request: &Request) -> Result<Value, String> {
    publish(request, "files", text_result)
}

/// Converts one text read to a path, error and text triple.
fn text_result(path: &str, limit: u64) -> Value {
    match read_text(path, limit) {
        Ok(text) => json!({"path": path, "error": "", "text": text}),
        Err(problem) => json!({"path": path, "error": problem, "text": ""}),
    }
}

/// Reads one regular file of at most `limit` bytes as strict UTF-8, refusing
/// symlinks exactly as the Swift host's `O_NOFOLLOW` open does.
pub(super) fn read_text(path: &str, limit: u64) -> Result<String, String> {
    if !fs::symlink_metadata(path)
        .map_err(|problem| problem.to_string())?
        .is_file()
    {
        return Err(format!("{path} is not a regular file"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|problem| problem.to_string())?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|problem| problem.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(too_large(bytes.len() as u64, limit));
    }
    String::from_utf8(bytes).map_err(|_| "Not UTF-8 text".to_owned())
}

/// Formats the stable bounded-read failure.
fn too_large(size: u64, limit: u64) -> String {
    format!("{size} bytes exceeds the {limit} byte limit")
}

#[cfg(test)]
mod tests {
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde_json::{json, Value};

    use super::read_text;
    use crate::adapters::answer;
    use crate::ffi;

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        /// Creates an isolated temporary directory.
        fn new() -> io::Result<Self> {
            let path = std::env::temp_dir().join(format!(
                "osprey-gpui-files-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path)?;
            Ok(Self(path))
        }

        /// Joins a test-relative path.
        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        /// Returns the directory's UTF-8 test path.
        fn text(&self, name: &str) -> String {
            self.join(name).to_string_lossy().into_owned()
        }

        /// Returns the directory root's UTF-8 test path.
        fn root(&self) -> String {
            self.0.to_string_lossy().into_owned()
        }
    }

    impl Drop for TestDir {
        /// Removes all temporary test artifacts.
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Builds a file capability request with a test-specific slice key.
    fn request(kind: &str, paths: &[&str], limit: u64, slice: &str) -> crate::envelope::Request {
        crate::adapters::request(
            kind,
            &[
                ("paths", json!(paths)),
                ("limit", json!(limit)),
                ("slice", json!(slice)),
            ],
        )
    }

    /// Decodes a published test slice.
    fn slice(key: &str) -> io::Result<Value> {
        ffi::get_slice(key)
            .and_then(|text| serde_json::from_str(&text).ok())
            .ok_or_else(|| io::Error::other("missing or invalid test slice"))
    }

    /// Returns one listing's entries, refusing a malformed test slice.
    fn entries(listing: &Value) -> io::Result<&Vec<Value>> {
        listing["entries"]
            .as_array()
            .ok_or_else(|| io::Error::other("entries are not an array"))
    }

    /// Lists `path` through the capability into the slice `key` and returns its listing.
    fn list(path: &str, limit: u64, key: &str) -> io::Result<Value> {
        assert_eq!(
            answer(&request("list-directories", &[path], limit, key)),
            Ok(Value::Null)
        );
        Ok(slice(key)?["paths"][0].clone())
    }

    /// Returns the listed entry called `name`, refusing a listing without it.
    fn named<'a>(listing: &'a Value, name: &str) -> io::Result<&'a Value> {
        entries(listing)?
            .iter()
            .find(|entry| entry["name"] == json!(name))
            .ok_or_else(|| io::Error::other(format!("{name} entry is missing")))
    }

    /// Reads `paths` through the capability into the slice `key` and returns each file's answer.
    fn read(paths: &[&str], limit: u64, key: &str) -> io::Result<Vec<Value>> {
        assert_eq!(
            answer(&request("read-texts", paths, limit, key)),
            Ok(Value::Null)
        );
        slice(key)?["files"]
            .as_array()
            .cloned()
            .ok_or_else(|| io::Error::other("files are not an array"))
    }

    #[test]
    /// Lists immediate entries with flags and enforces the entry limit.
    fn list_directories_reports_entries_and_truncation() -> io::Result<()> {
        let dir = TestDir::new()?;
        std::fs::write(dir.join("plain"), "text")?;
        std::fs::create_dir(dir.join("folder"))?;
        std::fs::write(dir.join(".hidden"), "")?;
        let listed = list(&dir.root(), 10, "list-full-test")?;
        let names: Vec<&str> = entries(&listed)?
            .iter()
            .map(|entry| entry["name"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(names.len(), 3);
        assert!(
            names.contains(&"plain") && names.contains(&"folder") && names.contains(&".hidden")
        );
        assert_eq!(named(&listed, ".hidden")?["hidden"], json!(true));
        assert_eq!(named(&listed, "folder")?["directory"], json!(true));
        assert_eq!(listed["missing"], json!(false));
        assert_eq!(listed["truncated"], json!(false));
        let limited = list(&dir.root(), 1, "list-limited-test")?;
        assert_eq!(limited["truncated"], json!(true));
        assert_eq!(entries(&limited)?.len(), 1);
        Ok(())
    }

    #[test]
    /// Distinguishes a missing directory from an inaccessible one.
    fn list_directories_reports_missing_paths() -> io::Result<()> {
        let dir = TestDir::new()?;
        let listed = list(&dir.text("absent"), 1, "list-missing-test")?;
        assert_eq!(listed["missing"], json!(true));
        assert_eq!(listed["error"], json!(""));
        assert_eq!(entries(&listed)?.len(), 0);
        Ok(())
    }

    #[test]
    /// Reads text and reports missing, invalid UTF-8 and oversized files per path.
    fn read_texts_reports_contents_and_per_file_errors() -> io::Result<()> {
        let dir = TestDir::new()?;
        std::fs::write(dir.join("good"), "hello")?;
        std::fs::write(dir.join("binary"), [0xff])?;
        let paths = [dir.text("good"), dir.text("binary"), dir.text("missing")];
        let files = read(&[&paths[0], &paths[1], &paths[2]], 100, "read-texts-test")?;
        assert_eq!(files[0]["text"], json!("hello"));
        assert_eq!(files[0]["error"], json!(""));
        assert_eq!(files[1]["error"], json!("Not UTF-8 text"));
        assert_eq!(files[1]["text"], json!(""));
        assert!(files[2]["error"]
            .as_str()
            .is_some_and(|error| !error.is_empty()));
        assert_eq!(files[2]["text"], json!(""));
        Ok(())
    }

    #[test]
    /// Reports a file over the byte limit as an error with empty text, never a truncation.
    fn read_texts_refuse_oversized_files() -> io::Result<()> {
        let dir = TestDir::new()?;
        std::fs::write(dir.join("large"), "hello")?;
        let large = dir.text("large");
        let file = read(&[&large], 4, "read-limit-test")?[0].clone();
        assert_eq!(file["text"], json!(""));
        assert_eq!(
            file["error"].as_str(),
            Some("5 bytes exceeds the 4 byte limit")
        );
        assert_eq!(read_text(&large, 5).as_deref(), Ok("hello"));
        Ok(())
    }

    #[test]
    /// Refuses directories and symlinks as text reads.
    fn read_text_refuses_non_regular_files() -> io::Result<()> {
        let dir = TestDir::new()?;
        std::fs::write(dir.join("plain"), "text")?;
        assert_eq!(
            read_text(&dir.root(), 10),
            Err(format!("{} is not a regular file", dir.root()))
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(Path::new("plain"), dir.join("link"))?;
            assert_eq!(
                read_text(&dir.text("link"), 10),
                Err(format!("{} is not a regular file", dir.text("link")))
            );
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    /// Marks symbolic links separately from their target's directory flag.
    fn list_directories_marks_links() -> io::Result<()> {
        let dir = TestDir::new()?;
        std::fs::write(dir.join("plain"), "text")?;
        std::os::unix::fs::symlink(Path::new("plain"), dir.join("link"))?;
        let listed = list(&dir.root(), 10, "list-links-test")?;
        let link = named(&listed, "link")?;
        assert_eq!(link["link"], json!(true));
        assert_eq!(link["directory"], json!(false));
        Ok(())
    }
}
