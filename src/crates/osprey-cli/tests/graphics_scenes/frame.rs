//! Execute the actual Direct3D frame path with injected SDK failures.
use std::{fs, path::Path, process::Command};

const FUNCTIONS: [&str; 6] = [
    "osp_gfx_await",
    "osp_gfx_wait",
    "osp_gfx_submit",
    "osp_gfx_frame",
    "osp_gfx_draw",
    "osp_gfx_close",
];

fn function<'a>(source: &'a str, name: &str) -> Result<&'a str, String> {
    let call = source
        .find(&format!("{name}("))
        .ok_or_else(|| format!("missing {name}"))?;
    let start = source[..call].rfind('\n').map_or(0, |i| i + 1);
    let brace = source[call..]
        .find('{')
        .ok_or_else(|| format!("missing body {name}"))?
        + call;
    let end = body_end(&source[brace..], name)?;
    Ok(&source[start..=brace + end])
}

fn body_end(source: &str, name: &str) -> Result<usize, String> {
    let mut depth = 0;
    for (offset, ch) in source.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            return Ok(offset);
        }
    }
    Err(format!("unclosed body {name}"))
}

fn run(command: &mut Command) -> Result<(), String> {
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{command:?}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// The shared ABI header both bridges include.
const ABI_HEADER: &str = "ospgfx_abi.h";

/// The frame functions are compiled against the REAL shared ABI header,
/// included exactly as the bridges include it, so the one C declaration they
/// share is itself compiled and checked on every platform that runs this
/// suite, not only where a bridge builds.
pub(super) fn verify(root: &Path) -> Result<(), String> {
    let graphics = root.join("src/examples/graphics");
    let source =
        fs::read_to_string(graphics.join("ospgfx_d3d12.c")).map_err(|error| error.to_string())?;
    let bodies = FUNCTIONS
        .iter()
        .map(|name| function(&source, name))
        .collect::<Result<Vec<_>, _>>()?
        .join("\n");
    execute(
        &graphics,
        &format!(
            "#include \"{ABI_HEADER}\"\n{}\n{bodies}\n{}",
            include_str!("frame_stubs.h"),
            include_str!("frame_cases.c")
        ),
    )
}

fn execute(include_dir: &Path, source: &str) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("osprey-frame-failures-{}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let code = dir.join("frame.c");
    fs::write(&code, source).map_err(|error| error.to_string())?;
    let binary = dir.join("frame.exe");
    run(Command::new("clang")
        .args(["-std=c11", "-Wall", "-Wextra", "-Werror", "-I"])
        .arg(include_dir)
        .arg(&code)
        .arg("-o")
        .arg(&binary))?;
    run(&mut Command::new(&binary))?;
    fs::remove_dir_all(&dir).map_err(|error| error.to_string())
}
