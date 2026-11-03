//! Packaged application arguments for the shared host. [CAST-MOBILE-GPUI]

use std::path::Path;

/// Command-line arguments take precedence over defaults beside the executable.
pub fn arguments() -> Result<Vec<String>, String> {
    let given = crate::self_test::app_arguments();
    let executable = std::env::current_exe()
        .map_err(|error| format!("Cannot locate the application: {error}"))?;
    let resources = executable
        .parent()
        .ok_or("The application has no resource folder")?;
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
