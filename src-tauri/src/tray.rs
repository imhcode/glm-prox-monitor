//! Tray icon + menu: Show/Hide, Refresh, Compact, Theme, Reset Posisi, Settings, Quit.

use std::sync::Arc;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager};

use crate::api;
use crate::AppState;

const TRAY_ID: &str = "glm-overflow-tray";

/// Handle item tray ber-state — dipakai untuk sinkron tanda centang.
pub struct TrayItems {
    pub compact: CheckMenuItem<tauri::Wry>,
    pub theme_dark: CheckMenuItem<tauri::Wry>,
    pub theme_light: CheckMenuItem<tauri::Wry>,
    pub theme_midnight: CheckMenuItem<tauri::Wry>,
    pub theme_oled: CheckMenuItem<tauri::Wry>,
}

pub fn create(app: &App) -> tauri::Result<TrayItems> {
    let toggle = MenuItem::with_id(app, "toggle", "Show / Hide Bar", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh Now", true, None::<&str>)?;

    let compact = CheckMenuItem::with_id(
        app, "compact", "Mode Compact (tanpa garis)", true, false, None::<&str>,
    )?;

    let theme_dark = CheckMenuItem::with_id(app, "theme-dark", "Dark", true, true, None::<&str>)?;
    let theme_light =
        CheckMenuItem::with_id(app, "theme-light", "Light", true, false, None::<&str>)?;
    let theme_midnight =
        CheckMenuItem::with_id(app, "theme-midnight", "Midnight", true, false, None::<&str>)?;
    let theme_oled =
        CheckMenuItem::with_id(app, "theme-oled", "OLED Black", true, false, None::<&str>)?;
    let theme_menu = Submenu::with_items(
        app,
        "Theme",
        true,
        &[&theme_dark, &theme_light, &theme_midnight, &theme_oled],
    )?;

    let reset_pos = MenuItem::with_id(app, "reset-pos", "Reset Posisi Bar", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit glm-overflow", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&toggle, &refresh, &compact, &theme_menu, &reset_pos, &settings, &quit],
    )?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("glm-overflow: memuat…")
        .on_menu_event(|app, event| menu_action(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_bar(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }

    builder.build(app)?;

    Ok(TrayItems {
        compact,
        theme_dark,
        theme_light,
        theme_midnight,
        theme_oled,
    })
}

fn menu_action(app: &AppHandle, id: &str) {
    match id {
        "toggle" => toggle_bar(app),
        "refresh" => {
            let st = app.state::<Arc<AppState>>().inner().clone();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                crate::update_and_emit(&handle, &st).await;
            });
        }
        "compact" => {
            // CheckMenuItem sudah mengubah state centangnya sendiri saat diklik.
            let st = app.state::<Arc<AppState>>().inner().clone();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let new_val = {
                    let guard = st.tray_items.lock().unwrap();
                    guard
                        .as_ref()
                        .and_then(|i| i.compact.is_checked().ok())
                        .unwrap_or(false)
                };
                st.config.write().await.compact = new_val;
                let cfg = st.config.read().await.clone();
                let _ = crate::config::save(&cfg);
                let _ = handle.emit("ui://config", &cfg);
                sync_checks(&handle, &st).await;
            });
        }
        "theme-dark" | "theme-light" | "theme-midnight" | "theme-oled" => {
            let theme = id.trim_start_matches("theme-").to_string();
            let st = app.state::<Arc<AppState>>().inner().clone();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                st.config.write().await.theme = crate::config::normalize_theme(&theme);
                let cfg = st.config.read().await.clone();
                let _ = crate::config::save(&cfg);
                let _ = handle.emit("ui://config", &cfg);
                sync_checks(&handle, &st).await;
            });
        }
        "reset-pos" => {
            let st = app.state::<Arc<AppState>>().inner().clone();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                crate::reset_position(&handle, &st).await;
            });
        }
        "settings" => {
            if let Some(win) = app.get_webview_window("bar") {
                let _ = win.show();
                let _ = win.set_focus();
            }
            let _ = app.emit("ui://open-settings", ());
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

fn toggle_bar(app: &AppHandle) {
    let st = app.state::<Arc<AppState>>().inner().clone();
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let visible = !st.config.read().await.bar_visible;
        st.config.write().await.bar_visible = visible;
        {
            let cfg = st.config.read().await.clone();
            let _ = crate::config::save(&cfg);
        }
        crate::apply_bar_visibility(&handle, &st).await;
    });
}

/// Samakan tanda centang menu dengan config aktif.
pub async fn sync_checks(_app: &AppHandle, state: &Arc<AppState>) {
    let cfg = state.config.read().await.clone();
    let guard = state.tray_items.lock().unwrap();
    let Some(items) = guard.as_ref() else { return };
    let _ = items.compact.set_checked(cfg.compact);
    let _ = items.theme_dark.set_checked(cfg.theme == "dark");
    let _ = items.theme_light.set_checked(cfg.theme == "light");
    let _ = items.theme_midnight.set_checked(cfg.theme == "midnight");
    let _ = items.theme_oled.set_checked(cfg.theme == "oled");
}

pub async fn update_tooltip(app: &AppHandle, state: &Arc<AppState>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let snap = state.snapshot.read().await.clone();
    let text = match snap {
        Some(s) if s.kind == "ok" && s.stats.is_some() => {
            let stats = s.stats.unwrap();
            match api::used_percent(&stats) {
                Some(pct) => format!(
                    "GLM {}: {:.0}% sisa · {} tok",
                    stats.name,
                    (100.0 - pct).max(0.0),
                    stats
                        .current_usage
                        .as_ref()
                        .map(|c| api::fmt_tokens(c.remaining_tokens))
                        .unwrap_or_default()
                ),
                None => format!("GLM {}", stats.name),
            }
        }
        Some(s) if s.kind == "limited" => {
            format!("GLM limit — reset {}", s.window_ends_at.unwrap_or_else(|| "…".into()))
        }
        Some(s) => format!("glm-overflow offline: {}", s.message.unwrap_or_default()),
        None => "glm-overflow: memuat…".to_string(),
    };
    let _ = tray.set_tooltip(Some(text));
}
