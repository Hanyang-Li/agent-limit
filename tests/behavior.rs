use agent_limit::args::{
    DEFAULT_INTERVAL_SECONDS, parse_interval_seconds_from, parse_options_from,
};
use agent_limit::claude::{ClaudeUsageResponse, UsageWindow, map_usage_response};
use agent_limit::provider::Provider as ArgProvider;
use agent_limit::render::{ProgressColor, calculate_trajectory_percent, render_progress_line};
use agent_limit::terminal::{
    REFRESH_COOLDOWN, normalize_raw_mode_newlines, refresh_cooldown_remaining,
    render_refresh_prompt,
};
use std::time::{Duration, Instant};

#[test]
fn interval_defaults_to_default_constant() {
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
fn default_interval_is_three_hundred_seconds() {
    assert_eq!(DEFAULT_INTERVAL_SECONDS, 300);
    let options = parse_options_from(["agent-limit"]).expect("valid args");
    assert_eq!(options.interval, 300);
    assert_eq!(options.provider, ArgProvider::Claude);
}

#[test]
fn provider_flag_selects_default_tab() {
    let short = parse_options_from(["agent-limit", "-p", "kimi"]).expect("valid args");
    let long = parse_options_from(["agent-limit", "--provider", "claude"]).expect("valid args");
    assert_eq!(short.provider, ArgProvider::Kimi);
    assert_eq!(long.provider, ArgProvider::Claude);
}

#[test]
fn provider_flag_rejects_unknown_provider() {
    let error = parse_options_from(["agent-limit", "-p", "gpt"]).expect_err("invalid provider");
    assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
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
    assert!(cooling.contains("\u{1b}[90m[R]efresh 9s\u{1b}[0m"));
    assert!(cooling.contains("[Q]uit"));

    let ready = render_refresh_prompt(None);
    assert!(ready.contains("\u{1b}[32m[R]efresh\u{1b}[0m"));
    assert!(ready.contains("[Q]uit"));
}

use agent_limit::claude::claude_snapshot_from_parts;
use agent_limit::provider::Provider as ClaudeProvider;

#[test]
fn claude_provider_snapshot_carries_provider_and_plan() {
    let metrics = vec![agent_limit::claude::UsageMetric {
        title: "Current session".to_string(),
        percentage: 10.0,
        resets_at: None,
        resets_in: "unknown".to_string(),
        period_seconds: 5 * 3600,
    }];
    let snapshot = claude_snapshot_from_parts("Max".to_string(), metrics);
    assert_eq!(snapshot.provider, ClaudeProvider::Claude);
    assert_eq!(snapshot.plan, "Max");
    assert_eq!(snapshot.metrics.len(), 1);
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

use agent_limit::provider::{Provider, available_providers, initial_active_index};

#[test]
fn providers_are_ordered_claude_then_kimi() {
    assert_eq!(
        available_providers(true, true),
        vec![Provider::Claude, Provider::Kimi]
    );
    assert_eq!(available_providers(false, true), vec![Provider::Kimi]);
    assert_eq!(available_providers(true, false), vec![Provider::Claude]);
    assert!(available_providers(false, false).is_empty());
}

#[test]
fn provider_parses_case_insensitively_and_rejects_unknown() {
    assert_eq!("claude".parse::<Provider>().unwrap(), Provider::Claude);
    assert_eq!("KIMI".parse::<Provider>().unwrap(), Provider::Kimi);
    assert!("gpt".parse::<Provider>().is_err());
}

#[test]
fn initial_active_index_prefers_requested_then_falls_back_to_zero() {
    let both = vec![Provider::Claude, Provider::Kimi];
    assert_eq!(initial_active_index(&both, Provider::Kimi), 1);
    // requested provider not available -> first available
    let kimi_only = vec![Provider::Kimi];
    assert_eq!(initial_active_index(&kimi_only, Provider::Claude), 0);
}

use agent_limit::kimi::{
    build_writeback_json, compute_expires_at, parse_credentials, token_is_fresh,
};

#[test]
fn token_is_fresh_only_within_thirty_second_skew() {
    // fresh: now < expires_at - 30
    assert!(token_is_fresh(Some(1_000), 900));
    // stale: within 30s of expiry
    assert!(!token_is_fresh(Some(1_000), 980));
    assert!(!token_is_fresh(Some(1_000), 1_200));
    // null expiry treated as fresh (matches kimi CLI)
    assert!(token_is_fresh(None, 10_000));
}

#[test]
fn compute_expires_at_adds_expires_in_to_now() {
    assert_eq!(compute_expires_at(1_000, 900), 1_900);
}

#[test]
fn writeback_preserves_schema_and_updates_token_fields() {
    let existing: serde_json::Value = serde_json::json!({
        "version": "1.0",
        "type": "oauth_token",
        "access_token": "old-access",
        "refresh_token": "old-refresh",
        "expires_at": 1_000,
        "scope": "kimi-code",
        "token_type": "Bearer"
    });
    let out = build_writeback_json(&existing, "new-access", "new-refresh", 2_000);
    assert_eq!(out["version"], "1.0");
    assert_eq!(out["type"], "oauth_token");
    assert_eq!(out["access_token"], "new-access");
    assert_eq!(out["refresh_token"], "new-refresh");
    assert_eq!(out["expires_at"], 2_000);
    assert_eq!(out["scope"], "kimi-code"); // untouched
}

#[test]
fn parse_credentials_reads_tokens_and_expiry() {
    let creds =
        parse_credentials(r#"{"access_token":"a","refresh_token":"r","expires_at":1784296622}"#)
            .expect("valid creds");
    assert_eq!(creds.access_token.as_deref(), Some("a"));
    assert_eq!(creds.refresh_token.as_deref(), Some("r"));
    assert_eq!(creds.expires_at, Some(1784296622));
}

use agent_limit::kimi::{KimiUsageResponse, kimi_plan_label, map_kimi_usage};

fn sample_kimi_response() -> KimiUsageResponse {
    serde_json::from_value(serde_json::json!({
        "user": { "membership": { "level": "pro" } },
        "subType": "kimi-code",
        "usage": { "used": 40, "limit": 200, "remaining": 160, "resetTime": "2026-07-20T18:59:00Z" },
        "limits": [
            { "window": { "duration": 300, "timeUnit": "TIME_UNIT_MINUTE" },
              "detail": { "used": "5", "limit": "50", "remaining": "45", "resetTime": "2026-07-17T14:19:00Z" } }
        ],
        "totalQuota": { "limit": 1000, "remaining": 250 },
        "parallel": { "limit": 4 }
    }))
    .expect("valid kimi response")
}

#[test]
fn kimi_usage_maps_session_week_and_monthly() {
    let metrics = map_kimi_usage(&sample_kimi_response());
    assert_eq!(metrics.len(), 3);

    assert_eq!(metrics[0].title, "Current session");
    assert_eq!(metrics[0].percentage.round(), 10.0); // 5/50
    assert_eq!(metrics[0].period_seconds, 5 * 3600);
    assert!(metrics[0].resets_at.is_some());

    assert_eq!(metrics[1].title, "Current week");
    assert_eq!(metrics[1].percentage.round(), 20.0); // 40/200
    assert_eq!(metrics[1].period_seconds, 7 * 24 * 3600);

    assert_eq!(metrics[2].title, "Monthly quota");
    assert_eq!(metrics[2].percentage.round(), 75.0); // (1000-250)/1000
    assert!(metrics[2].resets_at.is_none()); // no trajectory
}

#[test]
fn kimi_usage_handles_missing_sections_as_zero() {
    let response: KimiUsageResponse =
        serde_json::from_value(serde_json::json!({})).expect("empty response");
    let metrics = map_kimi_usage(&response);
    assert_eq!(metrics.len(), 3);
    assert_eq!(metrics[0].percentage, 0.0);
    assert_eq!(metrics[1].percentage, 0.0);
    assert_eq!(metrics[2].percentage, 0.0);
}

#[test]
fn kimi_plan_label_prefers_membership_then_subtype() {
    assert_eq!(kimi_plan_label(&sample_kimi_response()), "pro");
    let no_level: KimiUsageResponse = serde_json::from_value(serde_json::json!({
        "subType": "kimi-code"
    }))
    .unwrap();
    assert_eq!(kimi_plan_label(&no_level), "kimi-code");
    let empty: KimiUsageResponse = serde_json::from_value(serde_json::json!({})).unwrap();
    assert_eq!(kimi_plan_label(&empty), "Kimi");
}

use agent_limit::kimi::refresh_request_form;

#[test]
fn refresh_request_form_has_client_id_and_grant() {
    let form = refresh_request_form("my-refresh-token");
    assert!(form.contains(&(
        "client_id",
        "17e5f671-d194-4dfb-9706-5516cb48c098".to_string()
    )));
    assert!(form.contains(&("grant_type", "refresh_token".to_string())));
    assert!(form.contains(&("refresh_token", "my-refresh-token".to_string())));
}

use agent_limit::provider::Provider as RenderProvider;
use agent_limit::render::{
    format_frequency, format_header, render_box, render_provider_body, render_tab_bar,
    visible_width,
};

#[test]
fn frequency_is_humanized() {
    assert_eq!(format_frequency(300), "5m");
    assert_eq!(format_frequency(90), "1m 30s");
    assert_eq!(format_frequency(45), "45s");
    assert_eq!(format_frequency(3600), "1h");
    assert_eq!(format_frequency(3661), "1h 1m 1s");
}

#[test]
fn header_shows_updated_ago_and_frequency() {
    let header = format_header(Some(1_000_000), 12, 300);
    assert!(header.contains("Updated "));
    assert!(header.contains("12s ago"));
    assert!(header.contains("every 5m"));
}

#[test]
fn header_humanizes_large_ago_with_units() {
    // 3661s → "1h 1m 1s"; largest unit is hours.
    let header = format_header(Some(1_000_000), 3_661, 300);
    assert!(header.contains("1h 1m 1s ago"), "got: {header}");
}

#[test]
fn header_shows_fetching_before_first_data() {
    let header = format_header(None, 0, 300);
    assert!(header.contains("Fetching"));
    assert!(header.contains("every 5m"));
}

#[test]
fn tab_bar_absent_for_single_provider_and_highlights_active() {
    assert!(render_tab_bar(&[RenderProvider::Claude], 0).is_none());

    let bar = render_tab_bar(&[RenderProvider::Claude, RenderProvider::Kimi], 1)
        .expect("tab bar for two providers");
    assert!(bar.contains("Claude"));
    assert!(bar.contains("Kimi"));
    // Active tab (Kimi, index 1) uses its brand color #66A6F8 as background.
    assert!(
        bar.contains("\u{1b}[48;2;102;166;248m"),
        "active tab should have brand background, got: {bar:?}"
    );
}

#[test]
fn visible_width_ignores_ansi_codes() {
    assert_eq!(visible_width("\u{1b}[32m█████\u{1b}[0m"), 5);
}

const CLAUDE_RGB: (u8, u8, u8) = (0xCA, 0x7C, 0x5E);
const KIMI_RGB: (u8, u8, u8) = (0x66, 0xA6, 0xF8);

#[test]
fn render_box_frames_body_to_inner_width() {
    let out = render_box("Claude · Max", &["hello".to_string()], 20, CLAUDE_RGB);
    let lines: Vec<&str> = out.lines().collect();
    assert!(out.contains("╭─ "));
    assert!(out.contains("Claude · Max"));
    assert!(out.contains("hello"));
    assert!(lines[0].contains('╮'));
    assert!(lines.last().unwrap().contains('╰'));
    assert!(lines.last().unwrap().contains('╯'));
    // Border drawn in the brand color, title bold.
    assert!(
        lines[0].contains("\u{1b}[38;2;202;124;94m"),
        "border should use brand color: {:?}",
        lines[0]
    );
    assert!(lines[0].contains("\u{1b}[1m"), "title should be bold");
    // every rendered line has equal visible width
    let width = visible_width(lines[0]);
    for line in &lines {
        assert_eq!(visible_width(line), width);
    }
}

#[test]
fn render_box_keeps_equal_width_when_title_exceeds_inner_width() {
    // Title (12 chars) is far longer than inner_width (5) — must be truncated
    // with an ellipsis so every rendered line still has equal visible width.
    let out = render_box("Claude · Max", &["hi".to_string()], 5, CLAUDE_RGB);
    let lines: Vec<&str> = out.lines().collect();
    let width = visible_width(lines[0]);
    for line in &lines {
        assert_eq!(visible_width(line), width, "line width mismatch: {line:?}");
    }
    assert!(
        out.contains('…'),
        "long title should be ellipsized: {out:?}"
    );
    assert!(out.contains("╭─ "));
    assert!(lines[0].contains('╮'));
}

#[test]
fn render_box_keeps_equal_width_when_body_line_overflows() {
    // Body line (25 visible chars) is wider than inner_width (10) and must be
    // truncated so every rendered line keeps equal visible width.
    let out = render_box(
        "Kimi",
        &["Current week (all models)".to_string()],
        10,
        KIMI_RGB,
    );
    let lines: Vec<&str> = out.lines().collect();
    let width = visible_width(lines[0]);
    for line in &lines {
        assert_eq!(visible_width(line), width, "line width mismatch: {line:?}");
    }
    assert!(
        out.contains('…'),
        "overflowing body line should be ellipsized"
    );
}

#[test]
fn provider_body_contains_metric_titles_and_percentages() {
    let metrics = vec![usage_metric(
        "Current session",
        35.0,
        Some(1_000_000 + 3_600_000),
        2 * 3600,
    )];
    let body = render_provider_body(&metrics, 1_000_000, 40);
    let joined = body.join("\n");
    assert!(joined.contains("Current session"));
    assert!(joined.contains("35% used"));
    assert!(joined.contains("Resets "));
}

use agent_limit::terminal::{fetch_time_ms, should_fetch};

#[test]
fn should_fetch_when_empty_or_interval_elapsed() {
    let interval = Duration::from_secs(300);
    assert!(should_fetch(None, interval)); // never fetched
    assert!(should_fetch(Some(Duration::from_secs(300)), interval)); // exactly due
    assert!(should_fetch(Some(Duration::from_secs(400)), interval)); // overdue
    assert!(!should_fetch(Some(Duration::from_secs(120)), interval)); // still fresh
}

#[test]
fn fetch_time_ms_is_now_minus_elapsed() {
    // 12s ago at now=1_000_000ms → 988_000ms; consistent with the "Ns ago" label.
    assert_eq!(fetch_time_ms(1_000_000, 12), 988_000);
    assert_eq!(fetch_time_ms(1_000_000, 0), 1_000_000);
}
