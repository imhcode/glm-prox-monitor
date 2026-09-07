//! Provider `glmprox` — proxy GLM coding plan (default vendor).
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

use super::{ProviderKind, Snapshot};
use crate::config::Config;

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

pub async fn fetch(cfg: &Config) -> Snapshot {
    let info = ProviderKind::GlmProx.info();
    let url = format!("{}/stats", cfg.base_url.trim_end_matches('/'));

    let client = match super::http_client() {
        Ok(c) => c,
        Err(e) => return Snapshot::failed(info, e),
    };

    let resp = match client.get(&url).bearer_auth(&cfg.token).send().await {
        Ok(r) => r,
        Err(e) => return Snapshot::failed(info, format!("jaringan: {e}")),
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
        let mut snap = Snapshot::new(info, "limited");
        snap.message = Some(msg);
        snap.window_ends_at = err
            .get("window_ends_at")
            .and_then(|w| w.as_str())
            .map(|s| s.to_string());
        return snap;
    }

    if !status.is_success() {
        return Snapshot::failed(info, format!("HTTP {status}"));
    }

    match serde_json::from_value::<Stats>(value) {
        Ok(s) => {
            let mut snap = Snapshot::new(info, "ok");
            snap.stats = Some(s);
            snap
        }
        Err(e) => Snapshot::failed(info, format!("parse: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_upstream_alias_fields() {
        let raw = r#"{
            "key": "glm_abc", "name": "utama", "model": "glm-5.3-flash",
            "token_limit_per_5h": 20000000,
            "current_usage": {
                "tokens_used_in_current_window": 123,
                "window_started_at": "2026-01-01T00:00:00Z",
                "window_ends_at": "2026-01-01T05:00:00Z",
                "remaining_tokens": 42
            },
            "total_requests": 7, "total_lifetime_tokens": 999
        }"#;
        let s: Stats = serde_json::from_str(raw).unwrap();
        assert_eq!(s.token_limit, 20_000_000);
        let cu = s.current_usage.unwrap();
        assert_eq!(cu.tokens_used, 123);
        assert_eq!(cu.remaining_tokens, 42);
        assert_eq!(cu.window_ends_at, "2026-01-01T05:00:00Z");
    }

    #[test]
    fn used_percent_math() {
        let s = Stats {
            token_limit: 200,
            current_usage: Some(CurrentUsage {
                tokens_used: 50,
                ..Default::default()
            }),
            ..Default::default()
        };
        let pct = used_percent(&s).unwrap();
        assert!((pct - 25.0).abs() < 1e-9);
    }

    #[test]
    fn used_percent_none_without_usage_or_limit() {
        assert!(used_percent(&Stats::default()).is_none());
        let s = Stats {
            token_limit: 0,
            current_usage: Some(CurrentUsage::default()),
            ..Default::default()
        };
        assert!(used_percent(&s).is_none());
    }
}
