//! Offline post-processing for Osprey's sampling CPU profiler.
//!
//! The C runtime writes a raw, unsymbolized profile at exit
//! ([PROF-RAW-FORMAT]); this crate turns that file into human- and
//! tool-friendly artifacts. It symbolizes raw addresses
//! ([PROF-SYMBOLIZE-OFFLINE]), aggregates per-function, per-line, and
//! per-fiber statistics, writes the four exports ([PROF-CLI-RUN] —
//! speedscope, V8 `.cpuprofile`, collapsed stacks, and the editor summary),
//! and renders the terminal report ([PROF-CLI-REPORT]).

mod export;
mod model;
mod raw;
mod report;
mod symbolize;

#[cfg(test)]
mod e2e;
#[cfg(test)]
pub(crate) mod testutil;

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Failure modes of the profile post-processing pipeline. Every variant
/// carries enough context (path or message) to diagnose the failure from the
/// `Display` output alone.
#[derive(Debug)]
pub enum ProfileError {
    /// Reading or writing `path` failed.
    Io {
        /// The file or directory the operation touched.
        path: PathBuf,
        /// The underlying I/O error.
        source: std::io::Error,
    },
    /// `path` did not contain well-formed raw-profile JSON.
    Parse {
        /// The file that failed to parse.
        path: PathBuf,
        /// The underlying JSON error.
        source: serde_json::Error,
    },
    /// The raw profile parsed but violated a [PROF-RAW-FORMAT] invariant.
    Invalid(String),
    /// A symbolizer failed in a way that hex-name fallback cannot absorb.
    Symbolize(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "io error on {}: {source}", path.display()),
            Self::Parse { path, source } => {
                write!(
                    f,
                    "invalid raw profile JSON in {}: {source}",
                    path.display()
                )
            }
            Self::Invalid(message) => write!(f, "invalid raw profile: {message}"),
            Self::Symbolize(message) => write!(f, "symbolization failed: {message}"),
        }
    }
}

impl std::error::Error for ProfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
            Self::Invalid(_) | Self::Symbolize(_) => None,
        }
    }
}

/// Inputs to [`process_profile`]: where the raw profile and binary live, and
/// where/how the post-processed artifacts should be produced.
#[derive(Debug)]
pub struct ProfileOptions {
    /// Raw profile JSON written by the runtime at exit [PROF-RAW-FORMAT].
    pub raw_path: PathBuf,
    /// The executable that produced the profile (used for symbolization).
    pub binary_path: PathBuf,
    /// The `.osp` source path, shown in the report and summary export.
    pub source_path: String,
    /// Directory the export files are written into (created if missing).
    pub out_dir: PathBuf,
    /// Export basename: `<stem>.speedscope.json`, `<stem>.cpuprofile`, ….
    pub stem: String,
    /// Whether the terminal report uses ANSI colors.
    pub color: bool,
}

/// Everything [`process_profile`] produced: the rendered terminal report and
/// the paths of the four export files [PROF-CLI-RUN].
#[derive(Debug)]
pub struct ProfileOutcome {
    /// Terminal report text [PROF-CLI-REPORT], ready to print.
    pub report: String,
    /// Path of the speedscope export (`<stem>.speedscope.json`).
    pub speedscope_path: PathBuf,
    /// Path of the V8 export (`<stem>.cpuprofile`).
    pub cpuprofile_path: PathBuf,
    /// Path of the collapsed-stacks export (`<stem>.folded`).
    pub folded_path: PathBuf,
    /// Path of the editor summary export (`<stem>.profile.json`).
    pub summary_path: PathBuf,
}

/// Run the full post-processing pipeline [PROF-CLI-RUN]: parse the raw
/// profile, symbolize it against the binary, aggregate, write the four
/// exports into `out_dir`, and render the terminal report.
///
/// # Errors
///
/// Returns [`ProfileError`] when the raw profile cannot be read or parsed,
/// violates a format invariant, or an export file cannot be written. An
/// unavailable symbolizer is NOT an error — frames fall back to hex names.
pub fn process_profile(opts: &ProfileOptions) -> Result<ProfileOutcome, ProfileError> {
    let profile = raw::parse_file(&opts.raw_path)?;
    let sym = symbolize::tools::LlvmSymbolizer::new(&opts.binary_path, &profile.images);
    pipeline(opts, &profile, &sym)
}

/// Test seam: run the pipeline with an injected symbolizer.
#[cfg(test)]
pub(crate) fn process_with(
    opts: &ProfileOptions,
    sym: &dyn symbolize::Symbolize,
) -> Result<ProfileOutcome, ProfileError> {
    let profile = raw::parse_file(&opts.raw_path)?;
    pipeline(opts, &profile, sym)
}

/// Shared orchestration: symbolize → aggregate → export → report.
fn pipeline(
    opts: &ProfileOptions,
    profile: &raw::Profile,
    sym: &dyn symbolize::Symbolize,
) -> Result<ProfileOutcome, ProfileError> {
    let sym_stacks = symbolize::symbolize_stacks(profile, sym)?;
    let model = model::build_model(profile, &sym_stacks);
    fs::create_dir_all(&opts.out_dir).map_err(|source| ProfileError::Io {
        path: opts.out_dir.clone(),
        source,
    })?;
    let speedscope = export::speedscope::speedscope_json(profile, &model);
    let cpuprofile = export::cpuprofile::cpuprofile_json(profile, &model);
    let folded = export::folded::folded_text(profile, &model);
    let summary = export::summary::summary_json(&opts.source_path, &model);
    Ok(ProfileOutcome {
        report: report::render_report(&opts.source_path, &opts.stem, &model, opts.color),
        speedscope_path: write_json(&export_path(opts, "speedscope.json"), &speedscope)?,
        cpuprofile_path: write_json(&export_path(opts, "cpuprofile"), &cpuprofile)?,
        folded_path: write_text(&export_path(opts, "folded"), &folded)?,
        summary_path: write_json(&export_path(opts, "profile.json"), &summary)?,
    })
}

/// `<out_dir>/<stem>.<extension>` for one export artifact.
fn export_path(opts: &ProfileOptions, extension: &str) -> PathBuf {
    opts.out_dir.join(format!("{}.{extension}", opts.stem))
}

/// Write `contents` to `path`, wrapping failures with the path context.
fn write_text(path: &Path, contents: &str) -> Result<PathBuf, ProfileError> {
    fs::write(path, contents).map_err(|source| ProfileError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(path.to_path_buf())
}

/// Serialize `value` compactly and write it to `path`.
fn write_json(path: &Path, value: &serde_json::Value) -> Result<PathBuf, ProfileError> {
    write_text(path, &value.to_string())
}
