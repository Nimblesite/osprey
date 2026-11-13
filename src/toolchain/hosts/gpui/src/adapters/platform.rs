//! Thin GPUI native-panel and file-viewer adapters.

use std::future::Future;
use std::path::{Path, PathBuf};

use gpui_kit::{Context, PathPromptOptions, Window};
use serde_json::json;

use super::{files, number, text};
use crate::events::Touch;
use crate::ffi;
use crate::host::HostView;
use crate::rows::Ask;
use crate::store;

/// Starts a native panel or answers reveal, returning `None` while a panel runs.
pub fn dispatch(ask: &Ask, window: &mut Window, cx: &mut Context<HostView>) -> Option<Touch> {
    match ask.kind() {
        "choose-file" => launch(ask, window, cx, choose_file(ask, cx)),
        "choose-folder" => launch(ask, window, cx, choose_folder(cx)),
        "save-file" => launch(ask, window, cx, save_file(ask, cx)),
        "reveal" => Some(completion(ask, reveal(ask, cx))),
        kind => Some(completion(
            ask,
            Err(format!("unsupported capability {kind}")),
        )),
    }
}

/// Prepares the single-file prompt and its optional bounded content read. GPUI's prompt offers
/// no extension filter, so a chosen file outside `types` is refused once chosen.
fn choose_file<'a>(
    ask: &Ask,
    cx: &Context<'a, HostView>,
) -> Result<impl Future<Output = Result<String, String>>, String> {
    let types: Vec<String> = ask
        .texts(1)
        .iter()
        .map(|kind| kind.trim_start_matches('.').to_ascii_lowercase())
        .collect();
    let limit = number(ask, 2, "limit")?;
    let slice = text(ask, 3, "slice")?;
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
) -> Result<impl Future<Output = Result<String, String>>, String> {
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
    ask: &Ask,
    cx: &Context<'a, HostView>,
) -> Result<impl Future<Output = Result<String, String>>, String> {
    let name = text(ask, 1, "name")?;
    let staged = ffi::get_slice(&text(ask, 2, "slice")?).ok_or("save-file slice is missing")?;
    let directory = crate::home()
        .map(PathBuf::from)
        .or_else(|_| std::env::current_dir().map_err(|problem| problem.to_string()))?;
    let receiver = cx.prompt_for_new_path(&directory, Some(name.as_str()));
    Ok(async move {
        let path = receiver
            .await
            .map_err(|problem| problem.to_string())?
            .map_err(|problem| problem.to_string())?;
        if let Some(path) = &path {
            write_chosen(path, &staged)?;
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
fn reveal(ask: &Ask, cx: &Context<HostView>) -> Result<String, String> {
    let path = text(ask, 1, "path")?;
    if !Path::new(&path).is_absolute() {
        return Err("Reveal needs an absolute path".to_owned());
    }
    cx.reveal_path(Path::new(&path));
    Ok(json!({}).to_string())
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

/// Converts a native panel selection to the completion's bare path text.
fn chosen(path: Option<PathBuf>) -> String {
    path.map_or_else(String::new, |path| path.to_string_lossy().into_owned())
}

/// Spawns a prepared panel operation or returns its validation failure immediately.
fn launch<F>(
    ask: &Ask,
    window: &mut Window,
    cx: &mut Context<HostView>,
    operation: Result<F, String>,
) -> Option<Touch>
where
    F: Future<Output = Result<String, String>> + 'static,
{
    match operation {
        Ok(operation) => {
            spawn(ask, window, cx, operation);
            None
        }
        Err(problem) => Some(completion(ask, Err(problem))),
    }
}

/// Delivers an asynchronous panel completion through the normal view dispatch seam.
fn spawn<F>(ask: &Ask, window: &mut Window, cx: &mut Context<HostView>, operation: F)
where
    F: Future<Output = Result<String, String>> + 'static,
{
    let ticket = ask.ticket.clone();
    cx.spawn_in(window, async move |view, cx| {
        let completion = operation.await;
        let touch = Touch::completed(&ticket, completion);
        let _ = view.update_in(cx, |view, window, cx| view.send(touch, window, cx));
    })
    .detach();
}

/// Wraps a synchronous platform result in its ticketed completion.
fn completion(ask: &Ask, result: Result<String, String>) -> Touch {
    Touch::completed(&ask.ticket, result)
}

#[cfg(test)]
mod tests {
    use super::{accepted, chosen};
    use std::path::{Path, PathBuf};

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

    /// A chosen path completes as bare text, and a cancelled prompt as the empty string.
    #[test]
    fn chosen_paths_complete_as_bare_text() {
        assert_eq!(
            chosen(Some(PathBuf::from("/tmp/notes.txt"))),
            "/tmp/notes.txt"
        );
        assert_eq!(chosen(None), String::new());
    }
}
