//! Konfigurasi app — disimpan di config_dir OS:
//!   Windows: %APPDATA%\glm-overflow\config.json
//!   Linux:   ~/.config/glm-overflow/config.json

use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Vendor aktif: "glmprox" | "zai" (lihat providers::ProviderKind).
    pub provider: String,
    pub base_url: String,
    pub token: String,
    /// API key untuk provider Z.ai (fallback env ZAI_API_KEY saat default).
    pub zai_api_key: String,
    pub interval_secs: u64,
    pub bar_visible: bool,
    /// Posisi bar hasil drag (logical coords); None = default kanan-atas.
    pub bar_x: Option<f64>,
    pub bar_y: Option<f64>,
    /// Mode compact: pill kecil tanpa garis progress.
    pub compact: bool,
    /// dark | light | midnight | oled
    pub theme: String,
}

pub const THEMES: [&str; 4] = ["dark", "light", "midnight", "oled"];

pub fn normalize_theme(theme: &str) -> String {
    let t = theme.trim().to_lowercase();
    if THEMES.contains(&t.as_str()) {
        t
    } else {
        "dark".to_string()
    }
}

/// ID vendor tak dikenal -> fallback "glmprox" (vendor default).
pub fn normalize_provider(id: &str) -> String {
    match id.trim().to_lowercase().as_str() {
        "zai" | "z.ai" => "zai".to_string(),
        _ => "glmprox".to_string(),
    }
}

impl Default for Config {
    fn default() -> Self {
        // Kredensial tidak pernah disimpan di source code (repo ini publik).
        // Isi lewat env atau lewat Settings di app (tersimpan di config.json
        // lokal, di luar repo).
        let env_or_empty = |key: &str| {
            std::env::var(key)
                .ok()
                .filter(|t| !t.trim().is_empty())
                .unwrap_or_default()
        };
        Self {
            provider: "glmprox".to_string(),
            base_url: "https://glm.ajianaz.dev".to_string(),
            token: env_or_empty("GLM_OVERFLOW_TOKEN"),
            zai_api_key: env_or_empty("ZAI_API_KEY"),
            interval_secs: 60,
            bar_visible: true,
            bar_x: None,
            bar_y: None,
            compact: false,
            theme: "dark".to_string(),
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("glm-overflow")
        .join("config.json")
}

pub fn load() -> Config {
    if let Ok(text) = fs::read_to_string(config_path()) {
        if let Ok(cfg) = serde_json::from_str::<Config>(&text) {
            return cfg;
        }
    }
    // Pertama kali jalan (atau config rusak) — tulis default.
    let cfg = Config::default();
    let _ = save(&cfg);
    cfg
}

pub fn save(cfg: &Config) -> Result<(), String> {
    let path = config_path();
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}
