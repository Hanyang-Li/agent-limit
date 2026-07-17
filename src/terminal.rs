use crate::claude::{fetch_claude_provider_snapshot, is_claude_available};
use crate::kimi::{fetch_kimi_snapshot, is_kimi_available};
use crate::provider::{
    Provider, ProviderSnapshot, TabState, available_providers, initial_active_index,
};
use crate::render::{format_header, render_box, render_provider_body, render_tab_bar};
use anyhow::Result;
use chrono::Utc;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{self, Clear, ClearType, disable_raw_mode, enable_raw_mode};
use crossterm::{execute, queue};
use std::io::{Write, stdout};
use std::time::{Duration, Instant};

pub const REFRESH_COOLDOWN: Duration = Duration::from_secs(30);
const MIN_INNER_WIDTH: usize = 24;

struct RawModeGuard;

impl RawModeGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

fn fetch_provider(provider: Provider) -> Result<ProviderSnapshot> {
    match provider {
        Provider::Claude => fetch_claude_provider_snapshot(),
        Provider::Kimi => fetch_kimi_snapshot(),
    }
}

fn refresh_tab(provider: Provider) -> TabState {
    let fetched_at = Instant::now();
    match fetch_provider(provider) {
        Ok(snapshot) => TabState::Ready {
            snapshot,
            fetched_at,
        },
        Err(err) => TabState::Failed {
            message: err.to_string(),
            fetched_at,
        },
    }
}

pub fn run(interval_seconds: u64, requested: Provider) -> Result<()> {
    let providers = available_providers(is_claude_available(), is_kimi_available());
    let _raw = RawModeGuard::enter()?;
    let interval = Duration::from_secs(interval_seconds.max(1));
    let mut out = stdout();

    if providers.is_empty() {
        // Nothing authenticated: draw a static message, wait for quit.
        let body = format!("No authenticated providers found.\nRun `claude` or `kimi` to log in.",);
        draw_no_providers(&mut out, &body)?;
        wait_for_quit()?;
        execute!(out, Clear(ClearType::All), MoveTo(0, 0))?;
        return Ok(());
    }

    let mut active = initial_active_index(&providers, requested);
    let mut tabs: Vec<TabState> = providers.iter().map(|_| TabState::Empty).collect();

    // Startup: fetch active tab.
    tabs[active] = refresh_tab(providers[active]);
    draw(&mut out, &providers, active, &tabs, interval)?;

    let mut last_tick_secs = None;
    loop {
        // Auto-refresh active tab when its interval elapses.
        let elapsed = tab_elapsed(&tabs[active]);
        if should_fetch(elapsed, interval) && !matches!(tabs[active], TabState::Empty) {
            tabs[active] = refresh_tab(providers[active]);
            draw(&mut out, &providers, active, &tabs, interval)?;
        }

        // Re-draw ~1s for the "N s ago" and cooldown countdown.
        let tick = tab_secs_ago(&tabs[active]);
        if tick != last_tick_secs {
            draw(&mut out, &providers, active, &tabs, interval)?;
            last_tick_secs = tick;
        }

        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                KeyCode::Char('h') | KeyCode::Left => {
                    active = (active + providers.len() - 1) % providers.len();
                    maybe_fetch_on_switch(&mut tabs, active, providers[active], interval);
                    draw(&mut out, &providers, active, &tabs, interval)?;
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    active = (active + 1) % providers.len();
                    maybe_fetch_on_switch(&mut tabs, active, providers[active], interval);
                    draw(&mut out, &providers, active, &tabs, interval)?;
                }
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    // Manual refresh respects the 30s cooldown; if the tab has
                    // never fetched, allow immediately (no fetched_at to gate on).
                    let on_cooldown = tab_fetched_at(&tabs[active])
                        .and_then(|at| refresh_cooldown_remaining(Instant::now(), at))
                        .is_some();
                    if !on_cooldown {
                        tabs[active] = refresh_tab(providers[active]);
                    }
                    draw(&mut out, &providers, active, &tabs, interval)?;
                }
                _ => {}
            }
        }
    }

    execute!(out, Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}

fn tab_fetched_at(state: &TabState) -> Option<Instant> {
    match state {
        TabState::Empty => None,
        TabState::Ready { fetched_at, .. } | TabState::Failed { fetched_at, .. } => {
            Some(*fetched_at)
        }
    }
}

fn tab_elapsed(state: &TabState) -> Option<Duration> {
    tab_fetched_at(state).map(|at| Instant::now().saturating_duration_since(at))
}

fn tab_secs_ago(state: &TabState) -> Option<u64> {
    tab_elapsed(state).map(|d| d.as_secs())
}

fn maybe_fetch_on_switch(
    tabs: &mut [TabState],
    active: usize,
    provider: Provider,
    interval: Duration,
) {
    if should_fetch(tab_elapsed(&tabs[active]), interval) {
        tabs[active] = refresh_tab(provider);
    }
}

fn draw(
    out: &mut impl Write,
    providers: &[Provider],
    active: usize,
    tabs: &[TabState],
    interval: Duration,
) -> Result<()> {
    let terminal_width = terminal::size().map(|(w, _)| w).unwrap_or(80) as usize;
    let inner_width = terminal_width.saturating_sub(4).max(MIN_INNER_WIDTH);
    let now_ms = Utc::now().timestamp_millis();

    let (updated_at_ms, secs_ago) = match &tabs[active] {
        TabState::Empty => (None, 0),
        TabState::Ready { .. } | TabState::Failed { .. } => {
            let secs_ago = tab_secs_ago(&tabs[active]).unwrap_or(0);
            (Some(fetch_time_ms(now_ms, secs_ago)), secs_ago)
        }
    };

    let mut screen = String::new();
    screen.push_str(&format_header(updated_at_ms, secs_ago, interval.as_secs()));
    screen.push_str("\n\n");
    if let Some(bar) = render_tab_bar(providers, active) {
        screen.push_str(&bar);
        screen.push_str("\n\n");
    }

    let (title, body_lines) = match &tabs[active] {
        TabState::Empty => (providers[active].to_string(), vec!["Fetching…".to_string()]),
        TabState::Ready { snapshot, .. } => (
            format!("{} · {}", snapshot.provider, snapshot.plan),
            render_provider_body(&snapshot.metrics, now_ms, inner_width),
        ),
        TabState::Failed { message, .. } => (
            providers[active].to_string(),
            message.lines().map(|l| l.to_string()).collect(),
        ),
    };
    screen.push_str(&render_box(&title, &body_lines, inner_width));

    let cooldown =
        tab_fetched_at(&tabs[active]).and_then(|at| refresh_cooldown_remaining(Instant::now(), at));
    screen.push_str(&render_refresh_prompt(cooldown));

    queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;
    write!(out, "{}", normalize_raw_mode_newlines(&screen))?;
    out.flush()?;
    Ok(())
}

fn draw_no_providers(out: &mut impl Write, body: &str) -> Result<()> {
    queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;
    write!(out, "{}", normalize_raw_mode_newlines(body))?;
    write!(
        out,
        "{}",
        normalize_raw_mode_newlines(&render_refresh_prompt(None))
    )?;
    out.flush()?;
    Ok(())
}

fn wait_for_quit() -> Result<()> {
    loop {
        if event::poll(Duration::from_millis(250))?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    return Ok(());
                }
                _ => {}
            }
        }
    }
}

pub fn refresh_cooldown_remaining(now: Instant, refreshed_at: Instant) -> Option<Duration> {
    let elapsed = now.saturating_duration_since(refreshed_at);
    if elapsed >= REFRESH_COOLDOWN {
        None
    } else {
        Some(REFRESH_COOLDOWN - elapsed)
    }
}

pub fn render_refresh_prompt(cooldown: Option<Duration>) -> String {
    let refresh = match cooldown {
        Some(remaining) => format!(
            "\u{1b}[90m[R]efresh {}s\u{1b}[0m",
            cooldown_display_seconds(remaining)
        ),
        None => "\u{1b}[32m[R]efresh\u{1b}[0m".to_string(),
    };
    format!("\n{refresh}   [Q]uit\n")
}

pub fn should_fetch(last_fetch_elapsed: Option<Duration>, interval: Duration) -> bool {
    match last_fetch_elapsed {
        None => true,
        Some(elapsed) => elapsed >= interval,
    }
}

/// Wall-clock ms of the last fetch, derived from now and elapsed seconds,
/// so the header's "Updated <time>" stays consistent with "<N>s ago".
pub fn fetch_time_ms(now_ms: i64, secs_ago: u64) -> i64 {
    now_ms - (secs_ago as i64) * 1000
}

fn cooldown_display_seconds(remaining: Duration) -> u64 {
    let seconds = remaining.as_secs();
    if remaining.subsec_nanos() == 0 {
        seconds.max(1)
    } else {
        seconds + 1
    }
}

pub fn normalize_raw_mode_newlines(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                normalized.push('\r');
                if chars.peek() == Some(&'\n') {
                    normalized.push('\n');
                    chars.next();
                }
            }
            '\n' => normalized.push_str("\r\n"),
            _ => normalized.push(ch),
        }
    }

    normalized
}
