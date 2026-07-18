use crate::claude::{fetch_claude_provider_snapshot, is_claude_available};
use crate::kimi::{fetch_kimi_snapshot, is_kimi_available};
use crate::provider::{
    Provider, ProviderSnapshot, TabState, available_providers, initial_active_index,
};
use crate::render::{
    TabSpan, format_header, render_box, render_footer, render_provider_body, tab_bar_layout,
};
use anyhow::Result;
use chrono::Utc;
use crossterm::cursor::MoveTo;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton,
    MouseEventKind,
};
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
        execute!(stdout(), EnableMouseCapture)?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = execute!(stdout(), DisableMouseCapture);
        let _ = disable_raw_mode();
    }
}

/// Clickable regions of the current frame, in terminal cell coordinates.
#[derive(Default)]
struct ClickMap {
    has_tabs: bool,
    tab_row: u16,
    tabs: Vec<TabSpan>,
    has_footer: bool,
    footer_row: u16,
    refresh: (u16, u16),
    quit: (u16, u16),
}

impl ClickMap {
    fn tab_at(&self, col: u16, row: u16) -> Option<usize> {
        if !self.has_tabs || row != self.tab_row {
            return None;
        }
        self.tabs
            .iter()
            .find(|span| col >= span.start && col < span.end)
            .map(|span| span.index)
    }

    fn is_refresh(&self, col: u16, row: u16) -> bool {
        self.has_footer && row == self.footer_row && col >= self.refresh.0 && col < self.refresh.1
    }

    fn is_quit(&self, col: u16, row: u16) -> bool {
        self.has_footer && row == self.footer_row && col >= self.quit.0 && col < self.quit.1
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
    let mut click = draw(&mut out, &providers, active, &tabs, interval)?;

    let mut last_tick_secs = None;
    loop {
        // Auto-refresh active tab when its interval elapses.
        let elapsed = tab_elapsed(&tabs[active]);
        if should_fetch(elapsed, interval) && !matches!(tabs[active], TabState::Empty) {
            tabs[active] = refresh_tab(providers[active]);
            click = draw(&mut out, &providers, active, &tabs, interval)?;
        }

        // Re-draw ~1s for the "N s ago" and cooldown countdown.
        let tick = tab_secs_ago(&tabs[active]);
        if tick != last_tick_secs {
            click = draw(&mut out, &providers, active, &tabs, interval)?;
            last_tick_secs = tick;
        }

        if event::poll(Duration::from_millis(250))? {
            let mut dirty = false;
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Char('h') | KeyCode::Left => {
                        let next = (active + providers.len() - 1) % providers.len();
                        switch_tab(&mut tabs, &mut active, &providers, next, interval);
                        dirty = true;
                    }
                    KeyCode::Char('l') | KeyCode::Right => {
                        let next = (active + 1) % providers.len();
                        switch_tab(&mut tabs, &mut active, &providers, next, interval);
                        dirty = true;
                    }
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        manual_refresh(&mut tabs, active, &providers);
                        dirty = true;
                    }
                    _ => {}
                },
                Event::Mouse(mouse) => {
                    if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                        let (col, row) = (mouse.column, mouse.row);
                        if click.is_quit(col, row) {
                            break;
                        } else if click.is_refresh(col, row) {
                            manual_refresh(&mut tabs, active, &providers);
                            dirty = true;
                        } else if let Some(index) = click.tab_at(col, row) {
                            if index != active {
                                switch_tab(&mut tabs, &mut active, &providers, index, interval);
                            }
                            dirty = true;
                        }
                    }
                }
                _ => {}
            }

            if dirty {
                click = draw(&mut out, &providers, active, &tabs, interval)?;
                last_tick_secs = tab_secs_ago(&tabs[active]);
            }
        }
    }

    execute!(out, Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}

fn switch_tab(
    tabs: &mut [TabState],
    active: &mut usize,
    providers: &[Provider],
    next: usize,
    interval: Duration,
) {
    *active = next;
    maybe_fetch_on_switch(tabs, *active, providers[*active], interval);
}

/// Manual refresh respects the 30s cooldown; if the tab has never fetched, it
/// refreshes immediately (no fetched_at to gate on).
fn manual_refresh(tabs: &mut [TabState], active: usize, providers: &[Provider]) {
    let on_cooldown = tab_fetched_at(&tabs[active])
        .and_then(|at| refresh_cooldown_remaining(Instant::now(), at))
        .is_some();
    if !on_cooldown {
        tabs[active] = refresh_tab(providers[active]);
    }
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
) -> Result<ClickMap> {
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

    let mut lines: Vec<String> = Vec::new();
    let mut click = ClickMap::default();

    lines.push(format_header(updated_at_ms, secs_ago, interval.as_secs()));
    lines.push(String::new());

    if let Some((bar, spans)) = tab_bar_layout(providers, active) {
        click.has_tabs = true;
        click.tab_row = lines.len() as u16;
        click.tabs = spans;
        lines.push(bar);
        lines.push(String::new());
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
    let boxed = render_box(&title, &body_lines, inner_width, providers[active].color());
    for line in boxed.lines() {
        lines.push(line.to_string());
    }

    // Footer: a blank spacer row, then the right-aligned hints.
    lines.push(String::new());
    let cooldown =
        tab_fetched_at(&tabs[active]).and_then(|at| refresh_cooldown_remaining(Instant::now(), at));
    let footer = render_footer(terminal_width, cooldown.map(cooldown_display_seconds));
    click.has_footer = true;
    click.footer_row = lines.len() as u16;
    click.refresh = footer.refresh;
    click.quit = footer.quit;
    lines.push(footer.line);

    let screen = lines.join("\n");
    queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;
    write!(out, "{}", normalize_raw_mode_newlines(&screen))?;
    out.flush()?;
    Ok(click)
}

fn draw_no_providers(out: &mut impl Write, body: &str) -> Result<()> {
    let terminal_width = terminal::size().map(|(w, _)| w).unwrap_or(80) as usize;
    let footer = render_footer(terminal_width, None);
    let screen = format!("{body}\n{}", footer.line);
    queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;
    write!(out, "{}", normalize_raw_mode_newlines(&screen))?;
    out.flush()?;
    Ok(())
}

fn wait_for_quit() -> Result<()> {
    loop {
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    _ => {}
                },
                // The only action on the no-providers screen is to quit, so any
                // left click dismisses it.
                Event::Mouse(mouse)
                    if matches!(mouse.kind, MouseEventKind::Down(MouseButton::Left)) =>
                {
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
