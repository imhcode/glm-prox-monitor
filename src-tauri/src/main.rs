//! glm-overflow — GLM coding plan usage bar.
//! Pill overlay selalu-on-top + tray. Polling `/stats` sesuai interval di config.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod config;
mod tray;

use std::sync::Arc;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State};
use tokio::sync::RwLock;

const BAR_W: f64 = 460.0;
const BAR_H: f64 = 46.0;
const PANEL_H: f64 = 372.0;

pub struct AppState {
    pub config: RwLock<config::Config>,
    pub snapshot: RwLock<Option<api::Snapshot>>,
}

/// Posisikan bar di kanan-atas monitor utama.
fn position_bar_top_right(window: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return;
    };
    let logical_w = monitor.size().width as f64 / monitor.scale_factor();
    let x = (logical_w - BAR_W - 12.0).max(4.0);
    let _ = window.set_position(LogicalPosition::new(x, 10.0));
}

async fn apply_bar_visibility(app: &AppHandle, state: &Arc<AppState>) {
    let visible = state.config.read().await.bar_visible;
    if let Some(win) = app.get_webview_window("bar") {
        if visible {
            position_bar_top_right(&win);
            let _ = win.show();
        } else {
            let _ = win.hide();
        }
    }
}

pub async fn update_and_emit(app: &AppHandle, state: &Arc<AppState>) -> api::Snapshot {
    let (base, token) = {
        let c = state.config.read().await;
        (c.base_url.clone(), c.token.clone())
    };
    let snap = api::fetch_snapshot(&base, &token).await;
    *state.snapshot.write().await = Some(snap.clone());
    let _ = app.emit("stats://update", &snap);
    tray::update_tooltip(app, state).await;
    snap
}

#[tauri::command]
async fn get_state(state: State<'_, Arc<AppState>>) -> Result<serde_json::Value, String> {
    let cfg = state.config.read().await.clone();
    let snap = state.snapshot.read().await.clone();
    Ok(serde_json::json!({ "config": cfg, "snapshot": snap }))
}

#[tauri::command]
async fn refresh_now(app: AppHandle, state: State<'_, Arc<AppState>>) -> Result<api::Snapshot, String> {
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
    {
        let mut cfg = st.config.write().await;
        cfg.base_url = new_config.base_url.trim().trim_end_matches('/').to_string();
        if !new_config.token.trim().is_empty() {
            cfg.token = new_config.token.trim().to_string();
        }
        cfg.interval_secs = new_config.interval_secs.clamp(10, 3600);
        let visibility_changed = cfg.bar_visible != new_config.bar_visible;
        cfg.bar_visible = new_config.bar_visible;
        config::save(&cfg)?;
        if visibility_changed {
            drop(cfg);
            apply_bar_visibility(&app, &st).await;
        }
    }
    update_and_emit(&app, &st).await;
    Ok(())
}

#[tauri::command]
async fn set_expanded(window: tauri::WebviewWindow, expanded: bool) -> Result<(), String> {
    let h = if expanded { PANEL_H } else { BAR_H };
    window
        .set_size(LogicalSize::new(BAR_W, h))
        .map_err(|e| e.to_string())
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
    let state = Arc::new(AppState {
        config: RwLock::new(config::load()),
        snapshot: RwLock::new(None),
    });

    tauri::Builder::default()
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            get_state,
            refresh_now,
            save_config,
            set_expanded,
            hide_bar
        ])
        .setup(move |app| {
            tray::create(app)?;
            if let Some(win) = app.get_webview_window("bar") {
                position_bar_top_right(&win);
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
        .run(tauri::generate_context!())
        .expect("error while running glm-overflow");
}
