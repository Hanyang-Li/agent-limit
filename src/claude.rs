use crate::provider::{Provider, ProviderSnapshot};
use anyhow::{Context, Result, bail};
use chrono::{DateTime, Local};
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use std::process::Command;

const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UsageWindow {
    pub utilization: f64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ModelScopedUsageWindow {
    pub display_name: String,
    pub utilization: Option<f64>,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UsageLimit {
    pub kind: String,
    pub scope: Option<UsageLimitScope>,
    pub percent: Option<f64>,
    pub resets_at: Option<Value>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UsageLimitScope {
    pub model: Option<UsageLimitModelScope>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct UsageLimitModelScope {
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ClaudeUsageResponse {
    pub five_hour: Option<UsageWindow>,
    pub seven_day: Option<UsageWindow>,
    pub seven_day_opus: Option<UsageWindow>,
    #[serde(default)]
    pub model_scoped: Vec<ModelScopedUsageWindow>,
    #[serde(default)]
    pub limits: Vec<UsageLimit>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UsageMetric {
    pub title: String,
    pub percentage: f64,
    pub resets_at: Option<String>,
    pub resets_in: String,
    pub period_seconds: u64,
}

pub fn map_usage_response(response: ClaudeUsageResponse) -> Vec<UsageMetric> {
    let mut metrics = vec![
        metric_from_window(
            "Current session",
            response.five_hour.unwrap_or_else(empty_window),
            5 * 3600,
        ),
        metric_from_window(
            "Current week (all models)",
            response.seven_day.unwrap_or_else(empty_window),
            7 * 24 * 3600,
        ),
    ];

    metrics.extend(response.model_scoped.into_iter().filter_map(|window| {
        let display_name = window.display_name.trim();
        let utilization = window.utilization?;
        if display_name.is_empty() {
            return None;
        }

        Some(metric_from_window(
            &format!("Current week ({display_name})"),
            UsageWindow {
                utilization,
                resets_at: window.resets_at,
            },
            7 * 24 * 3600,
        ))
    }));

    metrics.extend(response.limits.into_iter().filter_map(|limit| {
        if limit.kind != "weekly_scoped" {
            return None;
        }

        let display_name = limit.scope?.model?.display_name;
        let display_name = display_name.trim();
        let utilization = limit.percent?;
        if display_name.is_empty() {
            return None;
        }

        Some(metric_from_window(
            &format!("Current week ({display_name})"),
            UsageWindow {
                utilization,
                resets_at: reset_value_to_string(limit.resets_at),
            },
            7 * 24 * 3600,
        ))
    }));

    metrics
}

fn empty_window() -> UsageWindow {
    UsageWindow {
        utilization: 0.0,
        resets_at: None,
    }
}

fn metric_from_window(title: &str, window: UsageWindow, period_seconds: u64) -> UsageMetric {
    let resets_in = format_reset_time(window.resets_at.as_deref());

    UsageMetric {
        title: title.to_string(),
        percentage: window.utilization,
        resets_at: window.resets_at,
        resets_in,
        period_seconds,
    }
}

fn reset_value_to_string(value: Option<Value>) -> Option<String> {
    match value? {
        Value::String(value) => Some(value),
        Value::Number(value) => value
            .as_i64()
            .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
            .map(|date| date.to_rfc3339()),
        _ => None,
    }
}

pub fn format_reset_time(iso_date: Option<&str>) -> String {
    let Some(iso_date) = iso_date else {
        return "unknown".to_string();
    };

    let Ok(parsed) = DateTime::parse_from_rfc3339(iso_date) else {
        return "unknown".to_string();
    };

    let local = parsed.with_timezone(&Local);
    let timezone = iana_time_zone::get_timezone().unwrap_or_else(|_| "Local".to_string());
    let date_part = if local.date_naive() == Local::now().date_naive() {
        local.format("%-I:%M%P").to_string()
    } else {
        local.format("%b %-d at %-I:%M%P").to_string()
    };
    format!("{date_part} ({timezone})")
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ClaudeCredentials {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: Option<String>,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(rename = "subscriptionType")]
    pub subscription_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KeychainPayload {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeCredentials>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeSnapshot {
    pub plan: String,
    pub metrics: Vec<UsageMetric>,
}

pub fn get_keychain_credentials() -> Result<Option<String>> {
    let output = Command::new("security")
        .args(["find-generic-password", "-s", KEYCHAIN_SERVICE, "-w"])
        .output()
        .context("failed to invoke macOS security command")?;

    if !output.status.success() {
        return Ok(None);
    }

    let raw = String::from_utf8(output.stdout).context("keychain output was not UTF-8")?;
    Ok(Some(raw.trim().to_string()))
}

pub fn get_claude_credentials() -> Result<Option<ClaudeCredentials>> {
    let Some(raw) = get_keychain_credentials()? else {
        return Ok(None);
    };

    let payload: KeychainPayload =
        serde_json::from_str(&raw).context("failed to parse Claude Code keychain payload")?;
    Ok(payload.claude_ai_oauth)
}

pub fn fetch_usage_with_credentials(
    credentials: &ClaudeCredentials,
) -> Result<ClaudeUsageResponse> {
    let response = Client::new()
        .get(USAGE_URL)
        .header(reqwest::header::ACCEPT, "application/json")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            reqwest::header::USER_AGENT,
            concat!("agent-limit/", env!("CARGO_PKG_VERSION")),
        )
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", credentials.access_token),
        )
        .header("anthropic-beta", "oauth-2025-04-20")
        .send()
        .context("failed to request Claude usage")?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        bail!("Claude token is expired; run `claude` to re-authenticate");
    }

    if !response.status().is_success() {
        bail!("Claude usage API returned {}", response.status());
    }

    response
        .json::<ClaudeUsageResponse>()
        .context("failed to parse Claude usage response")
}

pub fn fetch_claude_snapshot() -> Result<ClaudeSnapshot> {
    let Some(credentials) = get_claude_credentials()? else {
        bail!("Not logged in. Run `claude` to authenticate.");
    };

    let usage = fetch_usage_with_credentials(&credentials)?;
    let plan = credentials
        .subscription_type
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Pro".to_string());

    Ok(ClaudeSnapshot {
        plan,
        metrics: map_usage_response(usage),
    })
}

pub fn claude_snapshot_from_parts(plan: String, metrics: Vec<UsageMetric>) -> ProviderSnapshot {
    ProviderSnapshot {
        provider: Provider::Claude,
        plan,
        metrics,
    }
}

pub fn is_claude_available() -> bool {
    matches!(get_claude_credentials(), Ok(Some(_)))
}

pub fn fetch_claude_provider_snapshot() -> Result<ProviderSnapshot> {
    let snapshot = fetch_claude_snapshot()?;
    Ok(claude_snapshot_from_parts(snapshot.plan, snapshot.metrics))
}
