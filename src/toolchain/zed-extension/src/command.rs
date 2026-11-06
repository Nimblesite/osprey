//! Launch policy, isolated from Zed's WASM-only host imports [EDITOR-ZED-LAUNCH].

use zed_extension_api::{settings::CommandSettings, Command, Result};

pub(super) fn resolve(
    binary: Option<CommandSettings>,
    which: impl FnOnce() -> Option<String>,
) -> Result<Command> {
    let settings = binary.unwrap_or(CommandSettings {
        path: None,
        arguments: None,
        env: None,
    });
    let command = match settings.path {
        Some(path) if path.trim().is_empty() => {
            return Err("Osprey binary.path must not be blank".into())
        }
        Some(path) => path,
        None => which()
            .ok_or("Osprey was not found on PATH; install Osprey or set lsp.osprey.binary.path")?,
    };
    Ok(Command {
        command,
        args: settings.arguments.unwrap_or_else(|| vec!["lsp".into()]),
        env: settings.env.unwrap_or_default().into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use std::collections::HashMap;
    use zed_extension_api::settings::CommandSettings;

    // Host imports cannot execute outside Zed; isolate only the launch decision.
    // Implements [EDITOR-ZED-LAUNCH]. Real stdio coverage belongs to the LSP suite.
    #[test]
    fn launch_policy_preserves_paths_arguments_and_environment() -> Result<(), String> {
        let command = resolve(None, || Some("/tools/Osprey Compiler/osprey".into()))?;
        assert_eq!(command.command, "/tools/Osprey Compiler/osprey");
        assert_eq!(command.args, ["lsp"]);
        assert_eq!(command.env, Vec::new());

        for arguments in [
            None,
            Some(vec!["lsp".into(), "--trace".into()]),
            Some(vec![]),
        ] {
            let expected = arguments.clone().unwrap_or_else(|| vec!["lsp".into()]);
            let command = resolve(
                Some(CommandSettings {
                    path: Some("/custom path/osprey".into()),
                    arguments,
                    env: Some(HashMap::from([(
                        "OSPREY_TEST".into(),
                        "value with spaces".into(),
                    )])),
                }),
                || None,
            )?;
            assert_eq!(command.command, "/custom path/osprey");
            assert_eq!(command.args, expected);
            assert_eq!(
                command.env,
                [("OSPREY_TEST".into(), "value with spaces".into())]
            );
        }
        let command = resolve(
            Some(CommandSettings {
                path: None,
                arguments: Some(vec!["lsp".into()]),
                env: None,
            }),
            || Some("/path/osprey".into()),
        )?;
        assert_eq!(command.command, "/path/osprey");
        assert_eq!(command.args, ["lsp"]);
        Ok(())
    }

    #[test]
    fn missing_or_blank_binary_is_an_explicit_error() {
        assert_eq!(
            resolve(None, || None).err().as_deref(),
            Some("Osprey was not found on PATH; install Osprey or set lsp.osprey.binary.path")
        );
        for path in ["", " ", "\t\n"] {
            let result = resolve(
                Some(CommandSettings {
                    path: Some(path.into()),
                    arguments: None,
                    env: None,
                }),
                || Some("/fallback/osprey".into()),
            );
            assert_eq!(
                result.err().as_deref(),
                Some("Osprey binary.path must not be blank")
            );
        }
    }
}
