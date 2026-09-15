//! Cek & pasang update dari GitHub Releases (repo imhcode/glm-prox-monitor).
//! Alur: check() membandingkan versi semver dengan release terbaru; install()
//! mengunduh installer NSIS ke %TEMP%, menjalankannya senyap (/S), lalu app
//! exit supaya installer bisa menimpa exe — wrapper cmd me-restart app setelah
//! install selesai. Progress dikirim ke frontend via event `update://progress`.

use std::io::Write;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

/// API release terbaru (non-draft, non-prerelease) milik repo ini.
const RELEASES_API: &str = "https://api.github.com/repos/imhcode/glm-prox-monitor/releases/latest";
/// Halaman release — fallback manual (Linux / asset tidak ditemukan).
const RELEASES_PAGE: &str = "https://github.com/imhcode/glm-prox-monitor/releases/latest";
/// Akhiran nama asset installer Windows yang di-upload release.yml.
const WIN_ASSET_SUFFIX: &str = "x64-setup.exe";

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    /// Versi terbaru di GitHub (None = gagal fetch).
    pub latest: Option<String>,
    pub update_available: bool,
    /// URL unduh installer Windows (None = asset tidak ditemukan).
    pub asset_url: Option<String>,
    pub asset_name: Option<String>,
    pub html_url: String,
    pub message: Option<String>,
}

/// Payload event `update://progress`.
#[derive(Debug, Clone, Serialize)]
struct Progress {
    /// "download" | "install" | "error" | "manual"
    stage: String,
    downloaded: u64,
    total: u64,
    message: Option<String>,
}

pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    assets: Vec<GhAsset>,
}

/// Ambil release terbaru dari GitHub & bandingkan dengan versi app.
/// Tidak pernah panik — semua kegagalan menjadi `message` di hasil.
pub async fn check() -> UpdateInfo {
    let mut info = UpdateInfo {
        current: current_version().to_string(),
        latest: None,
        update_available: false,
        asset_url: None,
        asset_name: None,
        html_url: RELEASES_PAGE.to_string(),
        message: None,
    };

    let client = match crate::providers::http_client() {
        Ok(c) => c,
        Err(e) => {
            info.message = Some(e);
            return info;
        }
    };

    let resp = match client.get(RELEASES_API).send().await {
        Ok(r) => r,
        Err(e) => {
            info.message = Some(format!("jaringan: {e}"));
            return info;
        }
    };
    if !resp.status().is_success() {
        info.message = Some(format!("HTTP {}", resp.status()));
        return info;
    }

    let release: GhRelease = match resp.json().await {
        Ok(r) => r,
        Err(e) => {
            info.message = Some(format!("parse: {e}"));
            return info;
        }
    };

    info.html_url = release.html_url;
    let latest = release.tag_name.trim().trim_start_matches('v').to_string();

    match (
        semver::Version::parse(&latest),
        semver::Version::parse(&info.current),
    ) {
        (Ok(latest_v), Ok(current_v)) => {
            info.update_available = latest_v > current_v;
        }
        _ => {
            info.message = Some(format!("versi tidak valid: {latest}"));
        }
    }
    info.latest = Some(latest);

    if let Some(asset) = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(WIN_ASSET_SUFFIX))
    {
        info.asset_url = Some(asset.browser_download_url.clone());
        info.asset_name = Some(asset.name.clone());
    }

    info
}

/// Unduh & pasang update. Di Windows: unduh installer, jalankan senyap,
/// lalu app exit (installer menimpa exe & me-restart app). Di platform lain
/// atau tanpa asset: buka halaman release (fallback manual).
pub async fn install(app: &AppHandle, info: &UpdateInfo) -> Result<(), String> {
    // Auto-install hanya Windows (installer NSIS). Platform lain / asset tidak
    // ada: buka halaman release — jangan buang bandwidth mengunduh exe.
    #[cfg(not(windows))]
    {
        let _ = info;
        open_url(RELEASES_PAGE);
        emit_progress(
            app,
            "manual",
            0,
            0,
            Some("Auto-install hanya di Windows — halaman unduhan dibuka di browser.".into()),
        );
        return Ok(());
    }

    #[cfg(windows)]
    install_windows_flow(app, info).await
}

#[cfg(windows)]
async fn install_windows_flow(app: &AppHandle, info: &UpdateInfo) -> Result<(), String> {
    let Some(url) = info.asset_url.as_deref() else {
        open_url(RELEASES_PAGE);
        emit_progress(
            app,
            "manual",
            0,
            0,
            Some(format!(
                "Update v{} tersedia — halaman unduhan dibuka di browser.",
                info.latest.as_deref().unwrap_or("?")
            )),
        );
        return Ok(());
    };

    let dest = std::env::temp_dir().join("glm-overflow-update-setup.exe");

    // Client unduhan: tanpa timeout total (installer bisa puluhan MB); cukup
    // connect-timeout + user-agent yang sama seperti client provider.
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .user_agent(concat!("glm-overflow/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("client: {e}"))?;

    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("jaringan: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let total = resp.content_length().unwrap_or(0);
    let mut file =
        std::fs::File::create(&dest).map_err(|e| format!("tulis file: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut last_emit: u64 = 0;
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("jaringan: {e}"))? {
        file.write_all(&chunk).map_err(|e| format!("tulis file: {e}"))?;
        downloaded += chunk.len() as u64;
        if downloaded - last_emit >= 256 * 1024 {
            last_emit = downloaded;
            emit_progress(app, "download", downloaded, total, None);
        }
    }
    file.flush().map_err(|e| format!("tulis file: {e}"))?;
    drop(file);

    emit_progress(app, "install", downloaded, total, None);
    install_windows(app, &dest);
    // install_windows sudah exit app; runtime tidak akan kembali ke sini di Windows.
    Ok(())
}

/// Windows: jadwalkan installer senyap via cmd wrapper lalu exit app.
/// Urutan: (jeda 2 dtk, app sempat exit & melepas lock exe) → installer /S →
/// start ulang app dari path install yang sama. Pakai `&` (bukan `&&`) supaya
/// restart tetap jalan walau installer keluar non-zero. `ping` dipakai sebagai
/// jeda karena `timeout` gagal tanpa stdin konsol.
fn install_windows(app: &AppHandle, setup: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            emit_progress(app, "error", 0, 0, Some(format!("path exe: {e}")));
            return;
        }
    };
    let script = format!(
        "ping -n 3 127.0.0.1 >nul & \"{}\" /S & start \"\" \"{}\"",
        setup.display(),
        exe.display()
    );
    let spawned = std::process::Command::new("cmd")
        .args(["/C", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
    match spawned {
        // Beri runtime waktu mengirim event terakhir, lalu lepas lock exe.
        Ok(_) => {
            std::thread::sleep(std::time::Duration::from_millis(300));
            app.exit(0);
        }
        Err(e) => emit_progress(app, "error", 0, 0, Some(format!("jalankan installer: {e}"))),
    }
}

fn emit_progress(app: &AppHandle, stage: &str, downloaded: u64, total: u64, message: Option<String>) {
    let _ = app.emit(
        "update://progress",
        Progress {
            stage: stage.to_string(),
            downloaded,
            total,
            message,
        },
    );
}

fn open_url(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(url).spawn();
    #[cfg(not(windows))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}
