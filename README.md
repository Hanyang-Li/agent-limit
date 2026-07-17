# agent-limit 📊

> Live Claude Code & Kimi usage limits in your terminal — read straight from your local credentials.

**English** | [简体中文](README.zh-CN.md)

![license](https://img.shields.io/badge/license-MIT-blue.svg)
![platform](https://img.shields.io/badge/platform-macOS-lightgrey.svg)

`agent-limit` is a small terminal UI that shows how much of your Claude Code and
Kimi coding-plan usage you've burned through — session, weekly, and monthly
windows — with a live progress bar, a pace marker, and a color that tells you at
a glance whether you're ahead of or behind your allowance. It reads the
credentials the `claude` and `kimi` CLIs already store on your machine; there is
nothing to log into and no API key to paste.

- **Both providers, auto-detected.** Whichever of Claude Code / Kimi you're
  logged into shows up. Logged into both? They become switchable tabs.
- **Pace, not just percent.** Each bar has a `|` marker at where you *should* be
  for this point in the window, and turns green / yellow / red as you fall
  behind or race ahead of that pace.
- **Quiet and current.** Only the tab you're looking at is fetched and refreshed
  on a timer — nothing is written to disk.

## Requirements

- **macOS only** (reads the Claude Code keychain entry and the Kimi credentials
  file).
- At least one of:
  - **Claude Code** signed in — if you can run `claude`, you're set.
  - **Kimi** signed in — if you can run `kimi`, you're set.

If neither is logged in, `agent-limit` tells you which CLI to run.

## Installation

Install the latest release (macOS, Apple Silicon) with one command:

```sh
curl -fsSL https://raw.githubusercontent.com/Hanyang-Li/agent-limit/main/install.sh | sh
```

It downloads the release binary, verifies its checksum, and installs it to
`/usr/local/bin` (may prompt for `sudo`). Override with `AGENT_LIMIT_VERSION`
or `AGENT_LIMIT_INSTALL_DIR`.

Or install with Cargo:

```sh
cargo install --git https://github.com/Hanyang-Li/agent-limit --tag v0.2.2 --locked
```

## Usage

```sh
agent-limit
```

```
Options:
  -i, --interval <SECONDS>   Refresh interval, seconds (default: 300, min: 60)
  -p, --provider <PROVIDER>  Tab to open first: claude | kimi (default: claude)
  -h, --help                 Print help
  -V, --version              Print version
```

Open on the Kimi tab, refreshing every two minutes:

```sh
agent-limit -p kimi -i 120
```

If the requested provider isn't authenticated, `agent-limit` falls back to the
first available one; the tab order stays Claude, then Kimi.

### Keys

| Key | Action |
| --- | --- |
| `h` / `←`, `l` / `→` | Switch tab (when both providers are present) |
| `R` | Refresh the active tab now (honors a short cooldown) |
| `Q` | Quit (`Esc` and `Ctrl+C` also quit) |

## Development

```sh
git clone https://github.com/Hanyang-Li/agent-limit
cd agent-limit
cargo build --release   # binary at target/release/agent-limit
cargo test
```

Releases are built and published automatically by GitHub Actions when a `v*`
tag is pushed (`.github/workflows/release.yml`): it builds and tests the
`aarch64-apple-darwin` binary, tars it up with a checksum, and attaches both to
a generated GitHub release.

## License

[MIT](LICENSE) © Hanyang Li
