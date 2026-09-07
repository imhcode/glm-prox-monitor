//! Lapisan provider multi-vendor — pola `ProviderRuntime` dari openusage:
//! auth -> client -> mapper -> Snapshot ternormalisasi.
//! Dispatch pakai enum (bukan trait object) supaya async tetap sederhana;
//! vendor baru = 1 modul + 1 varian enum + 1 field kredensial di config.

pub mod glmprox;
pub mod zai;

use serde::Serialize;

use crate::config::Config;
use glmprox::Stats;

/// Info vendor — di-embed di tiap Snapshot untuk label pill/panel/tray.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub id: &'static str,
    pub name: &'static str,
    /// Label pendek untuk chip pill.
    pub short: &'static str,
}

/// Meter generik hasil mapping respons vendor (padanan `.progress` openusage).
#[derive(Debug, Clone, Serialize)]
pub struct Meter {
    pub label: String,
    /// 0..=100 — persen terpakai.
    pub used_percent: f64,
    /// Angka mentah used/limit bila vendor menyediakannya (meter persen = None).
    pub used: Option<f64>,
    pub limit: Option<f64>,
    /// "percent" | "count"
    pub unit: String,
    /// Epoch ms reset berikutnya (None = vendor tidak mengirim).
    pub resets_at_ms: Option<u64>,
    /// Panjang window dalam ms (untuk label & fallback cadence).
    pub period_ms: Option<u64>,
}

/// Kondisi terakhir pemakaian — dikirim ke frontend lewat event `stats://update`.
#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    /// "ok" | "limited" | "failed"
    pub kind: String,
    /// unix epoch millis saat data diambil (null = belum pernah).
    pub fetched_at: Option<u64>,
    pub provider: ProviderInfo,
    /// Nama plan bila vendor menyediakannya (Z.ai: subscription list).
    pub plan: Option<String>,
    /// Data glmprox-shaped (None untuk vendor lain).
    pub stats: Option<Stats>,
    /// Meter generik untuk vendor non-glmprox (urut: sesi, mingguan/bulanan, lainnya).
    pub meters: Vec<Meter>,
    pub message: Option<String>,
    /// ISO timestamp reset window (dari error.window_ends_at saat rate-limited).
    pub window_ends_at: Option<String>,
}

impl Snapshot {
    pub fn new(provider: ProviderInfo, kind: &str) -> Self {
        Self {
            kind: kind.to_string(),
            fetched_at: now_millis(),
            provider,
            plan: None,
            stats: None,
            meters: Vec::new(),
            message: None,
            window_ends_at: None,
        }
    }

    pub fn failed(provider: ProviderInfo, message: String) -> Self {
        let mut s = Self::new(provider, "failed");
        s.message = Some(message);
        s
    }
}

pub fn now_millis() -> Option<u64> {
    Some(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as u64,
    )
}

/// HTTP client bersama — timeout & user-agent seragam untuk semua vendor.
pub(crate) fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("glm-overflow/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("client: {e}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    GlmProx,
    Zai,
}

impl ProviderKind {
    /// ID tak dikenal -> fallback GlmProx (vendor default).
    pub fn from_id(id: &str) -> Self {
        match id.trim().to_lowercase().as_str() {
            "zai" | "z.ai" => ProviderKind::Zai,
            _ => ProviderKind::GlmProx,
        }
    }

    pub fn info(self) -> ProviderInfo {
        match self {
            ProviderKind::GlmProx => ProviderInfo {
                id: "glmprox",
                name: "GLM Proxy",
                short: "GLM",
            },
            ProviderKind::Zai => ProviderInfo {
                id: "zai",
                name: "Z.ai",
                short: "Z.ai",
            },
        }
    }

    pub async fn fetch(self, cfg: &Config) -> Snapshot {
        match self {
            ProviderKind::GlmProx => glmprox::fetch(cfg).await,
            ProviderKind::Zai => zai::fetch(cfg).await,
        }
    }
}
