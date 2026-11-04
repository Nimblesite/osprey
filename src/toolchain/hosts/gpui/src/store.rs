//! Synchronous, app-scoped persistent store capabilities ([GPUI-HOST-CAPABILITIES]).

use std::ffi::OsString;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::envelope::Request;
use crate::events;
use crate::ffi;
use crate::host::now;

/// Answers one store request or returns the protocol's unsupported-capability error.
pub fn answer(request: &Request) -> Result<Value, String> {
    match request.kind.as_str() {
        "store-read" => read(request),
        "store-write" => write(request),
        "store-list" => list(request),
        "store-delete" => delete(request),
        kind => Err(format!("unsupported capability {kind}")),
    }
}

/// Wraps a store result in the completion event carrying the original ticket.
pub fn completion(request: &Request) -> String {
    match answer(request) {
        Ok(result) => events::completed_ok(now(), &request.ticket, result),
        Err(problem) => events::completed_err(now(), &request.ticket, &problem),
    }
}

/// Reads a bounded UTF-8 entry into the requested host-held slice.
fn read(request: &Request) -> Result<Value, String> {
    let name = field(request, "name")?;
    let limit = request
        .fields
        .get("limit")
        .and_then(Value::as_u64)
        .ok_or("store-read has no valid limit")?;
    let path = entry(name)?;
    let metadata = fs::metadata(&path).map_err(|error| read_error(name, error))?;
    if metadata.len() > limit {
        return Err(format!("store entry {name} exceeds limit {limit}"));
    }
    let bytes = fs::read(&path).map_err(|error| read_error(name, error))?;
    let text = String::from_utf8(bytes).map_err(|_| format!("store entry {name} is not UTF-8"))?;
    ffi::put_slice(field(request, "slice")?, &text)?;
    Ok(Value::Null)
}

/// Maps a missing read to its stable protocol message.
fn read_error(name: &str, error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        format!("no such store entry {name}")
    } else {
        format!("could not read store entry {name}: {error}")
    }
}

/// Atomically replaces an entry with text already held under a slice key.
fn write(request: &Request) -> Result<Value, String> {
    let path = entry(field(request, "name")?)?;
    let text = ffi::get_slice(field(request, "slice")?).ok_or("store-write slice is missing")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let temporary = temporary(&path);
    fs::write(&temporary, text).map_err(io_error)?;
    fs::rename(&temporary, &path).map_err(io_error)?;
    Ok(Value::Null)
}

/// Lists entry names in a folder in deterministic order.
fn list(request: &Request) -> Result<Value, String> {
    let folder = entry(field(request, "name")?)?;
    let entries = fs::read_dir(folder).map_err(io_error)?;
    let mut names = entries
        .map(|entry| {
            let entry = entry.map_err(io_error)?;
            entry
                .file_name()
                .into_string()
                .map_err(|_| "store entry name is not UTF-8".to_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;
    names.sort();
    Ok(Value::Array(names.into_iter().map(Value::String).collect()))
}

/// Deletes one store entry.
fn delete(request: &Request) -> Result<Value, String> {
    fs::remove_file(entry(field(request, "name")?)?).map_err(io_error)?;
    Ok(Value::Null)
}

/// Reads a required string argument from the verbatim request fields.
fn field<'a>(request: &'a Request, name: &str) -> Result<&'a str, String> {
    request
        .fields
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{} has no valid {name}", request.kind))
}

/// Resolves a traversal-free relative name beneath the application store root.
fn entry(name: &str) -> Result<PathBuf, String> {
    let relative = Path::new(name);
    let safe = !relative.is_absolute()
        && relative
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    if !safe {
        return Err(format!("invalid store entry {name}"));
    }
    Ok(root()?.join(relative))
}

/// Returns the macOS store root: `$HOME/Library/Application Support/Osprey/<app>`.
fn macos_root(home: &str, app: &str) -> PathBuf {
    Path::new(home)
        .join("Library/Application Support/Osprey")
        .join(app)
}

/// Returns the Linux store root: `$XDG_DATA_HOME/osprey/<app>`, falling back to
/// `$HOME/.local/share/osprey/<app>` when `XDG_DATA_HOME` is unset or empty.
fn linux_root(home: Option<&str>, data_home: Option<&str>, app: &str) -> Result<PathBuf, String> {
    let base = data_home
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home.filter(|value| !value.is_empty())
                .map(|home| Path::new(home).join(".local/share"))
        })
        .ok_or_else(|| "HOME is not set".to_owned())?;
    Ok(base.join("osprey").join(app))
}

/// Returns the Windows store root: `%APPDATA%\\Osprey\\<app>`.
fn windows_root(app_data: Option<&str>, app: &str) -> Result<PathBuf, String> {
    Ok(Path::new(app_data.ok_or("APPDATA is not set")?)
        .join("Osprey")
        .join(app))
}

/// Selects the current platform's store root from environment values.
fn root_from(
    home: Option<&str>,
    data_home: Option<&str>,
    app_data: Option<&str>,
    app: &str,
) -> Result<PathBuf, String> {
    if cfg!(target_os = "macos") {
        return Ok(macos_root(home.ok_or("HOME is not set")?, app));
    }
    if cfg!(windows) {
        return windows_root(app_data, app);
    }
    linux_root(home, data_home, app)
}

/// Returns the platform's application store root ([GPUI-HOST-CAPABILITIES]):
/// macOS `~/Library/Application Support/Osprey/<app>`, Linux
/// `$XDG_DATA_HOME/osprey/<app>` (or `~/.local/share/osprey/<app>`), Windows
/// `%APPDATA%\\Osprey\\<app>`.
fn root() -> Result<PathBuf, String> {
    let app = std::env::var("OSPREY_GPUI_APP_NAME").unwrap_or_else(|_| "osprey".to_owned());
    root_from(
        std::env::var("HOME").ok().as_deref(),
        std::env::var("XDG_DATA_HOME").ok().as_deref(),
        std::env::var("APPDATA").ok().as_deref(),
        &app,
    )
}

/// Uses the sibling `<name>.tmp` required by the atomic replacement protocol.
pub(crate) fn temporary(path: &Path) -> PathBuf {
    let mut name = OsString::from(path);
    name.push(".tmp");
    PathBuf::from(name)
}

/// Adds operation context without hiding the underlying I/O error.
fn io_error(error: std::io::Error) -> String {
    format!("store operation failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::{linux_root, macos_root, root_from, windows_root};
    use std::path::Path;

    #[test]
    /// Selects each platform's root from environment values alone, never the disk.
    fn root_selection_is_pure_over_environment_values() {
        assert_eq!(
            macos_root("/Users/a", "app"),
            Path::new("/Users/a/Library/Application Support/Osprey/app")
        );
        assert_eq!(
            linux_root(Some("/home/a"), Some("/data"), "app").as_deref(),
            Ok(Path::new("/data/osprey/app"))
        );
        assert_eq!(
            linux_root(Some("/home/a"), Some(""), "app").as_deref(),
            Ok(Path::new("/home/a/.local/share/osprey/app"))
        );
        assert_eq!(
            linux_root(Some("/home/a"), None, "app").as_deref(),
            Ok(Path::new("/home/a/.local/share/osprey/app"))
        );
        assert_eq!(
            linux_root(None, None, "app").err().as_deref(),
            Some("HOME is not set")
        );
        assert_eq!(
            windows_root(Some(r"C:\Users\a\AppData\Roaming"), "app").as_deref(),
            Ok(Path::new(r"C:\Users\a\AppData\Roaming")
                .join("Osprey")
                .join("app")
                .as_path())
        );
        assert_eq!(
            windows_root(None, "app").err().as_deref(),
            Some("APPDATA is not set")
        );
    }

    #[test]
    /// Picks the convention of the platform the test binary runs on.
    fn root_from_uses_the_current_platform() {
        let selected = root_from(Some("/h"), Some("/d"), Some("C:/ad"), "app");
        #[cfg(target_os = "macos")]
        assert_eq!(
            selected.as_deref(),
            Ok(Path::new("/h/Library/Application Support/Osprey/app"))
        );
        #[cfg(all(unix, not(target_os = "macos")))]
        assert_eq!(selected.as_deref(), Ok(Path::new("/d/osprey/app")));
        #[cfg(windows)]
        assert_eq!(selected.as_deref(), Ok(Path::new("C:/ad/Osprey/app")));
        #[cfg(target_os = "macos")]
        assert_eq!(
            root_from(None, None, None, "app").err().as_deref(),
            Some("HOME is not set")
        );
        #[cfg(not(target_os = "macos"))]
        assert!(selected.is_ok());
    }
}
