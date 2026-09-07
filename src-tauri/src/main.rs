//! glm-overflow — multi-vendor usage bar (GLM coding plan).
//! Pill overlay always-on-top + tray. Polling sesuai interval config via
//! provider aktif (glmprox / Z.ai).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod providers;
mod tray;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, WindowEvent};
use tokio::sync::RwLock;

const BAR_W: f64 = 460.0;
const BAR_H: f64 = 46.0;
const COMPACT_W: f64 = 280.0;
const COMPACT_H: f64 = 32.0;
const PANEL_H: f64 = 400.0;

pub struct AppState {
    pub config: RwLock<config::Config>,
    pub snapshot: RwLock<Option<providers::Snapshot>>,
    /// Posisi bar hasil drag yang menunggu disimpan (logical coords).
    pub pending_pos: Mutex<Option<(f64, f64)>>,
    /// Generation counter untuk debounce penyimpanan posisi.
    pub pos_gen: AtomicU64,
    /// Item tray yang punya state check (theme/compact).
    pub tray_items: Mutex<Option<tray::TrayItems>>,
}

fn collapsed_size(compact: bool) -> LogicalSize<f64> {
    if compact {
        LogicalSize::new(COMPACT_W, COMPACT_H)
    } else {
        LogicalSize::new(BAR_W, BAR_H)
    }
}

/// Posisikan bar di kanan-atas monitor utama (posisi default).
fn position_bar_top_right(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let logical_w = monitor.size().width as f64 / monitor.scale_factor();
    let self_w = window.outer_size().map(|s| s.width as f64).unwrap_or(BAR_W as u32 as f64)
        / window.scale_factor().unwrap_or(1.0);
    let x = (logical_w - self_w - 12.0).max(4.0);
    let _ = window.set_position(LogicalPosition::new(x, 10.0));
}

/// Pakai posisi tersimpan hasil drag; kalau belum ada, default kanan-atas.
fn apply_initial_position(win: &tauri::WebviewWindow, cfg: &config::Config) {
    match (cfg.bar_x, cfg.bar_y) {
        (Some(x), Some(y)) => {
            let _ = win.set_position(LogicalPosition::new(x, y));
        }
        _ => position_bar_top_right(win),
    }
}

/// Re-assert always-on-top (Linux only). Di Windows/macOS config `alwaysOnTop`
/// sudah stabil; di Linux GTK/WM bisa mengabaikan atau menjatuhkan state
/// keep-above saat window di-map, di-show ulang, atau di-resize.
fn keep_above(win: &tauri::WebviewWindow) {
    #[cfg(target_os = "linux")]
    if let Err(e) = win.set_always_on_top(true) {
        eprintln!("glm-overflow: set_always_on_top gagal: {e}");
    }
    #[cfg(not(target_os = "linux"))]
    let _ = win;
}

/// Di Wayland native tidak ada protokol keep-above untuk window biasa — GTK
/// mengabaikan set_keep_above diam-diam, jadi bar tak bisa selalu di atas.
/// Solusi: jalankan via XWayland (GDK_BACKEND=x11) yang tetap dihormati WM
/// (GNOME/KDE/XFCE/Cinnamon/dll). Opt-out: GLM_OVERFLOW_ALLOW_WAYLAND=1.
/// Harus dipanggil sebelum GTK/tauri init (dari main(), pre-thread).
#[cfg(target_os = "linux")]
fn prepare_linux_backend() {
    let env_flag = |k: &str| std::env::var(k).map(|v| !v.trim().is_empty()).unwrap_or(false);
    let on_wayland = env_flag("WAYLAND_DISPLAY")
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false);
    if !on_wayland {
        return;
    }
    if env_flag("GLM_OVERFLOW_ALLOW_WAYLAND") {
        eprintln!(
            "glm-overflow: Wayland native dipaksa aktif — WM umumnya TIDAK mengizinkan always-on-top. \
             Alternatif KDE: Window Rule 'Keep above'."
        );
        return;
    }
    if !env_flag("DISPLAY") {
        eprintln!(
            "glm-overflow: sesi Wayland tanpa XWayland (DISPLAY kosong) — \
             always-on-top mungkin tidak jalan."
        );
        return;
    }
    std::env::set_var("GDK_BACKEND", "x11");
    eprintln!(
        "glm-overflow: sesi Wayland terdeteksi — pakai XWayland agar always-on-top jalan \
         (opt-out: GLM_OVERFLOW_ALLOW_WAYLAND=1)"
    );
}

async fn apply_bar_visibility(app: &AppHandle, state: &Arc<AppState>) {
    let visible = state.config.read().await.bar_visible;
    if let Some(win) = app.get_webview_window("bar") {
        if visible {
            let cfg = state.config.read().await;
            apply_initial_position(&win, &cfg);
            drop(cfg);
            let _ = win.show();
            keep_above(&win);
        } else {
            let _ = win.hide();
        }
    }
}

pub async fn update_and_emit(app: &AppHandle, state: &Arc<AppState>) -> providers::Snapshot {
    // Clone config dulu — jangan pegang guard RwLock lintas await fetch.
    let cfg = state.config.read().await.clone();
    let snap = providers::ProviderKind::from_id(&cfg.provider)
        .fetch(&cfg)
        .await;
    *state.snapshot.write().await = Some(snap.clone());
    let _ = app.emit("stats://update", &snap);
    tray::update_tooltip(app, state).await;
    snap
}

pub async fn reset_position(app: &AppHandle, state: &Arc<AppState>) {
    {
        let mut cfg = state.config.write().await;
        cfg.bar_x = None;
        cfg.bar_y = None;
    }
    let cfg = state.config.read().await.clone();
    let _ = config::save(&cfg);
    if let Some(win) = app.get_webview_window("bar") {
        position_bar_top_right(&win);
    }
}

#[tauri::command]
async fn get_state(state: State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let cfg = state.config.read().await.clone();
    let snap = state.snapshot.read().await.clone();
    Ok(serde_json::json!({ "config": cfg, "snapshot": snap }))
}

#[tauri::command]
async fn refresh_now(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<providers::Snapshot, String> {
    let st = state.inner().clone();
    Ok(update_and_emit(&app, &st).await)
}

#[tauri::command]
async fn save_config(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    new_config: config::Config,
) -> Result<(), String> {
    let st = state.inner().clone();
    let visibility_changed;
    {
        let mut cfg = st.config.write().await;
        cfg.provider = config::normalize_provider(&new_config.provider);
        cfg.base_url = new_config.base_url.trim().trim_end_matches('/').to_string();
        if !new_config.token.trim().is_empty() {
            cfg.token = new_config.token.trim().to_string();
        }
        // Aturan sama seperti token: isi kosong = pertahankan nilai lama.
        if !new_config.zai_api_key.trim().is_empty() {
            cfg.zai_api_key = new_config.zai_api_key.trim().to_string();
        }
        cfg.interval_secs = new_config.interval_secs.clamp(10, 3600);
        cfg.compact = new_config.compact;
        cfg.theme = config::normalize_theme(&new_config.theme);
        // bar_x/bar_y sengaja dipertahankan — posisi drag tidak boleh
        // tertimpa nilai dari form settings.
        visibility_changed = cfg.bar_visible != new_config.bar_visible;
        cfg.bar_visible = new_config.bar_visible;
        config::save(&cfg)?;
    }
    if visibility_changed {
        apply_bar_visibility(&app, &st).await;
    }
    // Sinkronkan frontend (theme/compact bisa berubah lewat form).
    let cfg = st.config.read().await.clone();
    let _ = app.emit("ui://config", &cfg);
    tray::sync_checks(&app, &st).await;
    update_and_emit(&app, &st).await;
    Ok(())
}

#[tauri::command]
async fn set_expanded(
    state: State<'_, Arc<AppState>>,
    window: tauri::WebviewWindow,
    expanded: bool,
) -> Result<(), String> {
    let compact = state.config.read().await.compact;
    let size = if expanded {
        LogicalSize::new(BAR_W, PANEL_H)
    } else {
        collapsed_size(compact)
    };
    window.set_size(size).map_err(|e| e.to_string())?;
    keep_above(&window);
    Ok(())
}

#[tauri::command]
async fn set_compact(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    compact: bool,
) -> Result<(), String> {
    let st = state.inner().clone();
    st.config.write().await.compact = compact;
    let cfg = st.config.read().await.clone();
    config::save(&cfg)?;
    let _ = app.emit("ui://config", &cfg);
    tray::sync_checks(&app, &st).await;
    Ok(())
}

#[tauri::command]
async fn set_theme(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    theme: String,
) -> Result<(), String> {
    let st = state.inner().clone();
    st.config.write().await.theme = config::normalize_theme(&theme);
    let cfg = st.config.read().await.clone();
    config::save(&cfg)?;
    let _ = app.emit("ui://config", &cfg);
    tray::sync_checks(&app, &st).await;
    Ok(())
}

#[tauri::command]
async fn hide_bar(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let st = state.inner().clone();
    st.config.write().await.bar_visible = false;
    {
        let cfg = st.config.read().await.clone();
        config::save(&cfg)?;
    }
    apply_bar_visibility(&app, &st).await;
    Ok(())
}

fn main() {
    #[cfg(target_os = "linux")]
    prepare_linux_backend();

    let cfg0 = config::load();
    let state = Arc::new(AppState {
        config: RwLock::new(cfg0.clone()),
        snapshot: RwLock::new(None),
        pending_pos: Mutex::new(None),
        pos_gen: AtomicU64::new(0),
        tray_items: Mutex::new(None),
    });

    tauri::Builder::default()
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            get_state,
            refresh_now,
            save_config,
            set_expanded,
            set_compact,
            set_theme,
            reset_bar_position,
            start_bar_drag,
            hide_bar
        ])
        .setup(move |app| {
            let items = tray::create(app)?;
            *app.state::<Arc<AppState>>().tray_items.lock().unwrap() = Some(items);

            if let Some(win) = app.get_webview_window("bar") {
                apply_initial_position(&win, &cfg0);
                keep_above(&win);

                // Beberapa WM X11 mengabaikan hint keep-above yang diset
                // sebelum window di-map — ulangi beberapa kali setelahnya.
                let retries = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    for ms in [300u64, 1_000, 3_000] {
                        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                        if let Some(win) = retries.get_webview_window("bar") {
                            keep_above(&win);
                        }
                    }
                });
            }

            // Watchdog: kalau WM menjatuhkan state keep-above di tengah jalan,
            // kembalikan tiap 20 detik. Di X11 ini kirim ulang request EWMH
            // _NET_WM_STATE ADD; WM no-op kalau memang sudah di atas.
            #[cfg(target_os = "linux")]
            {
                let watchdog = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(20)).await;
                        if let Some(win) = watchdog.get_webview_window("bar") {
                            if win.is_visible().unwrap_or(false) {
                                keep_above(&win);
                            }
                        }
                    }
                });
            }

            // Poller: fetch pertama, lalu berulang sesuai interval config.
            let handle = app.handle().clone();
            let st = state.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    update_and_emit(&handle, &st).await;
                    let iv = st.config.read().await.interval_secs.clamp(10, 3600);
                    tokio::time::sleep(std::time::Duration::from_secs(iv)).await;
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            // Drag bar → simpan posisi (debounce 800ms setelah move terakhir).
            if let WindowEvent::Moved(pos) = event {
                eprintln!("glm-overflow: moved to {},{}", pos.x, pos.y);
                let app = window.app_handle();
                if window.label() != "bar" {
                    return;
                }
                let Some(win) = app.get_webview_window("bar") else {
                    return;
                };
                let st = app.state::<Arc<AppState>>().inner().clone();
                let scale = win.scale_factor().unwrap_or(1.0);
                let logical = (pos.x as f64 / scale, pos.y as f64 / scale);
                *st.pending_pos.lock().unwrap() = Some(logical);
                let gen = st.pos_gen.fetch_add(1, Ordering::SeqCst) + 1;
                #[cfg(target_os = "linux")]
                let app_linux = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                    if st.pos_gen.load(Ordering::SeqCst) == gen {
                        // Ambil nilai posisi dulu — jangan pegang guard lintas await.
                        let pos_opt = *st.pending_pos.lock().unwrap();
                        if let Some((x, y)) = pos_opt {
                            let mut cfg = st.config.write().await;
                            cfg.bar_x = Some(x);
                            cfg.bar_y = Some(y);
                            if let Err(e) = config::save(&cfg) {
                                eprintln!("glm-overflow: gagal simpan posisi: {e}");
                            }
                        }
                        // Drag selesai — sebagian WM menjatuhkan keep-above saat move.
                        #[cfg(target_os = "linux")]
                        if let Some(win) = app_linux.get_webview_window("bar") {
                            keep_above(&win);
                        }
                    }
                });
            }
            // Kalau WM menjatuhkan keep-above, pulihkan begitu fokus bar berubah.
            #[cfg(target_os = "linux")]
            if let WindowEvent::Focused(_) = event {
                if window.label() == "bar" {
                    if let Some(win) = window.app_handle().get_webview_window("bar") {
                        keep_above(&win);
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running glm-overflow");
}

#[tauri::command]
async fn start_bar_drag(window: tauri::WebviewWindow) -> Result<(), String> {
    window.start_dragging().map_err(|e| e.to_string())
}

#[tauri::command]
async fn reset_bar_position(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let st = state.inner().clone();
    reset_position(&app, &st).await;
    Ok(())
}
