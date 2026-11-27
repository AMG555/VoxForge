use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use voxforge_core::store::memory::MemoryStore;
use voxforge_engine::{
    EdgeTtsEngine, EngineRegistry, KokoroTtsEngine, OpenAiRouterEngine, PiperTtsEngine,
    Qwen3TtsEngine,
};
use voxforge_hardware::HardwareProbe;

use crate::{
    create_app,
    routes::{setup, system},
    AppState,
};

/// Configuration for booting the embedded VoxForge HTTP/WebSocket API server.
#[derive(Debug, Clone)]
pub struct EmbeddedServerConfig {
    pub host: String,
    pub port: u16,
    pub allow_port_fallback: bool,
    pub api_key: Option<String>,
    pub data_dir: Option<PathBuf>,
    pub router_url: Option<String>,
    pub router_api_key: Option<String>,
    pub router_model: Option<String>,
}

impl Default for EmbeddedServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8080,
            allow_port_fallback: true,
            api_key: None,
            data_dir: None,
            router_url: None,
            router_api_key: None,
            router_model: None,
        }
    }
}

/// Handle to a running embedded VoxForge HTTP server instance.
pub struct StartedServer {
    pub addr: SocketAddr,
    pub browser_url: String,
    pub data_dir: PathBuf,
    shutdown_tx: Option<oneshot::Sender<()>>,
    task_handle: Option<JoinHandle<Result<(), String>>>,
}

impl StartedServer {
    /// Signal the HTTP server to drain and shut down cleanly.
    pub fn signal_shutdown(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Trigger graceful shutdown and wait for the server task to exit.
    pub async fn shutdown(mut self) -> Result<(), String> {
        self.signal_shutdown();
        self.wait().await
    }

    /// Wait for the running server task to finish.
    pub async fn wait(&mut self) -> Result<(), String> {
        if let Some(handle) = self.task_handle.take() {
            match handle.await {
                Ok(res) => res,
                Err(e) => Err(format!("Server task join error: {e}")),
            }
        } else {
            Ok(())
        }
    }
}

/// Resolve the stable user data directory for VoxForge (`%APPDATA%\VoxForge` on Windows, `~/.voxforge` elsewhere).
pub fn resolve_default_data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let dir = PathBuf::from(appdata).join("VoxForge");
            let _ = std::fs::create_dir_all(&dir);
            return dir;
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let fallback = home.join(".voxforge");
    let _ = std::fs::create_dir_all(&fallback);
    fallback
}

/// Populate `VOXFORGE_DATA_DIR`, `VOXFORGE_MODELS_DIR`, and `VOXFORGE_PROFILES_DIR` if unset.
pub fn configure_data_directories(data_dir: &Path) -> (PathBuf, PathBuf) {
    let _ = std::fs::create_dir_all(data_dir);
    std::env::set_var("VOXFORGE_DATA_DIR", data_dir);

    let profiles_dir = std::env::var("VOXFORGE_PROFILES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| data_dir.join("profiles"));
    let models_dir = std::env::var("VOXFORGE_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| data_dir.join("models"));

    let _ = std::fs::create_dir_all(&profiles_dir);
    let _ = std::fs::create_dir_all(&models_dir);

    std::env::set_var("VOXFORGE_PROFILES_DIR", &profiles_dir);
    std::env::set_var("VOXFORGE_MODELS_DIR", &models_dir);

    (profiles_dir, models_dir)
}

/// Build and populate the multi-engine TTS registry.
pub async fn initialize_engine_registry(
    router_url: Option<String>,
    router_api_key: Option<String>,
    router_model: Option<String>,
) -> Arc<EngineRegistry> {
    let registry = Arc::new(EngineRegistry::new());
    registry.register(Arc::new(EdgeTtsEngine::new())).await;
    registry.register(Arc::new(Qwen3TtsEngine::default())).await;
    registry.register(Arc::new(PiperTtsEngine::new())).await;
    registry.register(Arc::new(KokoroTtsEngine::new())).await;

    let r_url = router_url.or_else(|| std::env::var("VOXFORGE_ROUTER_URL").ok());
    let r_key = router_api_key.or_else(|| std::env::var("VOXFORGE_ROUTER_API_KEY").ok());
    let r_model = router_model.or_else(|| std::env::var("VOXFORGE_ROUTER_MODEL").ok());

    let router_engine = if let Some(url) = r_url {
        OpenAiRouterEngine::new(url, r_key, r_model)
    } else {
        OpenAiRouterEngine::default_openai(r_key)
    };
    registry.register(Arc::new(router_engine)).await;

    registry
}

async fn bind_listener(
    host: &str,
    port: u16,
    allow_port_fallback: bool,
) -> Result<tokio::net::TcpListener, String> {
    match tokio::net::TcpListener::bind(format!("{host}:{port}")).await {
        Ok(l) => Ok(l),
        Err(e) if port == 8080 && allow_port_fallback => {
            warn!("Port 8080 busy ({e}). Trying alternative port...");
            for candidate in 8081..=8090 {
                if let Ok(l) = tokio::net::TcpListener::bind(format!("{host}:{candidate}")).await {
                    return Ok(l);
                }
            }
            Err(format!(
                "Failed to bind to port 8080 or fallback ports 8081-8090: {e}"
            ))
        }
        Err(e) => Err(format!("Failed to bind to {host}:{port}: {e}")),
    }
}

/// Boot the full VoxForge API + embedded UI server in a background Tokio task.
pub async fn start_embedded_server(
    config: EmbeddedServerConfig,
) -> Result<StartedServer, String> {
    system::cleanup_stale_update_backups();

    let data_dir = config.data_dir.unwrap_or_else(resolve_default_data_dir);
    let (profiles_dir, models_dir) = configure_data_directories(&data_dir);
    setup::apply_desktop_config_env();

    info!("Starting VoxForge Workstation Server v{}", env!("CARGO_PKG_VERSION"));
    info!("Profiles directory: {}", profiles_dir.display());
    info!("Models directory:   {}", models_dir.display());

    let hardware = HardwareProbe::probe();
    info!("Host Architecture: {} / {}", hardware.os, hardware.arch);
    info!("Assigned Hardware Tier: {}", hardware.assigned_tier);
    info!(
        "CPU: {} ({} threads)",
        hardware.cpu_brand, hardware.cpu_logical_threads
    );

    let registry = initialize_engine_registry(
        config.router_url,
        config.router_api_key,
        config.router_model,
    )
    .await;
    let store = Arc::new(MemoryStore::new());
    let state = AppState::new(registry, store, hardware, config.api_key.clone());
    let app = create_app(state);

    let listener = bind_listener(&config.host, config.port, config.allow_port_fallback).await?;
    let actual_addr = listener
        .local_addr()
        .map_err(|e| format!("Failed reading bound server address: {e}"))?;

    let browser_url = if config.host == "0.0.0.0" {
        format!("http://localhost:{}", actual_addr.port())
    } else {
        format!("http://{actual_addr}")
    };

    info!("Listening for API requests on http://{actual_addr}");
    if config.api_key.is_some() {
        info!("Authentication enabled (Bearer token enforced)");
    } else {
        info!("Open mode (No API key enforced)");
    }

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let task_handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .map_err(|e| format!("HTTP server error: {e}"))
    });

    Ok(StartedServer {
        addr: actual_addr,
        browser_url,
        data_dir,
        shutdown_tx: Some(shutdown_tx),
        task_handle: Some(task_handle),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_embedded_server_starts_and_shuts_down() {
        let tmp = std::env::temp_dir().join(format!(
            "voxforge_srv_test_{}",
            uuid::Uuid::new_v4().simple()
        ));
        let server = start_embedded_server(EmbeddedServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            allow_port_fallback: false,
            api_key: None,
            data_dir: Some(tmp.clone()),
            router_url: None,
            router_api_key: None,
            router_model: None,
        })
        .await
        .expect("embedded server should start on ephemeral port");

        assert_ne!(server.addr.port(), 0);
        let resp = reqwest::get(format!("{}/health", server.browser_url))
            .await
            .expect("health request should succeed");
        assert!(resp.status().is_success());

        server.shutdown().await.expect("server should shutdown cleanly");
        let _ = std::fs::remove_dir_all(tmp);
    }
}
