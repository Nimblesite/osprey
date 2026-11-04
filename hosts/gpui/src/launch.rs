//! Packaged application arguments for the shared host. [CAST-MOBILE-GPUI]

use std::path::{Path, PathBuf};

/// Command-line arguments take precedence over defaults beside the executable.
pub fn arguments() -> Result<Vec<String>, String> {
    let given = crate::self_test::app_arguments();
    let resources = resources()?;
    let resources = resources.as_path();
    let path = resources.join("osprey-arguments.json");
    if !path
        .try_exists()
        .map_err(|error| format!("Cannot inspect application arguments: {error}"))?
    {
        return Ok(given);
    }
    let source = std::fs::read_to_string(path)
        .map_err(|error| format!("Cannot read application arguments: {error}"))?;
    configured(given, resources, &source)
}

/// Where the application's packaged files are: beside the executable, or on Android, where
/// an APK has no folder to read, where [`unpack`] put them.
fn resources() -> Result<PathBuf, String> {
    if cfg!(target_os = "android") {
        return Ok(PathBuf::from(crate::home()?).join("resources"));
    }
    let executable = std::env::current_exe()
        .map_err(|error| format!("Cannot locate the application: {error}"))?;
    executable
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "The application has no resource folder".to_string())
}

/// Writes each file named in `manifest`, one relative path a line, from `read` into `folder`.
/// A path that would leave the folder or a file that cannot be read stops the launch.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
pub(crate) fn unpack(
    manifest: &str,
    read: impl Fn(&str) -> Result<Vec<u8>, String>,
    folder: &Path,
) -> Result<(), String> {
    for name in manifest.lines().filter(|line| !line.is_empty()) {
        let relative = Path::new(name);
        if !relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
        {
            return Err(format!("Packaged file {name} leaves the application folder"));
        }
        let target = folder.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
        }
        std::fs::write(&target, read(name)?)
            .map_err(|error| format!("Cannot write {}: {error}", target.display()))?;
    }
    Ok(())
}

/// Decode only string arguments; malformed configuration never falls back silently.
pub(crate) fn configured(
    mut given: Vec<String>,
    resources: &Path,
    source: &str,
) -> Result<Vec<String>, String> {
    let bundled: Vec<String> = serde_json::from_str(source)
        .map_err(|error| format!("Invalid application arguments: {error}"))?;
    let folder = resources
        .to_str()
        .ok_or("The application resource path is not UTF-8")?;
    given.extend(
        bundled
            .into_iter()
            .map(|value| value.replace("${RESOURCES}", folder)),
    );
    Ok(given)
}

#[cfg(test)]
mod unpack_tests {
    use super::unpack;

    fn folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("osprey-unpack-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn packaged_files_land_in_their_folders() {
        let folder = folder("files");
        let read = |name: &str| Ok(format!("bytes of {name}").into_bytes());
        unpack("player.machine.json\nskins/a/b.png\n\n", read, &folder).unwrap();
        assert_eq!(std::fs::read_to_string(folder.join("skins/a/b.png")).unwrap(), "bytes of skins/a/b.png");
        assert_eq!(std::fs::read_to_string(folder.join("player.machine.json")).unwrap(), "bytes of player.machine.json");
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_path_leaving_the_folder_or_an_unreadable_file_stops_the_launch() {
        let folder = folder("refused");
        let read = |_: &str| Ok(Vec::new());
        for bad in ["../outside", "/etc/outside", "skins/../../outside"] {
            let problem = unpack(bad, read, &folder).unwrap_err();
            assert!(problem.contains("leaves the application folder"), "{problem}");
        }
        let missing = |name: &str| Err(format!("no {name}"));
        assert_eq!(unpack("gone", missing, &folder).unwrap_err(), "no gone");
        assert!(!folder.join("outside").exists());
        let _ = std::fs::remove_dir_all(folder);
    }
}
