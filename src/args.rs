use clap::Parser;

pub const DEFAULT_INTERVAL_SECONDS: u64 = 60;

#[derive(Debug, Parser)]
#[command(name = "agent-limit")]
#[command(about = "Show Claude Code usage limits from local OAuth credentials")]
#[command(version)]
pub struct Cli {
    #[arg(
        short = 'i',
        long = "interval",
        default_value_t = DEFAULT_INTERVAL_SECONDS,
        value_parser = clap::value_parser!(u64).range(DEFAULT_INTERVAL_SECONDS..)
    )]
    pub interval: u64,
}

pub fn parse_interval_seconds_from<I, T>(_itr: I) -> Result<u64, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    Cli::try_parse_from(_itr).map(|cli| cli.interval)
}

pub fn parse_interval_seconds() -> u64 {
    Cli::parse().interval
}
