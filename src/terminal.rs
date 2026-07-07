use crate::claude::fetch_claude_snapshot;
use crate::render::render_snapshot;
use anyhow::Result;
use chrono::Utc;
use crossterm::cursor::MoveTo;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{self, Clear, ClearType, disable_raw_mode, enable_raw_mode};
use crossterm::{execute, queue};
use std::io::{Write, stdout};
use std::time::{Duration, Instant};

pub const REFRESH_COOLDOWN: Duration = Duration::from_secs(30);

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

pub fn run(interval_seconds: u64) -> Result<()> {
    let _raw = RawModeGuard::enter()?;
    let interval = Duration::from_secs(interval_seconds.max(1));
    let mut next_refresh = Instant::now();
    let mut last_usage_refresh = None;
    let mut body = String::new();
    let mut last_prompt_seconds = None;
    let mut out = stdout();

    loop {
        let now = Instant::now();
        let cooldown = last_usage_refresh.and_then(|last| refresh_cooldown_remaining(now, last));

        if now >= next_refresh {
            if cooldown.is_none() {
                body = fetch_terminal_body();
                let refreshed_at = Instant::now();
                last_usage_refresh = Some(refreshed_at);
                next_refresh = refreshed_at + interval;

                let cooldown = refresh_cooldown_remaining(Instant::now(), refreshed_at);
                draw_screen(&mut out, &body, cooldown)?;
                last_prompt_seconds = cooldown.map(cooldown_display_seconds);
            } else if let Some(remaining) = cooldown {
                next_refresh = now + remaining;
            }
        }

        let cooldown =
            last_usage_refresh.and_then(|last| refresh_cooldown_remaining(Instant::now(), last));
        let prompt_seconds = cooldown.map(cooldown_display_seconds);
        if !body.is_empty() && prompt_seconds != last_prompt_seconds {
            draw_screen(&mut out, &body, cooldown)?;
            last_prompt_seconds = prompt_seconds;
        }

        let timeout = next_refresh
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(250));

        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                KeyCode::Char('r') => {
                    let now = Instant::now();
                    let cooldown =
                        last_usage_refresh.and_then(|last| refresh_cooldown_remaining(now, last));

                    if cooldown.is_none() {
                        body = fetch_terminal_body();
                        let refreshed_at = Instant::now();
                        last_usage_refresh = Some(refreshed_at);
                        next_refresh = refreshed_at + interval;

                        let cooldown = refresh_cooldown_remaining(Instant::now(), refreshed_at);
                        draw_screen(&mut out, &body, cooldown)?;
                        last_prompt_seconds = cooldown.map(cooldown_display_seconds);
                    } else {
                        draw_screen(&mut out, &body, cooldown)?;
                        last_prompt_seconds = cooldown.map(cooldown_display_seconds);
                    }
                }
                _ => {}
            }
        }
    }

    execute!(out, Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}

fn fetch_terminal_body() -> String {
    match fetch_claude_snapshot() {
        Ok(snapshot) => {
            let now_ms = Utc::now().timestamp_millis();
            let terminal_width = terminal::size().map(|(width, _)| width).unwrap_or(80);
            render_snapshot(&snapshot.plan, &snapshot.metrics, now_ms, terminal_width)
        }
        Err(err) => format!("Claude\n\n{}\n", err),
    }
}

fn draw_screen(out: &mut impl Write, body: &str, cooldown: Option<Duration>) -> Result<()> {
    queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;
    write_raw_mode_text(out, body)?;
    write_raw_mode_text(out, &render_refresh_prompt(cooldown))?;
    out.flush()?;
    Ok(())
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
            "\u{1b}[90m[r] refresh {}s\u{1b}[0m",
            cooldown_display_seconds(remaining)
        ),
        None => "\u{1b}[32m[r] refresh\u{1b}[0m".to_string(),
    };

    format!("\n{}   [q/Esc/Ctrl+C] quit\n", refresh)
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

fn write_raw_mode_text(out: &mut impl Write, text: &str) -> Result<()> {
    write!(out, "{}", normalize_raw_mode_newlines(text))?;
    Ok(())
}
