//! Tauri application library entry point.
//!
//! This is Tylluan Desktop: a native shell around the existing Tylluan
//! dashboard (see tauri.conf.json's `devUrl`).
//!
//! Upgrades in UI-CIERRE:
//! 1. System tray with real-time `/health` status badge (Green = nominal,
//!    Amber = degraded, Red = offline). The owl icon itself is NEVER modified;
//!    the status is rendered strictly as a small circular corner badge overlay.
//! 2. Native notifications strictly OPT-IN (disabled by default) and generic
//!    (zero message content, zero author names, zero file paths).
//! 3. Integrated titlebar/reload workflow: removed the legacy JS-injected
//!    `#tylluan-desktop-reload-btn` floating button in favor of the integrated
//!    header reload control and native menu/tray actions.

mod utils;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

/// Opt-in state for generic native notifications (OFF by default for privacy).
static NOTIFICATIONS_OPT_IN: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KernelHealthState {
    Nominal,
    Degraded,
    Offline,
}

impl KernelHealthState {
    fn badge_rgba(self) -> [u8; 4] {
        match self {
            KernelHealthState::Nominal => [16, 185, 129, 255],  // Emerald #10b981
            KernelHealthState::Degraded => [245, 158, 11, 255], // Amber   #f59e0b
            KernelHealthState::Offline => [239, 68, 68, 255],   // Red     #ef4444
        }
    }

    fn label_es(self) -> &'static str {
        match self {
            KernelHealthState::Nominal => "Estado: Nominal (/health OK)",
            KernelHealthState::Degraded => "Estado: Degradado (/health Degraded)",
            KernelHealthState::Offline => "Estado: En espera / Apagado",
        }
    }

    fn generic_notification_body(self) -> &'static str {
        match self {
            KernelHealthState::Nominal => "El kernel soberano Tylluan está en línea y nominal.",
            KernelHealthState::Degraded => "El kernel soberano Tylluan reporta estado degradado.",
            KernelHealthState::Offline => "El kernel soberano Tylluan está en espera o desconectado.",
        }
    }
}

/// Maximum bytes to read for kernel health probe (16 KiB limit).
const MAX_HEALTH_RESPONSE_BYTES: usize = 16 * 1024;

/// Overlay a small status badge in the bottom-right corner of the base RGBA icon
/// WITHOUT modifying the owl artwork itself (the owl remains invariant).
fn build_badged_tray_icon(base: &Image<'_>, state: KernelHealthState) -> Image<'static> {
    let width = base.width();
    let height = base.height();
    let mut rgba = base.rgba().to_vec();

    if width >= 12 && height >= 12 && rgba.len() == (width * height * 4) as usize {
        let color = state.badge_rgba();
        let border = [15u8, 23u8, 42u8, 255u8]; // Slate-900 ring for contrast
        let radius = (width.min(height) as f32 * 0.18).max(2.5);
        let border_radius = radius + 1.0;
        let cx = (width as f32) - border_radius - 1.0;
        let cy = (height as f32) - border_radius - 1.0;

        for y in 0..height {
            for x in 0..width {
                let dx = (x as f32) - cx;
                let dy = (y as f32) - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                let idx = ((y * width + x) * 4) as usize;
                if dist <= radius {
                    rgba[idx..idx + 4].copy_from_slice(&color);
                } else if dist <= border_radius {
                    rgba[idx..idx + 4].copy_from_slice(&border);
                }
            }
        }
    }

    Image::new_owned(rgba, width, height)
}

/// Pure function to classify kernel health from raw HTTP response.
/// Inspects the JSON "status" field strictly via serde_json.
/// Returns Nominal for "ok"/"healthy", Degraded for "degraded"/error/503,
/// and treats incomplete, truncated or unparseable bodies as Offline.
pub fn classify_health_response(raw_http: &str) -> KernelHealthState {
    if !raw_http.starts_with("HTTP/1.1 200") && !raw_http.starts_with("HTTP/1.0 200") {
        return if raw_http.starts_with("HTTP/1.") {
            KernelHealthState::Degraded
        } else {
            KernelHealthState::Offline
        };
    }

    let body = match raw_http.split_once("\r\n\r\n") {
        Some((_, b)) => b.trim(),
        None => match raw_http.split_once("\n\n") {
            Some((_, b)) => b.trim(),
            None => return KernelHealthState::Offline,
        },
    };

    if body.is_empty() {
        return KernelHealthState::Offline;
    }

    let json_val: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(_) => return KernelHealthState::Offline,
    };

    if let Some(status_str) = json_val.get("status").and_then(|s| s.as_str()) {
        match status_str.to_ascii_lowercase().as_str() {
            "ok" | "nominal" | "healthy" => KernelHealthState::Nominal,
            "degraded" => KernelHealthState::Degraded,
            _ => KernelHealthState::Degraded,
        }
    } else if let Some(ok_bool) = json_val.get("ok").and_then(|o| o.as_bool()) {
        if ok_bool {
            KernelHealthState::Nominal
        } else {
            KernelHealthState::Degraded
        }
    } else {
        KernelHealthState::Degraded
    }
}

/// Lightweight HTTP GET probe against `127.0.0.1:47004/health` (or `TYLLUAN_PORT`).
/// Inspects the JSON response via `classify_health_response`.
fn probe_kernel_health() -> KernelHealthState {
    let port: u16 = std::env::var("TYLLUAN_PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(47004);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let timeout = Duration::from_millis(1500);

    let mut stream = match TcpStream::connect_timeout(&addr, timeout) {
        Ok(s) => s,
        Err(_) => return KernelHealthState::Offline,
    };
    let _ = stream.set_read_timeout(Some(timeout));
    let _ = stream.set_write_timeout(Some(timeout));

    let req = format!(
        "GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nAccept: application/json\r\n\r\n"
    );
    if stream.write_all(req.as_bytes()).is_err() {
        return KernelHealthState::Offline;
    }

    let mut response_bytes = Vec::new();
    let mut chunk = [0u8; 4096];

    while response_bytes.len() < MAX_HEALTH_RESPONSE_BYTES {
        match stream.read(&mut chunk) {
            Ok(0) => break, // EOF reached (server closed connection)
            Ok(n) => {
                let to_take = n.min(MAX_HEALTH_RESPONSE_BYTES - response_bytes.len());
                response_bytes.extend_from_slice(&chunk[..to_take]);
                if response_bytes.len() >= MAX_HEALTH_RESPONSE_BYTES {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break, // Timeout or connection closed: classify what was received
        }
    }

    if response_bytes.is_empty() {
        return KernelHealthState::Offline;
    }

    let resp = String::from_utf8_lossy(&response_bytes);
    classify_health_response(&resp)
}

/// Path to persisted notifications preferences JSON file.
fn get_notifications_pref_path(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("notifications_opt_in.json"))
}

/// Load persisted notification opt-in preference (OFF by default for privacy).
fn load_notifications_opt_in(app: &tauri::AppHandle) -> bool {
    if let Some(path) = get_notifications_pref_path(app) {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                return val.get("opt_in").and_then(|v| v.as_bool()).unwrap_or(false);
            }
        }
    }
    false
}

/// Persist notification opt-in preference to disk atomically.
fn save_notifications_opt_in(app: &tauri::AppHandle, enabled: bool) {
    if let Some(path) = get_notifications_pref_path(app) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let payload = serde_json::json!({
            "opt_in": enabled,
            "updated_at": std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        });
        if let Ok(bytes) = serde_json::to_vec_pretty(&payload) {
            let _ = std::fs::write(path, bytes);
        }
    }
}

/// Native folder picker dialog. Opens OS-native directory chooser and returns
/// the absolute path as a string, or null if the user cancelled.
#[tauri::command]
fn pick_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(|p| p.to_string())
}

/// Toggle or query opt-in native notifications from the integrated UI if needed.
#[tauri::command]
fn set_native_notifications_opt_in(app: tauri::AppHandle, enabled: bool) -> bool {
    NOTIFICATIONS_OPT_IN.store(enabled, Ordering::SeqCst);
    save_notifications_opt_in(&app, enabled);
    enabled
}

/// Trigger a strictly generic native notification (ONLY if the user opted in).
/// Never accepts or displays message contents, author names, or file paths.
#[tauri::command]
fn notify_generic_event(app: tauri::AppHandle, kind: String) -> bool {
    if !NOTIFICATIONS_OPT_IN.load(Ordering::SeqCst) {
        return false;
    }
    let body = match kind.as_str() {
        "coloquio" => "Nueva actividad registrada en Coloquio.",
        "approval" => "Hay una solicitud de aprobación pendiente en la puerta HITL.",
        _ => "Actualización de estado en Tylluan Sovereign.",
    };
    let _ = app
        .notification()
        .builder()
        .title("Tylluan Sovereign")
        .body(body)
        .show();
    true
}

/// Application entry point. Sets up all plugins and initializes the app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Low-resource WebView2 configuration (Windows): limit background threads & disable non-essential Chromium features
    #[cfg(target_os = "windows")]
    {
        let current_args = std::env::var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS").unwrap_or_default();
        let efficiency_args = "--renderer-process-limit=1 --disable-features=Translate,MediaSessionService,OptimizationHints --enable-low-end-device-mode";
        let merged_args = if current_args.is_empty() {
            efficiency_args.to_string()
        } else {
            format!("{current_args} {efficiency_args}")
        };
        std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", merged_args);
    }

    let mut app_builder = tauri::Builder::default();

    // Single instance plugin must be registered FIRST.
    #[cfg(desktop)]
    {
        app_builder = app_builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.unminimize();
            }
        }));
    }

    // Window state plugin — saves/restores window position and size across launches.
    #[cfg(desktop)]
    {
        app_builder = app_builder.plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(tauri_plugin_window_state::StateFlags::all())
                .build(),
        );
    }

    app_builder
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_persisted_scope::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            pick_folder,
            set_native_notifications_opt_in,
            notify_generic_event
        ])
        .setup(|app| {
            log::info!("Tylluan Desktop starting up (integrated titlebar + system tray)");

            #[cfg(desktop)]
            {
                let reload_item = MenuItem::with_id(
                    app,
                    "reload",
                    "Sincronizar / Recargar vista",
                    true,
                    Some("CmdOrCtrl+R"),
                )?;
                let view_menu = Submenu::with_items(
                    app,
                    "Vista",
                    true,
                    &[&reload_item, &PredefinedMenuItem::quit(app, None)?],
                )?;
                let menu = Menu::with_items(app, &[&view_menu])?;
                app.set_menu(menu)?;

                app.on_menu_event(move |app_handle, event| {
                    if event.id() == "reload" {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            let _ = window.eval("location.reload()");
                        }
                    }
                });

                // System Tray with invariant owl icon + dynamic status badge
                let initial_state = probe_kernel_health();
                let initial_opt_in = load_notifications_opt_in(app.handle());
                NOTIFICATIONS_OPT_IN.store(initial_opt_in, Ordering::SeqCst);

                let show_item =
                    MenuItem::with_id(app, "tray_show", "Mostrar Tylluan", true, None::<&str>)?;
                let status_item = MenuItem::with_id(
                    app,
                    "tray_status",
                    initial_state.label_es(),
                    false,
                    None::<&str>,
                )?;
                let notif_item = CheckMenuItem::with_id(
                    app,
                    "tray_notif_opt_in",
                    "Notificaciones nativas genéricas (Opt-in)",
                    true,
                    initial_opt_in,
                    None::<&str>,
                )?;
                let tray_reload = MenuItem::with_id(
                    app,
                    "tray_reload",
                    "Recargar panel",
                    true,
                    None::<&str>,
                )?;
                let tray_quit =
                    MenuItem::with_id(app, "tray_quit", "Salir", true, None::<&str>)?;

                let tray_menu = Menu::with_items(
                    app,
                    &[
                        &show_item,
                        &status_item,
                        &notif_item,
                        &tray_reload,
                        &tray_quit,
                    ],
                )?;

                let base_icon = app
                    .default_window_icon()
                    .cloned()
                    .unwrap_or_else(|| Image::new_owned(vec![0u8; 32 * 32 * 4], 32, 32));
                let badged_icon = build_badged_tray_icon(&base_icon, initial_state);

                let notif_item_for_event = notif_item.clone();
                let tray = TrayIconBuilder::with_id("tylluan-tray")
                    .icon(badged_icon)
                    .tooltip(format!("Tylluan Sovereign — {}", initial_state.label_es()))
                    .menu(&tray_menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(move |app_handle, event| match event.id().as_ref() {
                        "tray_show" => {
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                        "tray_notif_opt_in" => {
                            let is_checked = notif_item_for_event.is_checked().unwrap_or(false);
                            NOTIFICATIONS_OPT_IN.store(is_checked, Ordering::SeqCst);
                            save_notifications_opt_in(app_handle, is_checked);
                        }
                        "tray_reload" => {
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.eval("location.reload()");
                            }
                        }
                        "tray_quit" => {
                            app_handle.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray_handle, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            let app_handle = tray_handle.app_handle();
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    })
                    .build(app)?;

                // Background health poller for tray badge + opt-in generic notifications
                let app_handle = app.handle().clone();
                let status_item_clone = status_item.clone();
                let tray_id = tray.id().clone();
                std::thread::spawn(move || {
                    let mut last_state = initial_state;
                    loop {
                        std::thread::sleep(Duration::from_secs(10));
                        let next_state = probe_kernel_health();
                        if next_state != last_state {
                            last_state = next_state;
                            let status_item_thread = status_item_clone.clone();
                            let tray_id_thread = tray_id.clone();
                            let app_handle_main = app_handle.clone();
                            let _ = app_handle.run_on_main_thread(move || {
                                let _ = status_item_thread.set_text(next_state.label_es());
                                if let Some(tray_icon) = app_handle_main.tray_by_id(&tray_id_thread) {
                                    if let Some(base) = app_handle_main.default_window_icon() {
                                        let updated_icon = build_badged_tray_icon(base, next_state);
                                        let _ = tray_icon.set_icon(Some(updated_icon));
                                    }
                                    let _ = tray_icon.set_tooltip(Some(format!(
                                        "Tylluan Sovereign — {}",
                                        next_state.label_es()
                                    )));
                                }
                            });

                            // Generic opt-in notification on state transition (no paths or messages)
                            if NOTIFICATIONS_OPT_IN.load(Ordering::SeqCst) {
                                let _ = app_handle
                                    .notification()
                                    .builder()
                                    .title("Tylluan Sovereign")
                                    .body(next_state.generic_notification_body())
                                    .show();
                            }
                        }
                    }
                });
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            if let RunEvent::WindowEvent {
                label,
                event: WindowEvent::CloseRequested { api: _api, .. },
                ..
            } = &event
            {
                if label == "main" {
                    _api.prevent_close();
                    #[cfg(desktop)]
                    {
                        use tauri_plugin_window_state::{AppHandleExt, StateFlags};
                        if let Err(e) = _app_handle.save_window_state(StateFlags::all()) {
                            log::warn!("Failed to save window state: {e}");
                        }
                    }
                    if let Some(window) = _app_handle.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }

            #[cfg(target_os = "macos")]
            if let RunEvent::Reopen { .. } = &event {
                if let Some(window) = _app_handle.get_webview_window("main") {
                    if !window.is_visible().unwrap_or(true) {
                        let _ = window.show();
                        use tauri_plugin_window_state::{StateFlags, WindowExt};
                        let _ = window.restore_state(StateFlags::all());
                        let _ = window.set_focus();
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_health_response_nominal() {
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"status\":\"ok\",\"version\":\"0.17.2\"}";
        assert_eq!(classify_health_response(resp), KernelHealthState::Nominal);

        let resp_healthy = "HTTP/1.0 200 OK\r\n\r\n{\"status\":\"healthy\"}";
        assert_eq!(classify_health_response(resp_healthy), KernelHealthState::Nominal);
    }

    #[test]
    fn test_classify_health_response_degraded() {
        let resp = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"status\":\"degraded\",\"sentinel\":{\"mode\":\"observe\"}}";
        assert_eq!(classify_health_response(resp), KernelHealthState::Degraded);

        // 503 during restart/preflight
        let resp_503 = "HTTP/1.1 503 Service Unavailable\r\n\r\n{\"status\":\"restarting\"}";
        assert_eq!(classify_health_response(resp_503), KernelHealthState::Degraded);
    }

    #[test]
    fn test_classify_health_response_does_not_false_positive_on_notes() {
        // A response where notes or message contains 'degraded' but status is 'ok'
        let resp = "HTTP/1.1 200 OK\r\n\r\n{\"status\":\"ok\",\"notes\":\"previously degraded node repaired\"}";
        assert_eq!(classify_health_response(resp), KernelHealthState::Nominal);
    }

    #[test]
    fn test_classify_health_response_large_body_above_2048_bytes() {
        // Construct a large valid health response > 2048 bytes with many metrics/notes
        let padding = "a".repeat(3000);
        let resp = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{{\"status\":\"ok\",\"padding\":\"{}\"}}",
            padding
        );
        assert!(resp.len() > 2048);
        assert_eq!(classify_health_response(&resp), KernelHealthState::Nominal);
    }

    #[test]
    fn test_classify_health_response_truncated_json_is_offline() {
        // Incomplete HTTP (no body delimiter)
        let resp_cut = "HTTP/1.1 200 OK\r\nContent-Type: text/plain";
        assert_eq!(classify_health_response(resp_cut), KernelHealthState::Offline);

        // Truncated / malformed JSON without closing delimiter
        let resp_bad_json = "HTTP/1.1 200 OK\r\n\r\n{\"status\":\"ok\",\"details\":{\"incomplete\":true";
        assert_eq!(classify_health_response(resp_bad_json), KernelHealthState::Offline);

        // Non-HTTP garbled
        let resp_raw = "some random non-http bytes";
        assert_eq!(classify_health_response(resp_raw), KernelHealthState::Offline);
    }
}
