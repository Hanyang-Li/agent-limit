use crate::claude::{UsageMetric, format_reset_time};
use crate::provider::{Provider, ProviderSnapshot};
use anyhow::{Context, Result, bail};
use chrono::Utc;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

pub const CREDENTIALS_REL_PATH: &str = ".kimi-code/credentials/kimi-code.json";
pub const CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
pub const DEFAULT_OAUTH_HOST: &str = "https://auth.kimi.com";
pub const DEFAULT_BASE_URL: &str = "https://api.kimi.com/coding/v1";
const REFRESH_SKEW_SECONDS: i64 = 30;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct KimiCredentials {
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: Option<i64>,
}

pub fn parse_credentials(raw: &str) -> Result<KimiCredentials> {
    serde_json::from_str(raw).context("failed to parse Kimi credentials file")
}

/// Matches the kimi CLI: fresh iff expires_at is null OR now < expires_at - 30.
pub fn token_is_fresh(expires_at: Option<i64>, now_seconds: i64) -> bool {
    match expires_at {
        None => true,
        Some(expires_at) => now_seconds < expires_at - REFRESH_SKEW_SECONDS,
    }
}

pub fn compute_expires_at(now_seconds: i64, expires_in: i64) -> i64 {
    now_seconds + expires_in
}

/// Rebuild the credentials JSON, preserving unrelated fields and stamping the
/// canonical version/type used by the kimi CLI.
pub fn build_writeback_json(
    existing: &Value,
    access_token: &str,
    refresh_token: &str,
    expires_at: i64,
) -> Value {
    let mut out = existing.clone();
    let map = out.as_object_mut().cloned().unwrap_or_default();
    let mut map = map;
    map.insert("version".to_string(), Value::from("1.0"));
    map.insert("type".to_string(), Value::from("oauth_token"));
    map.insert("access_token".to_string(), Value::from(access_token));
    map.insert("refresh_token".to_string(), Value::from(refresh_token));
    map.insert("expires_at".to_string(), Value::from(expires_at));
    Value::Object(map)
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct KimiUsageResponse {
    #[serde(default)]
    pub user: KimiUser,
    #[serde(rename = "subType", default)]
    pub sub_type: String,
    #[serde(default)]
    pub usage: Option<KimiWindowDetail>,
    #[serde(default)]
    pub limits: Vec<KimiLimit>,
    #[serde(rename = "totalQuota", default)]
    pub total_quota: Option<KimiWindowDetail>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct KimiUser {
    #[serde(default)]
    pub membership: KimiMembership,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct KimiMembership {
    #[serde(default)]
    pub level: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct KimiLimit {
    #[serde(default)]
    pub window: Option<KimiWindow>,
    #[serde(default)]
    pub detail: Option<KimiWindowDetail>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct KimiWindow {
    #[serde(default)]
    pub duration: Option<i64>,
    #[serde(rename = "timeUnit", default)]
    pub time_unit: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct KimiWindowDetail {
    #[serde(default)]
    pub used: Option<Value>,
    #[serde(default)]
    pub limit: Option<Value>,
    #[serde(default)]
    pub remaining: Option<Value>,
    #[serde(rename = "resetTime", default)]
    pub reset_time: Option<String>,
}

fn value_to_f64(value: &Option<Value>) -> f64 {
    match value {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
        Some(Value::String(text)) => text.trim().parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn percentage(used: f64, limit: f64) -> f64 {
    if limit > 0.0 {
        (used / limit * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    }
}

fn kimi_metric(
    title: &str,
    used: f64,
    limit: f64,
    resets_at: Option<String>,
    period_seconds: u64,
) -> UsageMetric {
    let resets_in = format_reset_time(resets_at.as_deref());
    UsageMetric {
        title: title.to_string(),
        percentage: percentage(used, limit),
        resets_at,
        resets_in,
        period_seconds,
    }
}

fn select_session_detail(limits: &[KimiLimit]) -> Option<&KimiWindowDetail> {
    limits
        .iter()
        .find(|limit| {
            limit.window.as_ref().is_some_and(|window| {
                window.duration == Some(300)
                    && window.time_unit.as_deref() == Some("TIME_UNIT_MINUTE")
            })
        })
        .or_else(|| limits.first())
        .and_then(|limit| limit.detail.as_ref())
}

pub fn map_kimi_usage(response: &KimiUsageResponse) -> Vec<UsageMetric> {
    let session = select_session_detail(&response.limits);
    let session_used = session.map(|d| value_to_f64(&d.used)).unwrap_or(0.0);
    let session_limit = session.map(|d| value_to_f64(&d.limit)).unwrap_or(0.0);
    let session_reset = session.and_then(|d| d.reset_time.clone());

    let week = response.usage.as_ref();
    let week_used = week.map(|d| value_to_f64(&d.used)).unwrap_or(0.0);
    let week_limit = week.map(|d| value_to_f64(&d.limit)).unwrap_or(0.0);
    let week_reset = week.and_then(|d| d.reset_time.clone());

    let monthly = response.total_quota.as_ref();
    let monthly_limit = monthly.map(|d| value_to_f64(&d.limit)).unwrap_or(0.0);
    let monthly_remaining = monthly.map(|d| value_to_f64(&d.remaining)).unwrap_or(0.0);
    let monthly_used = (monthly_limit - monthly_remaining).max(0.0);

    vec![
        kimi_metric(
            "Current session",
            session_used,
            session_limit,
            session_reset,
            5 * 3600,
        ),
        kimi_metric(
            "Current week",
            week_used,
            week_limit,
            week_reset,
            7 * 24 * 3600,
        ),
        kimi_metric("Monthly quota", monthly_used, monthly_limit, None, 0),
    ]
}

pub fn kimi_plan_label(response: &KimiUsageResponse) -> String {
    let level = response.user.membership.level.trim();
    if !level.is_empty() {
        return level.to_string();
    }
    let sub_type = response.sub_type.trim();
    if !sub_type.is_empty() {
        return sub_type.to_string();
    }
    "Kimi".to_string()
}

fn credentials_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(CREDENTIALS_REL_PATH))
}

fn oauth_host() -> String {
    let host = std::env::var("KIMI_CODE_OAUTH_HOST")
        .or_else(|_| std::env::var("KIMI_OAUTH_HOST"))
        .unwrap_or_else(|_| DEFAULT_OAUTH_HOST.to_string());
    host.trim_end_matches('/').to_string()
}

fn usages_url() -> String {
    let base = std::env::var("KIMI_CODE_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
    format!("{}/usages", base.trim_end_matches('/'))
}

pub fn is_kimi_available() -> bool {
    let Some(path) = credentials_path() else {
        return false;
    };
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return false;
    };
    match parse_credentials(&raw) {
        Ok(creds) => {
            creds
                .refresh_token
                .as_deref()
                .is_some_and(|t| !t.is_empty())
                || creds.access_token.as_deref().is_some_and(|t| !t.is_empty())
        }
        Err(_) => false,
    }
}

pub fn refresh_request_form(refresh_token: &str) -> Vec<(&'static str, String)> {
    vec![
        ("client_id", CLIENT_ID.to_string()),
        ("grant_type", "refresh_token".to_string()),
        ("refresh_token", refresh_token.to_string()),
    ]
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

fn write_credentials_atomic(path: &std::path::Path, value: &Value) -> Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    let serialized = serde_json::to_string_pretty(value)?;
    let tmp = path.with_extension("json.tmp");

    // Create the temp file 0600 so the credential (refresh token) file is not
    // group/world-readable after the atomic replace.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .context("failed to create temp Kimi credentials")?;
    file.write_all(serialized.as_bytes())
        .context("failed to write temp Kimi credentials")?;
    drop(file);

    if let Err(err) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp); // don't leave a 0600 tmp with live tokens behind
        return Err(err).context("failed to replace Kimi credentials");
    }
    Ok(())
}

/// Refresh the access token and persist it. Returns the new access token.
fn refresh_kimi_token(
    path: &std::path::Path,
    existing_raw: &str,
    refresh_token: &str,
) -> Result<String> {
    let url = format!("{}/api/oauth/token", oauth_host());
    let response = Client::new()
        .post(&url)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&refresh_request_form(refresh_token))
        .send()
        .context("failed to request Kimi token refresh")?;

    if !response.status().is_success() {
        bail!("Kimi token refresh returned {}", response.status());
    }

    let token: TokenResponse = response
        .json()
        .context("failed to parse Kimi token refresh response")?;

    let now_seconds = Utc::now().timestamp();
    let expires_at = compute_expires_at(now_seconds, token.expires_in.unwrap_or(900));
    let new_refresh = token
        .refresh_token
        .unwrap_or_else(|| refresh_token.to_string());

    let existing: Value = serde_json::from_str(existing_raw).unwrap_or(Value::Null);
    let writeback = build_writeback_json(&existing, &token.access_token, &new_refresh, expires_at);
    write_credentials_atomic(path, &writeback)?;

    Ok(token.access_token)
}

fn ensure_access_token(path: &std::path::Path) -> Result<String> {
    let raw = std::fs::read_to_string(path).context("failed to read Kimi credentials")?;
    let creds = parse_credentials(&raw)?;
    let now_seconds = Utc::now().timestamp();

    if let Some(access) = creds.access_token.as_deref() {
        if !access.is_empty() && token_is_fresh(creds.expires_at, now_seconds) {
            return Ok(access.to_string());
        }
    }

    let Some(refresh) = creds.refresh_token.as_deref().filter(|t| !t.is_empty()) else {
        bail!("Kimi token expired — run `kimi` to re-authenticate");
    };
    refresh_kimi_token(path, &raw, refresh)
}

fn fetch_usages(access_token: &str) -> Result<reqwest::blocking::Response> {
    Client::new()
        .get(usages_url())
        .header(reqwest::header::ACCEPT, "application/json")
        .header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {access_token}"),
        )
        .send()
        .context("failed to request Kimi usage")
}

pub fn fetch_kimi_snapshot() -> Result<ProviderSnapshot> {
    let Some(path) = credentials_path() else {
        bail!("Not logged in. Run `kimi` to authenticate.");
    };
    if !path.exists() {
        bail!("Not logged in. Run `kimi` to authenticate.");
    }

    let mut access_token = ensure_access_token(&path)?;
    let mut response = fetch_usages(&access_token)?;

    // On 401, force one refresh + retry (matches the CLI's invalidate-on-401).
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        let raw = std::fs::read_to_string(&path).context("failed to read Kimi credentials")?;
        let creds = parse_credentials(&raw)?;
        let Some(refresh) = creds.refresh_token.as_deref().filter(|t| !t.is_empty()) else {
            bail!("Kimi token expired — run `kimi` to re-authenticate");
        };
        access_token = refresh_kimi_token(&path, &raw, refresh)?;
        response = fetch_usages(&access_token)?;
    }

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        bail!("Kimi token expired — run `kimi` to re-authenticate");
    }
    if !response.status().is_success() {
        bail!("Kimi usage API returned {}", response.status());
    }

    let usage: KimiUsageResponse = response
        .json()
        .context("failed to parse Kimi usage response")?;

    Ok(ProviderSnapshot {
        provider: Provider::Kimi,
        plan: kimi_plan_label(&usage),
        metrics: map_kimi_usage(&usage),
    })
}
