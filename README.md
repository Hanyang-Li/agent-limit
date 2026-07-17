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
- **Quiet and current.** Only the tab you're looking at is fetched, cached in
  memory, and refreshed on a timer — nothing is written to disk.

## Features

- **Provider tabs.** Claude and Kimi are auto-detected from local credentials.
  With both present you get a tab bar (Claude first); switch with `h`/`l` or the
  arrow keys. With only one, the tab bar is hidden.
- **Rounded, titled boxes.** Each provider's usage is framed in a `╭─ Claude ─╮`
  box labelled with its plan (e.g. `╭─ Claude · Max ─╮`, `╭─ Kimi · pro ─╮`).
- **Freshness header.** The top line shows the last-update time, how long ago
  that was, and the refresh interval — e.g. `Updated 22:41:07 (Asia/Shanghai) · 12s ago · every 5m`.
- **Pace-aware progress bars.** A `|` marker shows the expected burn for the
  elapsed portion of each window; the bar is green when you're on/under pace,
  yellow when a little ahead, red when well ahead, with an `↑`/`↓` delta.
- **The windows that matter.** Claude: current session (5h), current week (all
  models), and per-model weekly limits. Kimi: current session (5h), current
  week, and monthly quota.
- **Fetches only the active tab.** Switching tabs reuses cached data unless it's
  older than your refresh interval; nothing is persisted between runs.
- **Kimi token refresh, handled for you.** Kimi's short-lived access tokens are
  refreshed automatically (mirroring the `kimi` CLI) and written back with
  `0600` permissions.

## Requirements

- **macOS only** (reads the Claude Code keychain entry and the Kimi credentials
  file).
- At least one of:
  - **Claude Code** signed in — its OAuth credentials live in the login
    keychain. If you can run `claude`, you're set.
  - **Kimi** signed in — credentials at `~/.kimi-code/credentials/kimi-code.json`.
    If you can run `kimi`, you're set.

If neither is logged in, `agent-limit` tells you which CLI to run.

## Installation

### Download a release binary (Apple Silicon)

```sh
VERSION=v0.2.1
curl -fsSL -o agent-limit.tar.gz \
  "https://github.com/Hanyang-Li/agent-limit/releases/download/${VERSION}/agent-limit-${VERSION}-aarch64-apple-darwin.tar.gz"
tar -xzf agent-limit.tar.gz
sudo mv agent-limit /usr/local/bin/    # or anywhere on your PATH
```

Each release also publishes a `.sha256` you can verify with `shasum -a 256 -c`.

### With Cargo

```sh
cargo install --git https://github.com/Hanyang-Li/agent-limit --tag v0.2.1 --locked
```

### From source

```sh
git clone https://github.com/Hanyang-Li/agent-limit
cd agent-limit
cargo build --release
# binary at target/release/agent-limit
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

## How it works

- **Detection & tabs.** On start, `agent-limit` checks for Claude Code
  credentials (login keychain) and Kimi credentials
  (`~/.kimi-code/credentials/kimi-code.json`). Available providers become tabs
  in a fixed order — Claude, then Kimi.
- **Fetch only what you're viewing.** The active tab is fetched on start and
  auto-refreshed once its data is older than the interval (default 5 minutes).
  Switching to another tab reuses its in-memory data if it's still fresh, or
  fetches if it's stale or never loaded. Background tabs are never fetched, and
  nothing is written to disk.
- **The pace marker.** For a window that resets at a known time, the `|` sits at
  the fraction of the window that has elapsed — your "on-pace" position. Bar
  color compares your actual usage to that: green (on/under), yellow (up to ~20
  points ahead), red (further ahead).
- **Kimi tokens.** Kimi access tokens expire quickly. Before each request
  `agent-limit` checks freshness and, if needed, performs a `refresh_token`
  grant against `auth.kimi.com` — exactly as the `kimi` CLI does — then writes
  the rotated token back atomically with `0600` permissions. On a `401` it forces
  one refresh and retries. Nothing but the usage endpoint and the token endpoint
  is contacted.

## Development

```sh
cargo build --release
cargo test
```

Releases are built and published automatically by GitHub Actions when a `v*`
tag is pushed (`.github/workflows/release.yml`): it builds and tests the
`aarch64-apple-darwin` binary, tars it up with a checksum, and attaches both to
a generated GitHub release.

## License

[MIT](LICENSE) © Hanyang Li
