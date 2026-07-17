use crate::provider::Provider;
use clap::Parser;

pub const DEFAULT_INTERVAL_SECONDS: u64 = 300;
pub const MIN_INTERVAL_SECONDS: u64 = 60;

#[derive(Debug, Parser)]
#[command(name = "agent-limit")]
#[command(about = "Show Claude and Kimi usage limits from local credentials")]
#[command(version)]
pub struct Cli {
    #[arg(
        short = 'i',
        long = "interval",
        default_value_t = DEFAULT_INTERVAL_SECONDS,
        value_parser = clap::value_parser!(u64).range(MIN_INTERVAL_SECONDS..)
    )]
    pub interval: u64,

    #[arg(
        short = 'p',
        long = "provider",
        default_value = "claude",
        value_parser = parse_provider
    )]
    pub provider: Provider,
}

fn parse_provider(value: &str) -> Result<Provider, String> {
    value.parse::<Provider>()
}

#[derive(Debug)]
pub struct CliOptions {
    pub interval: u64,
    pub provider: Provider,
}

pub fn parse_options_from<I, T>(itr: I) -> Result<CliOptions, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    Cli::try_parse_from(itr).map(|cli| CliOptions {
        interval: cli.interval,
        provider: cli.provider,
    })
}

pub fn parse_options() -> CliOptions {
    let cli = Cli::parse();
    CliOptions {
        interval: cli.interval,
        provider: cli.provider,
    }
}

pub fn parse_interval_seconds_from<I, T>(itr: I) -> Result<u64, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    parse_options_from(itr).map(|options| options.interval)
}
