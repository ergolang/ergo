//! Application runtime options.

use grease::runtime::LogLevel;
pub use structopt::StructOpt;

#[derive(Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Basic,
    Pretty,
    Auto,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(OutputFormat::Auto),
            "basic" => Ok(OutputFormat::Basic),
            "pretty" => Ok(OutputFormat::Pretty),
            _ => Err(format!("invalid output format '{}'", s)),
        }
    }
}

#[derive(Debug, StructOpt)]
/// Effio ergo sum.
///
/// Ergo is a runtime and language built for lazy task execution.
pub struct Opts {
    #[structopt(long = "log", default_value = "warn")]
    /// The log level used for tasks. May be "debug", "info", "warn", or "error".
    pub log_level: LogLevel,

    #[structopt(long, default_value = "auto")]
    /// The output format. May be "auto", "basic", or "pretty".
    pub format: OutputFormat,

    #[structopt(short, long)]
    /// The maximum number of jobs to run concurrently. If unspecified, the number of cpus is used.
    pub jobs: Option<usize>,

    #[structopt(long, default_value = concat!(".", env!("CARGO_PKG_NAME"), "_work"))]
    /// The storage directory for the runtime. If a relative path, it will be made relative to the
    /// furthest ancestor directory that is a workspace. If none are found, the current directory
    /// is used.
    pub storage: std::path::PathBuf,

    #[structopt(short, long)]
    /// Clear the storage directory prior to executing.
    pub clean: bool,

    #[structopt(short, long)]
    /// Check for common syntax mistakes without executing the final value.
    pub lint: bool,

    #[structopt(short, long)]
    /// Display documentation for the final value rather than executing it.
    pub doc: bool,

    #[structopt(short = "S", long)]
    /// Whether to stop immediately when an error occurs.
    pub stop: bool,

    /// Arguments for loading the value(s) to run.
    /// All additional arguments are run as if "ergo <args>..." were executed in a script.
    pub args: Vec<String>,
}
