use crate::claude::UsageMetric;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Claude,
    Kimi,
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Provider::Claude => "Claude",
            Provider::Kimi => "Kimi",
        };
        f.write_str(name)
    }
}

impl std::str::FromStr for Provider {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "claude" => Ok(Provider::Claude),
            "kimi" => Ok(Provider::Kimi),
            other => Err(format!(
                "unknown provider `{other}` (expected claude or kimi)"
            )),
        }
    }
}

/// Providers that have credentials, in fixed display order (Claude, Kimi).
pub fn available_providers(claude_available: bool, kimi_available: bool) -> Vec<Provider> {
    let mut providers = Vec::new();
    if claude_available {
        providers.push(Provider::Claude);
    }
    if kimi_available {
        providers.push(Provider::Kimi);
    }
    providers
}

/// Index of the requested provider if available, otherwise the first tab (0).
pub fn initial_active_index(providers: &[Provider], requested: Provider) -> usize {
    providers
        .iter()
        .position(|provider| *provider == requested)
        .unwrap_or(0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderSnapshot {
    pub provider: Provider,
    pub plan: String,
    pub metrics: Vec<UsageMetric>,
}

#[derive(Debug)]
pub enum TabState {
    Empty,
    Ready {
        snapshot: ProviderSnapshot,
        fetched_at: Instant,
    },
    Failed {
        message: String,
        fetched_at: Instant,
    },
}
