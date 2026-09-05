//! Fetch + parse response endpoint `/stats`.
//!
//! Schema sukses (terverifikasi dari API):
//! {
//!   "key": "glm_...fpH", "name": "...", "model": "glm-5.3-flash",
//!   "token_limit_per_5h": 20000000, "expiry_date": "...", "created_at": "...",
//!   "last_used": "...", "is_expired": false,
//!   "current_usage": { "tokens_used_in_current_window": ..., "window_started_at": ...,
//!                      "window_ends_at": ..., "remaining_tokens": ... },
//!   "total_requests": ..., "total_lifetime_tokens": ...
//! }
//! Response error (rate limit) punya bentuk { "error": { ..., "window_ends_at": ... } }.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CurrentUsage {
    // rename = nama wire Rust->JS; alias = nama field dari API upstream
    #[serde(default, rename = "tokens_used", alias = "tokens_used_in_current_window")]
    pub tokens_used: u64,
    #[serde(default, rename = "window_started_at")]
    pub window_started_at: String,
    #[serde(default, rename = "window_ends_at")]
    pub window_ends_at: String,
    #[serde(default, rename = "remaining_tokens")]
    pub remaining_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Stats {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub model: String,
    #[serde(default, rename = "token_limit", alias = "token_limit_per_5h")]
    pub token_limit: u64,
    #[serde(default, rename = "expiry_date")]
    pub expiry_date: String,
    #[serde(default, rename = "created_at")]
    pub created_at: String,
    #[serde(default, rename = "last_used")]
    pub last_used: String,
    #[serde(default, rename = "is_expired")]
    pub is_expired: bool,
    #[serde(default, rename = "current_usage")]
    pub current_usage: Option<CurrentUsage>,
    #[serde(default, rename = "total_requests")]
    pub total_requests: u64,
    #[serde(default, rename = "total_lifetime_tokens")]
    pub total_lifetime_tokens: u64,
}

/// Kondisi terakhir pemakaian — dikirim ke frontend lewat event `stats://update`.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    /// "ok" | "limited" | "failed"
    pub kind: String,
    /// unix epoch millis saat data diambil (null = belum pernah)
    pub fetched_at: Option<u64>,
    pub stats: Option<Stats>,
    pub message: Option<String>,
    /// ISO timestamp reset window (dari error.window_ends_at saat rate-limited)
    pub window_ends_at: Option<String>,
}

fn now_millis() -> Option<u64> {
    Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as u64,
    )
}

fn snapshot(kind: &str, stats: Option<Stats>, message: Option<String>, window_ends_at: Option<String>) -> Snapshot {
    Snapshot {
        kind: kind.to_string(),
        fetched_at: now_millis(),
        stats,
        message,
        window_ends_at,
    }
}

pub fn used_percent(stats: &Stats) -> Option<f64> {
    let cu = stats.current_usage.as_ref()?;
    if stats.token_limit == 0 {
        return None;
    }
    Some((cu.tokens_used as f64 / stats.token_limit as f64) * 100.0)
}

pub fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000_000 {
        format!("{:.1}B", n as f64 / 1e9)
    } else if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.0}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

pub async fn fetch_snapshot(base_url: &str, token: &str) -> Snapshot {
    let url = format!("{}/stats", base_url.trim_end_matches('/'));

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("glm-overflow/0.1")
        .build()
    {
        Ok(c) => c,
        Err(e) => return snapshot("failed", None, Some(format!("client: {e}")), None),
    };

    let resp = match client.get(&url).bearer_auth(token).send().await {
        Ok(r) => r,
        Err(e) => return snapshot("failed", None, Some(format!("jaringan: {e}")), None),
    };

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);

    // Error path — mis. rate limit: { "error": { ..., "window_ends_at": "..." } }
    if let Some(err) = value.get("error") {
        let msg = err
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("error dari API")
            .to_string();
        let wea = err
            .get("window_ends_at")
            .and_then(|w| w.as_str())
            .map(|s| s.to_string());
        return snapshot("limited", None, Some(msg), wea);
    }

    if !status.is_success() {
        return snapshot("failed", None, Some(format!("HTTP {status}")), None);
    }

    match serde_json::from_value::<Stats>(value) {
        Ok(s) => snapshot("ok", Some(s), None, None),
        Err(e) => snapshot("failed", None, Some(format!("parse: {e}")), None),
    }
}
