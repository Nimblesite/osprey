//! Thin Zed host for the existing Osprey stdio language server [EDITOR-ZED].

mod command;

use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Worktree};

struct Osprey;

impl zed::Extension for Osprey {
    fn new() -> Self {
        Self
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> zed::Result<zed::Command> {
        let settings = LspSettings::for_worktree(id.as_ref(), worktree)?;
        command::resolve(settings.binary, || worktree.which("osprey"))
    }

    fn language_server_initialization_options(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> zed::Result<Option<serde_json::Value>> {
        Ok(LspSettings::for_worktree(id.as_ref(), worktree)?.initialization_options)
    }

    fn language_server_workspace_configuration(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> zed::Result<Option<serde_json::Value>> {
        Ok(LspSettings::for_worktree(id.as_ref(), worktree)?.settings)
    }
}

// Keep the upstream macro's C-ABI initializer private to the host boundary.
mod registration {
    zed_extension_api::register_extension!(super::Osprey);
}
