//! Persistent Hunt reports and the investigation context [SYMEX-REPORT-HTML].
use super::{options::Options, report::Report, report_html, toolchain::Toolchain};
use crate::native::scratch_stem;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Saved files, printed only in text mode to keep JSON stdout valid.
#[derive(Debug, Default)]
pub(super) struct Saved {
    pub(super) html: Option<PathBuf>,
    pub(super) json: Option<PathBuf>,
}

pub(super) fn write(report: &Report<'_>, json: &str) -> Result<Saved, String> {
    if !report.options.reports.html && !report.options.reports.json {
        return Ok(Saved::default());
    }
    let directory = directory(report.options, report.program);
    std::fs::create_dir_all(&directory).map_err(|error| failure(&directory, &error))?;
    let json_path = save(&directory, "report.json", json, report.options.reports.json)?;
    let html = report_html::render(report, json);
    let html_path = save(
        &directory,
        "report.html",
        &html,
        report.options.reports.html,
    )?;
    Ok(Saved {
        html: html_path,
        json: json_path,
    })
}

fn directory(options: &Options, program: &str) -> PathBuf {
    options
        .out
        .clone()
        .unwrap_or_else(|| Path::new("target/hunt").join(scratch_stem(program)))
}

fn save(
    directory: &Path,
    name: &str,
    text: &str,
    enabled: bool,
) -> Result<Option<PathBuf>, String> {
    if !enabled {
        return Ok(None);
    }
    let path = directory.join(name);
    std::fs::write(&path, text).map_err(|error| failure(&path, &error))?;
    path.canonicalize()
        .map(Some)
        .map_err(|error| failure(&path, &error))
}

fn failure(path: &Path, error: &std::io::Error) -> String {
    format!("cannot save Hunt report {}: {error}", path.display())
}

pub(super) fn context(
    report: &Report<'_>,
    tools: &Toolchain,
    args: &[String],
    directory: &Path,
    stderr: &Path,
) -> Result<Value, String> {
    Ok(json!({
        "compilerVersion": env!("CARGO_PKG_VERSION"),
        "engineExecutable": tools.klee, "runtimeBitcode": tools.runtime,
        "engineArguments": args, "maxMemoryMb": report.options.max_memory,
        "nativeReplayEnabled": report.options.replay,
        "workingDirectory": std::env::current_dir().map_err(|e| e.to_string())?,
        "engineStderr": read(stderr)?, "engineReports": diagnostics(directory)?,
        "sources": sources(report), "symbolicObjects": symbolic_objects(report, directory),
        "interpretation": "Only a reproduced native failure confirms a crash. Unsupported operations and unfinished paths limit analysis. Safe Osprey memory findings can indicate compiler, runtime, FFI or engine-model defects. No findings is not proof of correctness."
    }))
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn diagnostics(directory: &Path) -> Result<Value, String> {
    let mut result = serde_json::Map::new();
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.is_file()
            && (path
                .extension()
                .is_some_and(|ext| ext == "err" || ext == "early")
                || matches!(name.as_str(), "info" | "messages.txt" | "warnings.txt"))
        {
            let _ = result.insert(name, Value::String(read(&path)?));
        }
    }
    Ok(Value::Object(result))
}

fn sources(report: &Report<'_>) -> Value {
    let files: BTreeSet<&str> = std::iter::once(report.program)
        .chain(
            report
                .scan
                .findings
                .iter()
                .filter_map(|finding| finding.location.as_ref().map(|l| l.file.as_str())),
        )
        .collect();
    Value::Object(
        files
            .into_iter()
            .map(|file| {
                let source = match std::fs::read_to_string(source_path(report.program, file)) {
                    Ok(text) => json!({"content": text}),
                    Err(error) => json!({"unavailable": error.to_string()}),
                };
                (file.to_string(), source)
            })
            .collect(),
    )
}

fn source_path<'a>(program: &'a str, file: &'a str) -> &'a Path {
    let path = Path::new(file);
    let program = Path::new(program);
    if path.components().count() == 1 && path.file_name() == program.file_name() {
        program
    } else {
        path
    }
}

fn symbolic_objects(report: &Report<'_>, directory: &Path) -> Value {
    Value::Object(
        report
            .scan
            .findings
            .iter()
            .map(|finding| {
                let path = directory.join(format!("{}.ktest", finding.test));
                let objects = std::fs::read(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|bytes| super::ktest::parse(&bytes));
                let value = match objects {
                    Ok(test) => json!({"objects": test.objects.iter().map(|object|
                json!({"name": object.name, "bytes": object.bytes})
            ).collect::<Vec<_>>()}),
                    Err(error) => json!({"unavailable": error}),
                };
                (finding.test.clone(), value)
            })
            .collect(),
    )
}
