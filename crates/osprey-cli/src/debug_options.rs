//! Native debug controls and validation. Implements [DEBUGGER-BUILD-OPTIONS].
use crate::invocation::Cli;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DebugOptions {
    pub(crate) info: DebugInfo,
    pub(crate) optimization: DebugOptimization,
    pub(crate) preserve_ir: bool,
    pub(crate) preserve_symbols: bool,
    pub(crate) explicit_output: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DebugInfo {
    #[default]
    Dwarf,
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DebugOptimization {
    #[default]
    Environment,
    Unoptimized,
}

pub(crate) fn parse(cli: &mut Cli, flag: &str) -> Result<bool, String> {
    match flag {
        "--debug-preserve-ir" => cli.debug_options.preserve_ir = true,
        "--debug-preserve-symbols" => cli.debug_options.preserve_symbols = true,
        "--debug-info=dwarf" => cli.debug_options.info = DebugInfo::Dwarf,
        "--debug-info=none" => cli.debug_options.info = DebugInfo::None,
        "--debug-opt=none" => cli.debug_options.optimization = DebugOptimization::Unoptimized,
        "--debug-memory=off" => (),
        _ => return unsupported(flag),
    }
    cli.debug = true;
    Ok(true)
}

fn unsupported(flag: &str) -> Result<bool, String> {
    let Some((name, _)) = flag.split_once('=') else {
        return Ok(false);
    };
    let choices = match name {
        "--debug-info" => "dwarf or none",
        "--debug-opt" => "none (limited and optimized debugging are not implemented)",
        "--debug-memory" => "off (object-graph and timeline inspection are not implemented)",
        _ => return Ok(false),
    };
    Err(format!("unsupported {flag}; supported: {choices}"))
}

pub(crate) fn set_output(cli: &mut Cli, output: &str, debug: bool) -> Result<(), String> {
    if (debug || cli.debug_options.explicit_output)
        && cli
            .output
            .as_deref()
            .is_some_and(|previous| previous != output)
    {
        return Err("--debug-out and -o must name the same output".into());
    }
    if output.is_empty() {
        return Err("output path cannot be empty".into());
    }
    cli.output = Some(output.into());
    cli.debug_options.explicit_output |= debug;
    cli.debug |= debug;
    Ok(())
}

pub(crate) fn validate(cli: &Cli) -> Result<(), String> {
    let options = cli.debug_options;
    if options.preserve_symbols && options.info == DebugInfo::None {
        return Err("--debug-preserve-symbols requires --debug-info=dwarf".into());
    }
    if options.keeps_artifacts() && !matches!(cli.mode.as_str(), "--compile" | "--run") {
        return Err("debug artifact options require --compile or --run".into());
    }
    Ok(())
}

impl DebugOptions {
    pub(crate) fn keeps_artifacts(self) -> bool {
        self.preserve_ir || self.preserve_symbols || self.explicit_output
    }
}
