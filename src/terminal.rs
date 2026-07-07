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
    let mut out = stdout();

    loop {
        let now = Instant::now();
        if now >= next_refresh {
            draw_once(&mut out)?;
            next_refresh = Instant::now() + interval;
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
                    draw_once(&mut out)?;
                    next_refresh = Instant::now() + interval;
                }
                _ => {}
            }
        }
    }

    execute!(out, Clear(ClearType::All), MoveTo(0, 0))?;
    Ok(())
}

fn draw_once(out: &mut impl Write) -> Result<()> {
    queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;

    match fetch_claude_snapshot() {
        Ok(snapshot) => {
            let now_ms = Utc::now().timestamp_millis();
            let terminal_width = terminal::size().map(|(width, _)| width).unwrap_or(80);
            write_raw_mode_text(
                out,
                &render_snapshot(&snapshot.plan, &snapshot.metrics, now_ms, terminal_width),
            )?;
        }
        Err(err) => {
            write_raw_mode_text(out, &format!("Claude\n\n{}\n", err))?;
        }
    }

    write_raw_mode_text(out, "\n[r] refresh   [q/Esc/Ctrl+C] quit\n")?;
    out.flush()?;
    Ok(())
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
