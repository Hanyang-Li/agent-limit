use agent_limit::args::{DEFAULT_INTERVAL_SECONDS, parse_interval_seconds_from};
use agent_limit::claude::{ClaudeUsageResponse, UsageWindow, map_usage_response};
use agent_limit::render::{
    ProgressColor, calculate_trajectory_percent, render_progress_line, render_snapshot,
};
use agent_limit::terminal::{
    REFRESH_COOLDOWN, normalize_raw_mode_newlines, refresh_cooldown_remaining,
    render_refresh_prompt,
};
use std::time::{Duration, Instant};

#[test]
fn interval_defaults_to_sixty_seconds() {
    let seconds = parse_interval_seconds_from(["agent-limit"]).expect("valid args");
    assert_eq!(seconds, DEFAULT_INTERVAL_SECONDS);
}

#[test]
fn interval_accepts_short_and_long_flags() {
    let short = parse_interval_seconds_from(["agent-limit", "-i", "60"]).expect("valid args");
    let long =
        parse_interval_seconds_from(["agent-limit", "--interval", "120"]).expect("valid args");
    assert_eq!(short, 60);
    assert_eq!(long, 120);
}

#[test]
fn interval_rejects_values_below_sixty_seconds() {
    let error = parse_interval_seconds_from(["agent-limit", "-i", "59"]).expect_err("invalid args");
    assert!(
        error.to_string().contains("not in 60.."),
        "unexpected error: {error}"
    );
}

#[test]
fn version_flags_print_package_version() {
    for flag in ["-V", "--version"] {
        let error =
            parse_interval_seconds_from(["agent-limit", flag]).expect_err("version exits early");
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayVersion);
        assert!(
            error.to_string().contains(env!("CARGO_PKG_VERSION")),
            "unexpected version output for {flag}: {error}"
        );
    }

    for flag in ["-v", "--verson"] {
        let error = parse_interval_seconds_from(["agent-limit", flag])
            .expect_err("unsupported version alias");
        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }
}

#[test]
fn usage_response_maps_base_claude_windows() {
    let metrics = map_usage_response(ClaudeUsageResponse {
        five_hour: Some(UsageWindow {
            utilization: 35.2,
            resets_at: Some("2026-07-07T14:19:00+08:00".to_string()),
        }),
        seven_day: Some(UsageWindow {
            utilization: 5.0,
            resets_at: Some("2026-07-10T18:59:00+08:00".to_string()),
        }),
        seven_day_opus: Some(UsageWindow {
            utilization: 0.0,
            resets_at: None,
        }),
        model_scoped: Vec::new(),
        limits: Vec::new(),
    });

    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0].title, "Current session");
    assert_eq!(metrics[0].percentage, 35.2);
    assert_eq!(metrics[0].period_seconds, 5 * 3600);
    assert_eq!(metrics[1].title, "Current week (all models)");
    assert_eq!(metrics[1].period_seconds, 7 * 24 * 3600);
}

#[test]
fn usage_response_renders_base_windows_when_model_scoped_is_absent() {
    let metrics = map_usage_response(ClaudeUsageResponse {
        five_hour: None,
        seven_day: None,
        seven_day_opus: None,
        model_scoped: Vec::new(),
        limits: Vec::new(),
    });

    assert_eq!(metrics.len(), 2);
    assert_eq!(metrics[0].title, "Current session");
    assert_eq!(metrics[0].percentage, 0.0);
    assert_eq!(metrics[0].resets_in, "unknown");
    assert_eq!(metrics[1].title, "Current week (all models)");
    assert_eq!(metrics[1].percentage, 0.0);
}

#[test]
fn usage_response_maps_all_model_scoped_windows() {
    let response: ClaudeUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": {
            "utilization": 35.0,
            "resets_at": "2026-07-07T14:19:00+08:00"
        },
        "seven_day": {
            "utilization": 5.0,
            "resets_at": "2026-07-10T18:59:00+08:00"
        },
        "model_scoped": [
            {
                "display_name": "Fable",
                "utilization": 12.0,
                "resets_at": "2026-07-10T18:59:00+08:00"
            },
            {
                "display_name": "Opus",
                "utilization": 24.0,
                "resets_at": "2026-07-11T18:59:00+08:00"
            }
        ]
    }))
    .expect("valid usage response");

    let metrics = map_usage_response(response);

    assert_eq!(metrics.len(), 4);
    assert_eq!(metrics[2].title, "Current week (Fable)");
    assert_eq!(metrics[2].percentage, 12.0);
    assert_eq!(metrics[2].period_seconds, 7 * 24 * 3600);
    assert_eq!(metrics[3].title, "Current week (Opus)");
    assert_eq!(metrics[3].percentage, 24.0);
    assert_eq!(metrics[3].period_seconds, 7 * 24 * 3600);
}

#[test]
fn usage_response_maps_weekly_scoped_limits() {
    let response: ClaudeUsageResponse = serde_json::from_value(serde_json::json!({
        "five_hour": {
            "utilization": 35.0,
            "resets_at": "2026-07-07T14:19:00+08:00"
        },
        "seven_day": {
            "utilization": 5.0,
            "resets_at": "2026-07-10T18:59:00+08:00"
        },
        "limits": [
            {
                "kind": "weekly_scoped",
                "scope": {
                    "model": {
                        "display_name": "Fable"
                    }
                },
                "percent": 12.0,
                "resets_at": 1783677540
            },
            {
                "kind": "weekly_scoped",
                "scope": {
                    "model": {
                        "display_name": "Opus"
                    }
                },
                "percent": 24.0,
                "resets_at": "2026-07-11T18:59:00+08:00"
            }
        ]
    }))
    .expect("valid usage response");

    let metrics = map_usage_response(response);

    assert_eq!(metrics.len(), 4);
    assert_eq!(metrics[2].title, "Current week (Fable)");
    assert_eq!(metrics[2].percentage, 12.0);
    assert!(metrics[2].resets_at.is_some());
    assert_eq!(metrics[3].title, "Current week (Opus)");
    assert_eq!(metrics[3].percentage, 24.0);
    assert_eq!(
        metrics[3].resets_at.as_deref(),
        Some("2026-07-11T18:59:00+08:00")
    );
}

#[test]
fn trajectory_uses_elapsed_share_of_window() {
    let now_ms = 1_000_000;
    let resets_at_ms = now_ms + 3_600_000;
    let resets_at = chrono::DateTime::from_timestamp_millis(resets_at_ms)
        .unwrap()
        .to_rfc3339();

    let trajectory = calculate_trajectory_percent(2 * 3600, &resets_at, now_ms);
    assert_eq!(trajectory.round(), 50.0);
}

#[test]
fn progress_line_matches_reference_bar_and_trend_style() {
    let metric = agent_limit::claude::UsageMetric {
        title: "Current session".to_string(),
        percentage: 35.0,
        resets_at: Some(
            chrono::DateTime::from_timestamp_millis(1_000_000 + 3_600_000)
                .unwrap()
                .to_rfc3339(),
        ),
        resets_in: "in 1h".to_string(),
        period_seconds: 2 * 3600,
    };

    let line = render_progress_line(&metric, 12, 1_000_000);
    assert_eq!(line.bar, "████░░|░░░░░");
    assert_eq!(line.percentage_label, "35% used");
    assert_eq!(line.trend_label, "↓15%");
    assert_eq!(line.color, ProgressColor::Green);
}

#[test]
fn progress_line_is_yellow_when_usage_is_up_to_twenty_points_above_speed() {
    let metric = usage_metric(
        "Current session",
        65.0,
        Some(1_000_000 + 3_600_000),
        2 * 3600,
    );
    let line = render_progress_line(&metric, 12, 1_000_000);
    assert_eq!(line.trend_label, "↑15%");
    assert_eq!(line.color, ProgressColor::Yellow);
}

#[test]
fn progress_line_is_red_when_usage_is_more_than_twenty_points_above_speed() {
    let metric = usage_metric(
        "Current session",
        75.0,
        Some(1_000_000 + 3_600_000),
        2 * 3600,
    );
    let line = render_progress_line(&metric, 12, 1_000_000);
    assert_eq!(line.trend_label, "↑25%");
    assert_eq!(line.color, ProgressColor::Red);
}

#[test]
fn snapshot_renders_model_scoped_progress_lines() {
    let metrics = vec![
        usage_metric(
            "Current session",
            35.0,
            Some(1_000_000 + 3_600_000),
            2 * 3600,
        ),
        usage_metric(
            "Current week (all models)",
            5.0,
            Some(1_000_000 + 3_600_000),
            2 * 3600,
        ),
        usage_metric(
            "Current week (Fable)",
            25.0,
            Some(1_000_000 + 3_600_000),
            7 * 24 * 3600,
        ),
    ];

    let output = render_snapshot("team", &metrics, 1_000_000, 80);
    assert!(output.contains("Current week (Fable)\n"));
    assert!(output.contains("25% used"));
}

#[test]
fn snapshot_starts_with_last_updated_time() {
    let metrics = vec![usage_metric(
        "Current session",
        35.0,
        Some(1_000_000 + 3_600_000),
        2 * 3600,
    )];

    let output = render_snapshot("team", &metrics, 1_000_000, 80);
    assert!(output.starts_with("Last updated: "));
    assert!(output.contains("\nClaude team\n\nCurrent session\n"));
}

#[test]
fn snapshot_progress_bar_width_tracks_terminal_width() {
    let metrics = vec![usage_metric(
        "Current session",
        35.0,
        Some(1_000_000 + 3_600_000),
        2 * 3600,
    )];

    let narrow = render_snapshot("team", &metrics, 1_000_000, 40);
    let wide = render_snapshot("team", &metrics, 1_000_000, 100);

    assert!(first_progress_bar_len(&wide) > first_progress_bar_len(&narrow));
}

#[test]
fn terminal_output_uses_carriage_return_newlines_for_raw_mode() {
    let normalized = normalize_raw_mode_newlines("Claude\n\nCurrent session\r\nDone\n");
    assert_eq!(normalized, "Claude\r\n\r\nCurrent session\r\nDone\r\n");
}

#[test]
fn refresh_cooldown_blocks_until_thirty_seconds_after_usage_call() {
    let refreshed_at = Instant::now();

    assert_eq!(
        refresh_cooldown_remaining(refreshed_at + Duration::from_secs(12), refreshed_at),
        Some(Duration::from_secs(18))
    );
    assert_eq!(
        refresh_cooldown_remaining(refreshed_at + REFRESH_COOLDOWN, refreshed_at),
        None
    );
}

#[test]
fn refresh_prompt_is_gray_during_cooldown_and_green_when_ready() {
    let cooling = render_refresh_prompt(Some(Duration::from_secs(9)));
    assert!(cooling.contains("\u{1b}[90m[r] refresh 9s\u{1b}[0m"));

    let ready = render_refresh_prompt(None);
    assert!(ready.contains("\u{1b}[32m[r] refresh\u{1b}[0m"));
}

fn usage_metric(
    title: &str,
    percentage: f64,
    resets_at_ms: Option<i64>,
    period_seconds: u64,
) -> agent_limit::claude::UsageMetric {
    agent_limit::claude::UsageMetric {
        title: title.to_string(),
        percentage,
        resets_at: resets_at_ms.map(|ms| {
            chrono::DateTime::from_timestamp_millis(ms)
                .unwrap()
                .to_rfc3339()
        }),
        resets_in: "later".to_string(),
        period_seconds,
    }
}

fn first_progress_bar_len(output: &str) -> usize {
    strip_ansi(output)
        .lines()
        .find(|line| line.contains("% used"))
        .and_then(|line| line.split("  ").next())
        .expect("progress line")
        .chars()
        .count()
}

fn strip_ansi(input: &str) -> String {
    let mut stripped = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for ansi_ch in chars.by_ref() {
                if ansi_ch == 'm' {
                    break;
                }
            }
        } else {
            stripped.push(ch);
        }
    }

    stripped
}
