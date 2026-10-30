//! Thin GPUI native-panel and file-viewer adapters.

use std::future::Future;
use std::path::{Path, PathBuf};

use gpui_kit::{Context, PathPromptOptions, Window};
use serde_json::{json, Value};

use super::{field, files, number, strings};
use crate::envelope::Request;
use crate::events;
use crate::ffi;
use crate::host::{now, HostView};
use crate::store;

/// Starts a native panel or answers reveal, returning `None` while a panel runs.
pub fn dispatch(
    request: &Request,
    window: &mut Window,
    cx: &mut Context<HostView>,
) -> Option<String> {
    match request.kind.as_str() {
        "choose-file" => launch(request, window, cx, choose_file(request, cx)),
        "choose-folder" => launch(request, window, cx, choose_folder(cx)),
        "save-file" => launch(request, window, cx, save_file(request, cx)),
        "reveal" => Some(completion(request, reveal(request, cx))),
        kind => Some(completion(
            request,
            Err(format!("unsupported capability {kind}")),
        )),
    }
}

/// Prepares the single-file prompt and its optional bounded content read. GPUI's prompt offers
/// no extension filter, so a chosen file outside `types` is refused once chosen.
fn choose_file<'a>(
    request: &Request,
    cx: &Context<'a, HostView>,
) -> Result<impl Future<Output = Result<Value, String>>, String> {
    let types: Vec<String> = strings(request, "types")?
        .into_iter()
        .map(|kind| kind.trim_start_matches('.').to_ascii_lowercase())
        .collect();
    let limit = number(request, "limit")?;
    let slice = field(request, "slice")?.to_owned();
    let receiver = cx.prompt_for_paths(options(true, false));
    Ok(async move {
        let paths = receiver
            .await
            .map_err(|problem| problem.to_string())?
            .map_err(|problem| problem.to_string())?;
        let path = paths.and_then(|paths| paths.into_iter().next());
        if let Some(path) = &path {
            if !accepted(path, &types) {
                return Err(format!(
                    "{} is not one of {}",
                    path.display(),
                    types.join(", ")
                ));
            }
            publish_chosen(path, &slice, limit)?;
        }
        Ok(chosen(path))
    })
}

/// Prepares the single-folder prompt.
fn choose_folder<'a>(
    cx: &Context<'a, HostView>,
) -> Result<impl Future<Output = Result<Value, String>>, String> {
    let receiver = cx.prompt_for_paths(options(false, true));
    Ok(async move {
        let paths = receiver
            .await
            .map_err(|problem| problem.to_string())?
            .map_err(|problem| problem.to_string())?;
        Ok(chosen(paths.and_then(|paths| paths.into_iter().next())))
    })
}

/// Prepares the save prompt with the staged text captured before the user chooses.
fn save_file<'a>(
    request: &Request,
    cx: &Context<'a, HostView>,
) -> Result<impl Future<Output = Result<Value, String>>, String> {
    let name = field(request, "name")?;
    let text = ffi::get_slice(field(request, "slice")?).ok_or("save-file slice is missing")?;
    let directory = crate::home()
        .map(PathBuf::from)
        .or_else(|_| std::env::current_dir().map_err(|problem| problem.to_string()))?;
    let receiver = cx.prompt_for_new_path(&directory, Some(name));
    Ok(async move {
        let path = receiver
            .await
            .map_err(|problem| problem.to_string())?
            .map_err(|problem| problem.to_string())?;
        if let Some(path) = &path {
            write_chosen(path, &text)?;
        }
        Ok(chosen(path))
    })
}

/// Replaces the chosen destination through the same staged sibling the store uses.
fn write_chosen(path: &Path, text: &str) -> Result<(), String> {
    let staged = store::temporary(path);
    std::fs::write(&staged, text).map_err(|problem| problem.to_string())?;
    std::fs::rename(&staged, path).map_err(|problem| problem.to_string())?;
    Ok(())
}

/// Validates and reveals one absolute path in the platform file viewer.
fn reveal(request: &Request, cx: &Context<HostView>) -> Result<Value, String> {
    let path = field(request, "path")?;
    if !Path::new(path).is_absolute() {
        return Err("Reveal needs an absolute path".to_owned());
    }
    cx.reveal_path(Path::new(path));
    Ok(json!({}))
}

/// Builds GPUI's single-selection path options.
fn options(files: bool, directories: bool) -> PathPromptOptions {
    PathPromptOptions {
        files,
        directories,
        multiple: false,
        prompt: None,
    }
}

/// Whether `path` has one of the requested extensions; an empty list accepts every file.
fn accepted(path: &Path, types: &[String]) -> bool {
    types.is_empty()
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                types
                    .iter()
                    .any(|kind| kind.eq_ignore_ascii_case(extension))
            })
}

/// Publishes selected file text only when the request names a nonempty slice and positive limit.
fn publish_chosen(path: &Path, slice: &str, limit: u64) -> Result<(), String> {
    if slice.is_empty() || limit == 0 {
        return Ok(());
    }
    let text = files::read_text(&path.to_string_lossy(), limit)?;
    ffi::put_slice(slice, &text)
}

/// Converts a native panel selection to the Swift host's path/cancelled shape.
fn chosen(path: Option<PathBuf>) -> Value {
    match path {
        Some(path) => json!({"path": path.to_string_lossy(), "cancelled": false}),
        None => json!({"path": "", "cancelled": true}),
    }
}

/// Spawns a prepared panel operation or returns its validation failure immediately.
fn launch<F>(
    request: &Request,
    window: &mut Window,
    cx: &mut Context<HostView>,
    operation: Result<F, String>,
) -> Option<String>
where
    F: Future<Output = Result<Value, String>> + 'static,
{
    match operation {
        Ok(operation) => {
            spawn(request, window, cx, operation);
            None
        }
        Err(problem) => Some(completion(request, Err(problem))),
    }
}

/// Delivers an asynchronous panel completion through the normal view dispatch seam.
fn spawn<F>(request: &Request, window: &mut Window, cx: &mut Context<HostView>, operation: F)
where
    F: Future<Output = Result<Value, String>> + 'static,
{
    let ticket = request.ticket.clone();
    cx.spawn_in(window, async move |view, cx| {
        let completion = match operation.await {
            Ok(result) => events::completed_ok(now(), &ticket, result),
            Err(problem) => events::completed_err(now(), &ticket, &problem),
        };
        let _ = view.update_in(cx, |view, window, cx| view.dispatch(completion, window, cx));
    })
    .detach();
}

/// Wraps a synchronous platform result in its ticketed completion.
fn completion(request: &Request, result: Result<Value, String>) -> String {
    match result {
        Ok(result) => events::completed_ok(now(), &request.ticket, result),
        Err(problem) => events::completed_err(now(), &request.ticket, &problem),
    }
}

#[cfg(test)]
mod tests {
    use super::accepted;
    use std::path::Path;

    /// A chosen file must carry one of the requested extensions, in any case; none requested accepts all.
    #[test]
    fn chosen_files_are_checked_against_the_requested_types() {
        let types = vec!["txt".to_owned(), "md".to_owned()];
        assert!(accepted(Path::new("/tmp/notes.TXT"), &types));
        assert!(accepted(Path::new("/tmp/readme.md"), &types));
        assert!(!accepted(Path::new("/tmp/photo.png"), &types));
        assert!(!accepted(Path::new("/tmp/Makefile"), &types));
        assert!(accepted(Path::new("/tmp/anything"), &[]));
    }
}
