#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, RunEvent,
};
use tauri_plugin_dialog::DialogExt;
use tokio::sync::oneshot;
use tracing::{error, info, Level};
use tracing_subscriber::FmtSubscriber;
use voxforge_api::server::{start_embedded_server, EmbeddedServerConfig, StartedServer};

struct DesktopState {
    server_base_url: String,
    data_dir: String,
    version: String,
    server: Mutex<Option<StartedServer>>,
}

#[derive(Serialize)]
struct DesktopContextInfo {
    is_desktop: bool,
    server_base_url: String,
    data_dir: String,
    version: String,
    platform: String,
}

#[tauri::command]
fn get_desktop_context(state: tauri::State<'_, DesktopState>) -> DesktopContextInfo {
    DesktopContextInfo {
        is_desktop: true,
        server_base_url: state.server_base_url.clone(),
        data_dir: state.data_dir.clone(),
        version: state.version.clone(),
        platform: std::env::consts::OS.to_string(),
    }
}

#[tauri::command]
async fn pick_runtime_directory(
    app: tauri::AppHandle,
    initial_path: Option<String>,
) -> Result<Option<String>, String> {
    let (tx, rx) = oneshot::channel();
    let mut builder = app
        .dialog()
        .file()
        .set_title("Choose VoxForge App Environment Folder");
    if let Some(ref p) = initial_path {
        let path = std::path::Path::new(p);
        if path.exists() {
            builder = builder.set_directory(path);
        }
    }
    builder.pick_folder(move |folder| {
        let resolved = folder
            .and_then(|f| f.into_path().ok())
            .map(|p| p.to_string_lossy().to_string());
        let _ = tx.send(resolved);
    });
    rx.await
        .map_err(|e| format!("Folder dialog cancelled: {e}"))
}

#[tauri::command]
async fn pick_python_executable(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = oneshot::channel();
    let mut builder = app.dialog().file().set_title("Select Python Interpreter");
    if cfg!(windows) {
        builder = builder.add_filter("Python Executable", &["exe"]);
    }
    builder.pick_file(move |file| {
        let resolved = file
            .and_then(|f| f.into_path().ok())
            .map(|p| p.to_string_lossy().to_string());
        let _ = tx.send(resolved);
    });
    rx.await.map_err(|e| format!("File dialog cancelled: {e}"))
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err("Only http:// and https:// URLs are allowed".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("cmd")
            .args(["/C", "start", "", trimmed])
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|e| format!("Failed opening URL: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| format!("Failed opening URL: {e}"))?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| format!("Failed opening URL: {e}"))?;
    }
    Ok(())
}

async fn boot_backend() -> Result<StartedServer, String> {
    let primary = EmbeddedServerConfig {
        host: "127.0.0.1".to_string(),
        port: 8080,
        allow_port_fallback: true,
        ..Default::default()
    };
    match start_embedded_server(primary).await {
        Ok(srv) => Ok(srv),
        Err(_) => {
            start_embedded_server(EmbeddedServerConfig {
                host: "127.0.0.1".to_string(),
                port: 0,
                allow_port_fallback: false,
                ..Default::default()
            })
            .await
        }
    }
}

fn main() {
    if std::env::args().any(|arg| arg == "--version" || arg == "-V") {
        println!("voxforge-desktop {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).ok();

    let started = tauri::async_runtime::block_on(boot_backend()).unwrap_or_else(|e| {
        error!("Failed launching embedded VoxForge backend: {e}");
        std::process::exit(1);
    });

    info!(
        "VoxForge Desktop connected to embedded backend at {}",
        started.browser_url
    );

    let desktop_state = DesktopState {
        server_base_url: started.browser_url.clone(),
        data_dir: started.data_dir.to_string_lossy().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        server: Mutex::new(Some(started)),
    };

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(desktop_state)
        .invoke_handler(tauri::generate_handler![
            get_desktop_context,
            pick_runtime_directory,
            pick_python_executable,
            open_external_url
        ])
        .setup(|app| {
            let show_item = MenuItem::with_id(app, "show", "Show VoxForge", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit VoxForge", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            if let Some(icon) = app.default_window_icon().cloned() {
                TrayIconBuilder::new()
                    .icon(icon)
                    .menu(&menu)
                    .tooltip("VoxForge — Neural Speech Studio")
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(win) = app.get_webview_window("main") {
                                let _ = win.show();
                                let _ = win.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .build(app)?;
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error building VoxForge desktop application");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            if let Some(state) = app_handle.try_state::<DesktopState>() {
                if let Ok(mut guard) = state.server.lock() {
                    if let Some(mut srv) = guard.take() {
                        srv.signal_shutdown();
                    }
                }
            }
        }
    });
}
