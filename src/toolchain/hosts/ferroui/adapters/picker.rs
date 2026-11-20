//! The native file picker owned by this FerroUI window. A cancelled panel returns an empty path.
use std::path::Path;
use std::rc::Rc;

use ferroui_base::platform::storage::{
    FilePickerFileType, FilePickerOpenOptions, IStorageItem, IStorageProvider,
};

use crate::{ffi, rows::Ask};

pub(crate) async fn choose(
    provider: Rc<dyn IStorageProvider>,
    ask: &Ask,
) -> Result<String, String> {
    if !provider.can_open() {
        return Err("This window has no file picker.".into());
    }
    let types = ask.texts(1);
    let limit = super::number(ask, 2, "limit")?;
    let slice = super::text(ask, 3, "slice")?;
    let options = options(&types);
    let files = provider
        .open_file_picker_async(options)
        .await
        .map_err(|e| e.to_string())?;
    let Some(file) = files.first() else {
        return Ok(String::new());
    };
    let item: &dyn IStorageItem = file.as_ref();
    let path = item.try_get_local_path();
    for file in files {
        file.dispose();
    }
    let path = path.ok_or("The selected file has no local path.")?;
    selected(&path, &types, &slice, limit)?;
    Ok(path)
}

fn options(types: &[String]) -> FilePickerOpenOptions {
    let options = FilePickerOpenOptions::new();
    if types.is_empty() {
        return options;
    }
    let filter = FilePickerFileType::new(Some(&types.join(", ")));
    filter.set_patterns(Some(
        types
            .iter()
            .map(|kind| format!("*.{}", kind.trim_start_matches('.')))
            .collect(),
    ));
    options.with_file_type_filter(vec![filter])
}

fn selected(path: &str, types: &[String], slice: &str, limit: u64) -> Result<(), String> {
    let extension = Path::new(path).extension().and_then(|value| value.to_str());
    if !types.is_empty()
        && !types.iter().any(|kind| {
            extension.is_some_and(|ext| ext.eq_ignore_ascii_case(kind.trim_start_matches('.')))
        })
    {
        return Err(format!("{path} is not one of {}", types.join(", ")));
    }
    if !slice.is_empty() && limit > 0 {
        ffi::put_slice(slice, &super::files::read_text(path, limit)?)?;
    }
    Ok(())
}
