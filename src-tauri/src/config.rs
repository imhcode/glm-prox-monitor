//! Konfigurasi app — disimpan di config_dir OS:
//!   Windows: %APPDATA%\glm-overflow\config.json
//!   Linux:   ~/.config/glm-overflow/config.json

use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub base_url: String,
    pub token: String,
    pub interval_secs: u64,
    pub bar_visible: bool,
}

impl Default for Config {
    fn default() -> Self {
        // Token tidak pernah disimpan di source code (repo ini publik).
        // Isi lewat env GLM_OVERFLOW_TOKEN atau lewat Settings di app
        // (tersimpan di config.json lokal, di luar repo).
        let token = std::env::var("GLM_OVERFLOW_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_default();
        Self {
            base_url: "https://glm.ajianaz.dev".to_string(),
            token,
            interval_secs: 60,
            bar_visible: true,
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
