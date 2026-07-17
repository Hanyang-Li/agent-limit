fn main() -> anyhow::Result<()> {
    let options = agent_limit::args::parse_options();
    agent_limit::terminal::run(options.interval, options.provider)
}
