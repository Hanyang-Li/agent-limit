use crate::claude::UsageMetric;
use chrono::Local;

const FABLE_TITLE: &str = "Current week (Fable)";
const MIN_BAR_WIDTH: usize = 8;

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
    let is_fable = metric.title == FABLE_TITLE;

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

    let trend_label = if is_fable {
        String::new()
    } else {
        trend_label(percentage, trajectory_percent)
    };
    let color = progress_color(is_fable, percentage, trajectory_percent);

    ProgressLine {
        bar,
        percentage_label: format!("{}% used", percentage.round() as i64),
        trend_label,
        color,
    }
}

pub fn render_snapshot(
    plan: &str,
    metrics: &[UsageMetric],
    now_ms: i64,
    terminal_width: u16,
) -> String {
    let terminal_width = usize::from(terminal_width).max(MIN_BAR_WIDTH);
    let mut output = String::new();
    output.push_str(&format!("Last updated: {}\n", format_last_updated(now_ms)));
    output.push_str(&format!("Claude {plan}\n\n"));

    let visible_metrics: Vec<_> = metrics
        .iter()
        .filter(|metric| metric.title != FABLE_TITLE)
        .collect();

    for (index, metric) in visible_metrics.iter().enumerate() {
        let bar_width = progress_bar_width(metric, terminal_width);
        let progress = render_progress_line(metric, bar_width, now_ms);

        output.push_str(&metric.title);
        output.push('\n');
        output.push_str(&colorize_progress_line(
            &format_progress_line(&progress),
            progress.color,
        ));

        output.push_str(&format!("Resets {}\n", metric.resets_in));

        if index + 1 < visible_metrics.len() {
            output.push('\n');
        }
    }

    output
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

fn progress_color(
    is_fable: bool,
    percentage: f64,
    trajectory_percent: Option<f64>,
) -> ProgressColor {
    if is_fable {
        return ProgressColor::White;
    }

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

fn format_last_updated(now_ms: i64) -> String {
    let Some(updated_at) = chrono::DateTime::from_timestamp_millis(now_ms) else {
        return "unknown".to_string();
    };

    let local = updated_at.with_timezone(&Local);
    let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "Local".to_string());
    format!("{} ({timezone})", local.format("%-I:%M%P"))
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
