//! Tray icon + menu: Show/Hide Bar, Refresh Now, Settings, Quit.

use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager};

use crate::api;
use crate::AppState;

const TRAY_ID: &str = "glm-overflow-tray";

pub fn create(app: &App) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Show / Hide Bar", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh Now", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit glm-overflow", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&toggle, &refresh, &settings, &quit])?;

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
    Ok(())
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
