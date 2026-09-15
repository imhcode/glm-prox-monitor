//! Tray icon + menu: Show/Hide, Refresh, Compact, Provider, Theme, Reset Posisi, Settings, Quit.

use std::sync::Arc;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager};

use crate::providers;
use crate::AppState;

const TRAY_ID: &str = "glm-overflow-tray";

/// Handle item tray ber-state — dipakai untuk sinkron tanda centang.
pub struct TrayItems {
    pub compact: CheckMenuItem<tauri::Wry>,
    pub provider_glmprox: CheckMenuItem<tauri::Wry>,
    pub provider_zai: CheckMenuItem<tauri::Wry>,
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

    let provider_glmprox =
        CheckMenuItem::with_id(app, "provider-glmprox", "GLM Proxy (glmprox)", true, true, None::<&str>)?;
    let provider_zai =
        CheckMenuItem::with_id(app, "provider-zai", "Z.ai (API key)", true, false, None::<&str>)?;
    let provider_menu = Submenu::with_items(
        app,
        "Provider",
        true,
        &[&provider_glmprox, &provider_zai],
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
    let check_update =
        MenuItem::with_id(app, "check-update", "Cek Update", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit glm-overflow", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &refresh,
            &compact,
            &provider_menu,
            &theme_menu,
            &reset_pos,
            &settings,
            &check_update,
            &quit,
        ],
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
        provider_glmprox,
        provider_zai,
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
        "provider-glmprox" | "provider-zai" => {
            let id = id.trim_start_matches("provider-").to_string();
            let st = app.state::<Arc<AppState>>().inner().clone();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                st.config.write().await.provider = crate::config::normalize_provider(&id);
                let cfg = st.config.read().await.clone();
                let _ = crate::config::save(&cfg);
                let _ = handle.emit("ui://config", &cfg);
                sync_checks(&handle, &st).await;
                crate::update_and_emit(&handle, &st).await;
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
        "check-update" => {
            let st = app.state::<Arc<AppState>>().inner().clone();
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let info = crate::updater::check().await;
                *st.pending_update.write().await = Some(info.clone());
                let _ = handle.emit("update://checked", &info);
                if info.update_available {
                    // Tampilkan panel supaya progress unduh terlihat.
                    if let Some(win) = handle.get_webview_window("bar") {
                        let _ = win.show();
                        let _ = win.set_focus();
                    }
                    let _ = handle.emit("ui://open-update", ());
                    let _ = crate::updater::install(&handle, &info).await;
                }
            });
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
    let _ = items
        .provider_glmprox
        .set_checked(crate::config::normalize_provider(&cfg.provider) == "glmprox");
    let _ = items
        .provider_zai
        .set_checked(crate::config::normalize_provider(&cfg.provider) == "zai");
    let _ = items.theme_dark.set_checked(cfg.theme == "dark");
    let _ = items.theme_light.set_checked(cfg.theme == "light");
    let _ = items.theme_midnight.set_checked(cfg.theme == "midnight");
    let _ = items.theme_oled.set_checked(cfg.theme == "oled");
}

pub async fn update_tooltip(app: &AppHandle, state: &Arc<AppState>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let snap = state.snapshot.read().await.clone();
    let text = match snap {
        Some(s) if s.kind == "ok" => {
            if let Some(stats) = s.stats.as_ref() {
                // glmprox: persen dari token used/limit.
                match providers::glmprox::used_percent(stats) {
                    Some(pct) => format!(
                        "{} {}: {:.0}% sisa · {} tok",
                        s.provider.short,
                        stats.name,
                        (100.0 - pct).max(0.0),
                        stats
                            .current_usage
                            .as_ref()
                            .map(|c| providers::glmprox::fmt_tokens(c.remaining_tokens))
                            .unwrap_or_default()
                    ),
                    None => format!("{} {}", s.provider.short, stats.name),
                }
            } else if let Some(m) = s.meters.first() {
                // Vendor meter-based (mis. Z.ai): meter pertama = sesi utama.
                let sisa = (100.0 - m.used_percent).max(0.0);
                match (m.used, m.limit) {
                    (Some(u), Some(l)) => format!(
                        "{} {}: {:.0}% sisa · {:.0}/{:.0}",
                        s.provider.short, m.label, sisa, u, l
                    ),
                    _ => format!("{} {}: {:.0}% sisa", s.provider.short, m.label, sisa),
                }
            } else {
                format!("{} memuat…", s.provider.short)
            }
        }
        Some(s) if s.kind == "limited" => {
            format!(
                "{} limit — reset {}",
                s.provider.short,
                s.window_ends_at.unwrap_or_else(|| "…".into())
            )
        }
        Some(s) => format!("glm-overflow offline: {}", s.message.unwrap_or_default()),
        None => "glm-overflow: memuat…".to_string(),
    };
    let _ = tray.set_tooltip(Some(text));
}
