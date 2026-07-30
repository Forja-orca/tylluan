//! Tauri application library entry point.
//!
//! This is Tylluan Desktop: a native shell around the existing Tylluan
//! dashboard (see tauri.conf.json's `devUrl`). No custom frontend lives in
//! this crate -- everything here exists to support that shell (window
//! chrome, native menu, plugins the real dashboard may need later, e.g.
//! the native folder picker for the planned sandbox permission system).

mod utils;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Manager, RunEvent, WindowEvent};

/// Application entry point. Sets up all plugins and initializes the app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut app_builder = tauri::Builder::default();

    // Single instance plugin must be registered FIRST.
    // When the user tries to open a second instance, focus the existing window instead.
    #[cfg(desktop)]
    {
        app_builder = app_builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
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
        .on_page_load(|webview, _payload| {
            // Injects a small floating reload button into whatever page just
            // loaded. Fires on every navigation/reload (unlike a one-time
            // eval() in setup(), which would only run once -- the button
            // would vanish on the very first location.reload()). Guarded by
            // an id check so repeated fires (page load + any SPA re-render)
            // don't stack duplicate buttons.
            let _ = webview.eval(
                r#"
                (function () {
                    if (document.getElementById('tylluan-desktop-reload-btn')) return;
                    var btn = document.createElement('button');
                    btn.id = 'tylluan-desktop-reload-btn';
                    btn.title = 'Reload dashboard (Ctrl/Cmd+R also works)';
                    btn.textContent = '↻';
                    btn.onclick = function () { window.location.reload(); };
                    Object.assign(btn.style, {
                        position: 'fixed', bottom: '16px', right: '16px', zIndex: '2147483647',
                        width: '40px', height: '40px', borderRadius: '9999px', border: 'none',
                        background: 'rgba(20,20,24,0.75)', color: '#fff', fontSize: '20px',
                        cursor: 'pointer', boxShadow: '0 2px 8px rgba(0,0,0,0.35)',
                        display: 'flex', alignItems: 'center', justifyContent: 'center',
                    });
                    document.body.appendChild(btn);
                })();
                "#,
            );
        })
        .setup(|app| {
            log::info!("Tylluan Desktop starting up");

            // Native menu, built in Rust so it works regardless of what the
            // loaded page does. Just a Reload item for now: the dashboard has
            // no reload affordance of its own, and native window chrome
            // (decorations:true) doesn't provide a browser-style reload
            // button either.
            #[cfg(desktop)]
            {
                let reload_item = MenuItem::with_id(
                    app,
                    "reload",
                    "Reload",
                    true,
                    Some("CmdOrCtrl+R"),
                )?;
                let view_menu = Submenu::with_items(
                    app,
                    "View",
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
                    #[cfg(target_os = "macos")]
                    {
                        // macOS convention: hide instead of quitting, so the dock icon
                        // can reopen the window (RunEvent::Reopen below).
                        _api.prevent_close();
                        use tauri_plugin_window_state::{AppHandleExt, StateFlags};
                        if let Err(e) = _app_handle.save_window_state(StateFlags::all()) {
                            log::warn!("Failed to save window state: {e}");
                        }
                        if let Some(window) = _app_handle.get_webview_window("main") {
                            let _ = window.hide();
                        }
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
