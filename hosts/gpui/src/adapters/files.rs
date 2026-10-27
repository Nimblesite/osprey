//! Bounded directory listings and UTF-8 file reads ([GPUI-HOST-CAPABILITIES]).

use std::fs::{self, Metadata};
use std::io::Read;

use serde_json::{json, Value};

use super::{field, number, strings};
use crate::envelope::Request;
use crate::ffi;

/// Writes immediate-child directory facts to the requested slice.
pub fn list_directories(request: &Request) -> Result<Value, String> {
    let limit = number(request, "limit")?;
    let paths = strings(request, "paths")?
        .into_iter()
        .map(|path| listing(path, limit))
        .collect::<Vec<_>>();
    ffi::put_slice(
        field(request, "slice")?,
        &json!({"paths": paths}).to_string(),
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
    let limit = number(request, "limit")?;
    let files = strings(request, "paths")?
        .into_iter()
        .map(|path| text_result(path, limit))
        .collect::<Vec<_>>();
    ffi::put_slice(
        field(request, "slice")?,
        &json!({"files": files}).to_string(),
    )?;
    Ok(Value::Null)
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

    #[test]
    /// Lists immediate entries with flags and enforces the entry limit.
    fn list_directories_reports_entries_and_truncation() -> io::Result<()> {
        let dir = TestDir::new()?;
        std::fs::write(dir.join("plain"), "text")?;
        std::fs::create_dir(dir.join("folder"))?;
        std::fs::write(dir.join(".hidden"), "")?;
        let path = dir.root();
        assert_eq!(
            answer(&request("list-directories", &[&path], 10, "list-full-test")),
            Ok(Value::Null)
        );
        let listed = slice("list-full-test")?["paths"][0].clone();
        let names: Vec<&str> = entries(&listed)?
            .iter()
            .map(|entry| entry["name"].as_str().unwrap_or_default())
            .collect();
        assert_eq!(names.len(), 3);
        assert!(
            names.contains(&"plain") && names.contains(&"folder") && names.contains(&".hidden")
        );
        let hidden = entries(&listed)?
            .iter()
            .find(|entry| entry["name"] == json!(".hidden"))
            .ok_or_else(|| io::Error::other("hidden entry is missing"))?;
        assert_eq!(hidden["hidden"], json!(true));
        let folder = entries(&listed)?
            .iter()
            .find(|entry| entry["name"] == json!("folder"))
            .ok_or_else(|| io::Error::other("folder entry is missing"))?;
        assert_eq!(folder["directory"], json!(true));
        assert_eq!(listed["missing"], json!(false));
        assert_eq!(listed["truncated"], json!(false));
        assert_eq!(
            answer(&request(
                "list-directories",
                &[&path],
                1,
                "list-limited-test"
            )),
            Ok(Value::Null)
        );
        let limited = slice("list-limited-test")?["paths"][0].clone();
        assert_eq!(limited["truncated"], json!(true));
        assert_eq!(limited["entries"].as_array().map(Vec::len), Some(1));
        Ok(())
    }

    #[test]
    /// Distinguishes a missing directory from an inaccessible one.
    fn list_directories_reports_missing_paths() -> io::Result<()> {
        let dir = TestDir::new()?;
        let missing = dir.text("absent");
        assert_eq!(
            answer(&request(
                "list-directories",
                &[&missing],
                1,
                "list-missing-test"
            )),
            Ok(Value::Null)
        );
        let listed = slice("list-missing-test")?["paths"][0].clone();
        assert_eq!(listed["missing"], json!(true));
        assert_eq!(listed["error"], json!(""));
        assert_eq!(listed["entries"].as_array().map(Vec::len), Some(0));
        Ok(())
    }

    #[test]
    /// Reads text and reports missing, invalid UTF-8 and oversized files per path.
    fn read_texts_reports_contents_and_per_file_errors() -> io::Result<()> {
        let dir = TestDir::new()?;
        std::fs::write(dir.join("good"), "hello")?;
        std::fs::write(dir.join("binary"), [0xff])?;
        let good = dir.text("good");
        let binary = dir.text("binary");
        let missing = dir.text("missing");
        let request = request(
            "read-texts",
            &[&good, &binary, &missing],
            100,
            "read-texts-test",
        );
        assert_eq!(answer(&request), Ok(Value::Null));
        let files = slice("read-texts-test")?["files"].as_array().cloned();
        let files = files.ok_or_else(|| io::Error::other("files are not an array"))?;
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
        let request = request("read-texts", &[&large], 4, "read-limit-test");
        assert_eq!(answer(&request), Ok(Value::Null));
        let file = slice("read-limit-test")?["files"][0].clone();
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
        let path = dir.root();
        assert_eq!(
            answer(&request(
                "list-directories",
                &[&path],
                10,
                "list-links-test"
            )),
            Ok(Value::Null)
        );
        let listed = slice("list-links-test")?["paths"][0].clone();
        let link = entries(&listed)?
            .iter()
            .find(|entry| entry["name"] == json!("link"))
            .ok_or_else(|| io::Error::other("link entry is missing"))?;
        assert_eq!(link["link"], json!(true));
        assert_eq!(link["directory"], json!(false));
        Ok(())
    }
}
