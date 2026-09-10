//! Validated export options. Unknown or incomplete options never generate pages.
//! Implements [DOC-EXPORT].

use osprey_syntax::Flavor;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Format {
    Markdown,
    Html,
}

pub(super) struct Options {
    pub(super) directory: PathBuf,
    pub(super) source: Option<String>,
    pub(super) flavor: Option<Flavor>,
    pub(super) format: Format,
    pub(super) theme: String,
    pub(super) pages: Vec<PathBuf>,
    pub(super) css: Vec<PathBuf>,
}

impl Options {
    pub(super) fn parse(args: &[String]) -> Result<Self, String> {
        let mut options = Self {
            directory: PathBuf::new(),
            source: None,
            flavor: None,
            format: Format::Markdown,
            theme: "osprey".into(),
            pages: Vec::new(),
            css: Vec::new(),
        };
        let mut arguments = args.iter();
        while let Some(argument) = arguments.next() {
            if argument == "--docs" {
                continue;
            }
            options.argument(argument, &mut arguments)?;
        }
        if options.directory.as_os_str().is_empty() {
            return Err("--docs requires --docs-dir <directory>".into());
        }
        if options.format == Format::Markdown
            && (!options.css.is_empty() || options.theme != "osprey")
        {
            return Err("custom CSS and themes require --docs-format html".into());
        }
        Ok(options)
    }

    fn argument<'a>(
        &mut self,
        argument: &str,
        args: &mut impl Iterator<Item = &'a String>,
    ) -> Result<(), String> {
        match argument {
            "--docs-dir" => self.directory = value(args, argument)?.into(),
            "--source" => self.set_source(value(args, argument)?)?,
            "--flavor" => self.flavor = Some(value(args, argument)?.parse()?),
            "--docs-format" => self.format = parse_format(value(args, argument)?)?,
            "--docs-theme" => self.theme = parse_theme(value(args, argument)?)?,
            "--docs-page" => self.pages.push(value(args, argument)?.into()),
            "--docs-css" => self.css.push(value(args, argument)?.into()),
            flag if flag.starts_with('-') => {
                return Err(format!("unknown documentation option {flag}"))
            }
            path => self.set_source(path)?,
        }
        Ok(())
    }

    fn set_source(&mut self, source: &str) -> Result<(), String> {
        if self.source.is_some() {
            return Err("specify one documentation source or project".into());
        }
        self.source = Some(source.into());
        Ok(())
    }
}

fn value<'a>(args: &mut impl Iterator<Item = &'a String>, option: &str) -> Result<&'a str, String> {
    match args.next() {
        Some(value) if !value.starts_with('-') => Ok(value),
        _ => Err(format!("{option} requires a value")),
    }
}

fn parse_format(value: &str) -> Result<Format, String> {
    match value {
        "markdown" | "md" => Ok(Format::Markdown),
        "html" => Ok(Format::Html),
        other => Err(format!(
            "unknown documentation format {other}; expected markdown or html"
        )),
    }
}

fn parse_theme(value: &str) -> Result<String, String> {
    match value {
        "osprey" | "midnight" | "paper" => Ok(value.into()),
        other => Err(format!(
            "unknown documentation theme {other}; expected osprey, midnight or paper"
        )),
    }
}
