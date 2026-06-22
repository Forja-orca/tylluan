// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandChild;
use tracing::{info, warn, error};
use serde::Serialize;

/// AppState manages the state of the sidecar process.
struct AppState {
    /// Holds the child process if we spawned it ourselves.
    sidecar_child: Arc<Mutex<Option<CommandChild>>>,
    /// True if we attached to an existing running process (so we leave it alive on exit).
    attached: Arc<Mutex<bool>>,
}

#[derive(Serialize)]
struct SystemStatus {
    status: String,
    kernel_version: String,
}

#[tauri::command]
async fn get_system_status(state: State<'_, AppState>) -> Result<SystemStatus, String> {
    let attached = *state.attached.lock().await;
    Ok(SystemStatus {
        status: if attached { "Attached (External)".into() } else { "Operational (Sidecar)".into() },
        kernel_version: "3.0.0".into(),
    })
}

#[tauri::command]
async fn check_model_files() -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "vision": false,
        "embeddings": true,
        "tts": false,
        "instructions": false
    }))
}

#[tauri::command]
async fn list_guilds() -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let res = client.get("http://127.0.0.1:3030/api/v1/guilds")
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let json = res.json::<serde_json::Value>().await.map_err(|e| e.to_string())?;
    Ok(json)
}

fn setup_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let show_i = MenuItem::with_id(app, "show", "Show Dashboard", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .on_menu_event(move |app_handle: &AppHandle, event| {
            match event.id.as_ref() {
                "quit" => {
                    app_handle.exit(0);
                }
                "show" => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

fn main() {
    let state = AppState {
        sidecar_child: Arc::new(Mutex::new(None)),
        attached: Arc::new(Mutex::new(false)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(state)
        .setup(|app| {
            let handle = app.handle().clone();
            setup_tray(app)?;

            // Background task for spawn-or-attach and health monitoring
            tauri::async_runtime::spawn(async move {
                let state = handle.state::<AppState>();
                let client = reqwest::Client::new();
                
                info!("🔍 Checking if TylluanNexus kernel is already running on port 3030...");
                let mut already_running = false;
                for _ in 0..6 { // Poll 6 times (every 500ms) = 3s total
                    if let Ok(res) = client.get("http://127.0.0.1:3030/health").send().await {
                        if res.status().is_success() {
                            already_running = true;
                            break;
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }

                if already_running {
                    info!("🔌 Attached to already running TylluanNexus kernel on 127.0.0.1:3030.");
                    *state.attached.lock().await = true;
                } else {
                    info!("🚀 No active kernel found. Spawning sidecar tylluan-nexus...");
                    match handle.shell().sidecar("tylluan-nexus") {
                        Ok(sidecar) => {
                            match sidecar.spawn() {
                                Ok((_rx, child)) => {
                                    info!("✅ Sidecar spawned successfully.");
                                    *state.sidecar_child.lock().await = Some(child);
                                }
                                Err(e) => {
                                    error!("❌ Failed to spawn tylluan-nexus sidecar: {:?}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("❌ Sidecar 'tylluan-nexus' configuration error: {:?}", e);
                        }
                    }
                }

                // Poll health endpoint for up to 30s to update window state/title
                let main_window = handle.get_webview_window("main");
                let mut operational = false;
                for i in 0..60 { // 60 * 500ms = 30s
                    if let Ok(res) = client.get("http://127.0.0.1:3030/health").send().await {
                        if res.status().is_success() {
                            operational = true;
                            break;
                        }
                    }
                    if let Some(ref win) = main_window {
                        let _ = win.set_title(&format!("TylluanNexus - Starting... ({}s)", i / 2));
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }

                if let Some(ref win) = main_window {
                    if operational {
                        let _ = win.set_title("TylluanNexus - Operational");
                    } else {
                        let _ = win.set_title("TylluanNexus - Startup Error");
                    }
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Prevent immediate exit, handle graceful shutdown of sidecar first
                api.prevent_close();
                let handle = window.app_handle().clone();
                
                tauri::async_runtime::spawn(async move {
                    let state = handle.state::<AppState>();
                    let is_attached = *state.attached.lock().await;

                    if is_attached {
                        info!("🔌 Attached kernel left running. Closing window...");
                    } else {
                        info!("🛑 Spawner mode: Initiating graceful shutdown of sidecar...");
                        let client = reqwest::Client::new();
                        
                        // Send shutdown command
                        let _ = client.post("http://127.0.0.1:3030/api/v1/admin/shutdown").send().await;
                        
                        // Poll health endpoint for up to 10s waiting for exit
                        let mut exited = false;
                        for _ in 0..20 { // 20 * 500ms = 10s
                            match client.get("http://127.0.0.1:3030/health").send().await {
                                Err(_) => {
                                    exited = true;
                                    break;
                                }
                                _ => {}
                            }
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }

                        if exited {
                            info!("✅ Sidecar exited gracefully.");
                        } else {
                            warn!("⚠️ Sidecar did not exit gracefully within 10s. Force terminating...");
                            let mut lock = state.sidecar_child.lock().await;
                            if let Some(child) = lock.take() {
                                if let Err(e) = child.kill() {
                                    error!("❌ Failed to kill sidecar process: {:?}", e);
                                }
                            }
                        }
                    }

                    // Exit the app
                    handle.exit(0);
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_system_status,
            check_model_files,
            list_guilds
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
