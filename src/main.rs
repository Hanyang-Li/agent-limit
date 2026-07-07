fn main() -> anyhow::Result<()> {
    let interval = agent_limit::args::parse_interval_seconds();
    agent_limit::terminal::run(interval)
}
