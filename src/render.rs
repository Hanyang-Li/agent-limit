use crate::claude::UsageMetric;
use crate::provider::Provider;
use chrono::Local;

const MIN_BAR_WIDTH: usize = 8;

const RESET: &str = "\u{1b}[0m";
const BOLD: &str = "\u{1b}[1m";
const NOT_BOLD: &str = "\u{1b}[22m";
const REVERSE: &str = "\u{1b}[7m";

// Hint colors, two tiers designed around the progress colors. Bright tier
// (hover) uses theme colors: 39m (the terminal's default foreground — the same
// white as the body text, which can be brighter than 97m) and the same 32m
// green as the progress text. Dim tier (normal) is truecolor at matched
// perceived brightness: a cool blue-leaning gray close to white (matching the
// bluish gray of the progress bar's ░ track), and a muted green shifted toward
// that gray and slightly darker than the hover green.
const WHITE: &str = "\u{1b}[39m";
const GRAY: &str = "\u{1b}[38;2;165;175;200m";
const DIM_GREEN: &str = "\u{1b}[38;2;140;195;140m";

fn fg_color((r, g, b): (u8, u8, u8)) -> String {
    format!("\u{1b}[38;2;{r};{g};{b}m")
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgressLine {
    pub bar: String,
    pub percentage_label: String,
    pub trend_label: String,
    pub color: ProgressColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressColor {
    Green,
    Yellow,
    Red,
    White,
}

pub fn calculate_trajectory_percent(period_seconds: u64, resets_at: &str, now_ms: i64) -> f64 {
    let Ok(resets_at) = chrono::DateTime::parse_from_rfc3339(resets_at) else {
        return 0.0;
    };

    let remaining_ms = resets_at.timestamp_millis() - now_ms;
    let period_ms = (period_seconds as i64).saturating_mul(1000);
    if period_ms <= 0 {
        return 0.0;
    }

    let elapsed_ms = period_ms.saturating_sub(remaining_ms);
    ((elapsed_ms as f64 / period_ms as f64) * 100.0).clamp(0.0, 100.0)
}

pub fn render_progress_line(metric: &UsageMetric, width: usize, now_ms: i64) -> ProgressLine {
    let width = width.max(1);
    let percentage = metric.percentage.clamp(0.0, 100.0);
    let filled_count = ((percentage / 100.0) * width as f64).round() as usize;

    let trajectory_percent = metric
        .resets_at
        .as_deref()
        .map(|resets_at| calculate_trajectory_percent(metric.period_seconds, resets_at, now_ms));
    let trajectory_pos =
        trajectory_percent.map(|percent| ((percent / 100.0) * width as f64).round() as usize);

    let mut bar = String::with_capacity(width);
    for i in 0..width {
        if trajectory_pos == Some(i) {
            bar.push('|');
        } else if i < filled_count {
            bar.push('█');
        } else {
            bar.push('░');
        }
    }

    let trend_label = trend_label(percentage, trajectory_percent);
    let color = progress_color(percentage, trajectory_percent);

    ProgressLine {
        bar,
        percentage_label: format!("{}% used", percentage.round() as i64),
        trend_label,
        color,
    }
}

fn progress_bar_width(metric: &UsageMetric, terminal_width: usize) -> usize {
    let percentage_label = format!(
        "{}% used",
        metric.percentage.clamp(0.0, 100.0).round() as i64
    );
    // Worst-case trend text is short, but reserve enough space to prevent wrapping.
    let metadata_width = 2 + percentage_label.chars().count() + 2 + "↑100%".chars().count();

    terminal_width
        .saturating_sub(metadata_width)
        .max(MIN_BAR_WIDTH)
}

fn format_progress_line(progress: &ProgressLine) -> String {
    format!(
        "{}  {}  {}\n",
        progress.bar, progress.percentage_label, progress.trend_label
    )
}

fn trend_label(percentage: f64, trajectory_percent: Option<f64>) -> String {
    match trajectory_percent {
        Some(trajectory) => {
            let delta = percentage - trajectory;
            let abs_delta = delta.abs().round() as i64;
            if delta < -1.0 {
                format!("↓{abs_delta}%")
            } else if delta > 1.0 {
                format!("↑{abs_delta}%")
            } else {
                "±0%".to_string()
            }
        }
        None => "±0%".to_string(),
    }
}

fn progress_color(percentage: f64, trajectory_percent: Option<f64>) -> ProgressColor {
    match trajectory_percent {
        Some(trajectory) => {
            let delta = percentage - trajectory;
            if delta <= 0.0 {
                ProgressColor::Green
            } else if delta <= 20.0 {
                ProgressColor::Yellow
            } else {
                ProgressColor::Red
            }
        }
        None => ProgressColor::White,
    }
}

fn colorize_progress_line(line: &str, color: ProgressColor) -> String {
    let start = match color {
        ProgressColor::Green => "\u{1b}[32m",
        ProgressColor::Yellow => "\u{1b}[33m",
        ProgressColor::Red => "\u{1b}[31m",
        ProgressColor::White => "",
    };

    if start.is_empty() {
        line.to_string()
    } else {
        format!("{start}{line}\u{1b}[0m")
    }
}

/// Humanize a second count into space-separated h/m/s units (largest unit is
/// hours), dropping zero components, e.g. 300 → "5m", 3661 → "1h 1m 1s".
pub fn format_frequency(seconds: u64) -> String {
    if seconds == 0 {
        return "0s".to_string();
    }
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    let mut parts = Vec::new();
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if secs > 0 {
        parts.push(format!("{secs}s"));
    }
    parts.join(" ")
}

pub fn format_header(updated_at_ms: Option<i64>, secs_ago: u64, interval_seconds: u64) -> String {
    let frequency = format_frequency(interval_seconds);
    match updated_at_ms {
        None => format!("Fetching… · every {frequency}"),
        Some(updated_at_ms) => {
            let time = format_clock(updated_at_ms);
            let ago = format_frequency(secs_ago);
            format!("Updated {time} · {ago} ago · every {frequency}")
        }
    }
}

fn format_clock(now_ms: i64) -> String {
    let Some(updated_at) = chrono::DateTime::from_timestamp_millis(now_ms) else {
        return "unknown".to_string();
    };
    let local = updated_at.with_timezone(&Local);
    let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "Local".to_string());
    format!("{} ({timezone})", local.format("%H:%M:%S"))
}

/// Column span [start, end) of a clickable tab within the tab bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabSpan {
    pub index: usize,
    pub start: u16,
    pub end: u16,
}

/// Clickable element currently under the mouse pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoverTarget {
    Tab(usize),
    Refresh,
    Quit,
}

/// Render the tab bar and report each tab's clickable column span. Returns
/// `None` (no bar) when only one provider is present. Inactive tabs are gray;
/// the hovered one turns white. The active tab keeps its brand-color block.
pub fn tab_bar_layout(
    providers: &[Provider],
    active: usize,
    hover: Option<HoverTarget>,
) -> Option<(String, Vec<TabSpan>)> {
    if providers.len() <= 1 {
        return None;
    }
    let mut bar = String::new();
    let mut spans = Vec::with_capacity(providers.len());
    let mut col: usize = 0;
    for (index, provider) in providers.iter().enumerate() {
        if index > 0 {
            bar.push_str("  ");
            col += 2;
        }
        let label = format!(" {provider} ");
        let width = label.chars().count();
        spans.push(TabSpan {
            index,
            start: col as u16,
            end: (col + width) as u16,
        });
        if index == active {
            // Selected tab: a solid brand-color block with knocked-out letters
            // (reverse-video on the brand color shows the terminal background
            // through the text).
            bar.push_str(&fg_color(provider.color()));
            bar.push_str(REVERSE);
            bar.push_str(&label);
            bar.push_str(RESET);
        } else if hover == Some(HoverTarget::Tab(index)) {
            bar.push_str(WHITE);
            bar.push_str(&label);
            bar.push_str(RESET);
        } else {
            bar.push_str(GRAY);
            bar.push_str(&label);
            bar.push_str(RESET);
        }
        col += width;
    }
    Some((bar, spans))
}

pub fn render_tab_bar(providers: &[Provider], active: usize) -> Option<String> {
    tab_bar_layout(providers, active, None).map(|(bar, _)| bar)
}

/// A right-aligned `[R]efresh   [Q]uit` footer plus the clickable column spans
/// of each hint (in the visible coordinate space of the rendered line).
#[derive(Debug, Clone, PartialEq)]
pub struct FooterLayout {
    pub line: String,
    pub refresh: (u16, u16),
    pub quit: (u16, u16),
}

/// Build the footer, right-aligned to `terminal_width`. `cooldown_secs` shows a
/// gray countdown when refresh is cooling down (no hover feedback: it is not
/// clickable); when ready, `[R]efresh` is muted green and turns the progress
/// text's green on hover. `[Q]uit` is gray and turns white on hover.
pub fn render_footer(
    terminal_width: usize,
    cooldown_secs: Option<u64>,
    hover: Option<HoverTarget>,
) -> FooterLayout {
    let refresh_label = match cooldown_secs {
        Some(seconds) => format!("[R]efresh {seconds}s"),
        None => "[R]efresh".to_string(),
    };
    let quit_label = "[Q]uit";
    let gap = 3usize;

    let refresh_len = refresh_label.chars().count();
    let quit_len = quit_label.chars().count();
    let visible = refresh_len + gap + quit_len;
    let pad = terminal_width.saturating_sub(visible);

    let refresh = (pad as u16, (pad + refresh_len) as u16);
    let quit_start = pad + refresh_len + gap;
    let quit = (quit_start as u16, (quit_start + quit_len) as u16);

    let refresh_colored = match cooldown_secs {
        Some(_) => format!("{GRAY}{refresh_label}{RESET}"),
        None if hover == Some(HoverTarget::Refresh) => {
            format!("\u{1b}[32m{refresh_label}{RESET}")
        }
        None => format!("{DIM_GREEN}{refresh_label}{RESET}"),
    };
    let quit_colored = if hover == Some(HoverTarget::Quit) {
        format!("{WHITE}{quit_label}{RESET}")
    } else {
        format!("{GRAY}{quit_label}{RESET}")
    };
    let line = format!(
        "{}{}{}{}",
        " ".repeat(pad),
        refresh_colored,
        " ".repeat(gap),
        quit_colored
    );

    FooterLayout {
        line,
        refresh,
        quit,
    }
}

pub fn visible_width(text: &str) -> usize {
    let mut width = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for code in chars.by_ref() {
                if code == 'm' {
                    break;
                }
            }
        } else {
            width += 1;
        }
    }
    width
}

/// Truncate `text` to at most `max` visible columns (ANSI escape sequences are
/// copied through without counting), appending an ellipsis + reset when cut.
fn truncate_visible(text: &str, max: usize) -> String {
    if visible_width(text) <= max {
        return text.to_string();
    }
    let keep = max.saturating_sub(1); // room for the ellipsis
    let mut out = String::new();
    let mut count = 0usize;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            out.push(ch);
            if let Some(bracket) = chars.next() {
                out.push(bracket);
            }
            for code in chars.by_ref() {
                out.push(code);
                if code == 'm' {
                    break;
                }
            }
            continue;
        }
        if count >= keep {
            break;
        }
        out.push(ch);
        count += 1;
    }
    out.push('…');
    out.push_str("\u{1b}[0m"); // avoid color bleed if we cut mid-sequence
    out
}

/// Frame `body_lines` in a rounded box whose border and title are drawn in the
/// given brand `color` (title additionally bold). Every rendered line has equal
/// visible width (`inner_width + 4`); the title is truncated to fit.
pub fn render_box(
    title: &str,
    body_lines: &[String],
    inner_width: usize,
    color: (u8, u8, u8),
) -> String {
    let c = fg_color(color);
    // Truncate the title so the top border matches the body/bottom width.
    // Reserve one column for the space between the title and the dash run.
    let max_title = inner_width.saturating_sub(1);
    let title: String = if title.chars().count() > max_title {
        let keep = max_title.saturating_sub(1);
        format!("{}…", title.chars().take(keep).collect::<String>())
    } else {
        title.to_string()
    };
    let title_len = title.chars().count();
    // total visible width = inner_width + 4 ("╭─ " + title + " " + dashes + "╮")
    let dashes = inner_width.saturating_sub(title_len + 1);
    let mut out = String::new();

    // Top border: colored line, bold title.
    out.push_str(&c);
    out.push_str("╭─ ");
    out.push_str(BOLD);
    out.push_str(&title);
    out.push_str(NOT_BOLD);
    out.push(' ');
    out.push_str(&"─".repeat(dashes));
    out.push('╮');
    out.push_str(RESET);
    out.push('\n');

    // Body: colored side borders, content keeps its own colors.
    for line in body_lines {
        let line = truncate_visible(line, inner_width);
        let pad = inner_width.saturating_sub(visible_width(&line));
        out.push_str(&c);
        out.push('│');
        out.push_str(RESET);
        out.push(' ');
        out.push_str(&line);
        out.push_str(&" ".repeat(pad));
        out.push(' ');
        out.push_str(&c);
        out.push('│');
        out.push_str(RESET);
        out.push('\n');
    }

    // Bottom border.
    out.push_str(&c);
    out.push('╰');
    out.push_str(&"─".repeat(inner_width + 2));
    out.push('╯');
    out.push_str(RESET);
    out.push('\n');
    out
}

pub fn render_provider_body(
    metrics: &[UsageMetric],
    now_ms: i64,
    inner_width: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, metric) in metrics.iter().enumerate() {
        let bar_width = progress_bar_width(metric, inner_width);
        let progress = render_progress_line(metric, bar_width, now_ms);
        lines.push(metric.title.clone());
        lines.push(colorize_progress_line(
            format_progress_line(&progress).trim_end_matches('\n'),
            progress.color,
        ));
        lines.push(format!("Resets {}", metric.resets_in));
        if index + 1 < metrics.len() {
            lines.push(String::new());
        }
    }
    lines
}
