use std::collections::HashMap;
use std::time::Duration;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use tokio::net::TcpStream;

use crate::state::AppState;

pub async fn health_check() -> Json<Value> {
    // Probe neural sidecar backend if running (non-blocking, 200ms timeout)
    let sidecar_info = probe_sidecar_backend().await;
    Json(json!({
        "status": "healthy",
        "service": "voxforge",
        "version": env!("CARGO_PKG_VERSION"),
        "neural_sidecar": sidecar_info,
    }))
}

/// Non-blocking probe of the neural sidecar backend via TCP connect.
///
/// Tries each well-known sidecar port (OmniVoice, F5-TTS, CosyVoice2, Kokoro).
/// Returns `{"running": true, "url": "http://127.0.0.1:<port>"}` for the first
/// reachable port, or `{"running": false}` if none respond within 200ms.
/// Uses Tokio `TcpStream::connect` — no extra HTTP client crate needed.
async fn probe_sidecar_backend() -> Value {
    // Well-known sidecar ports: OmniVoice=9000, F5-TTS=8880, CosyVoice2=9880,
    // Kokoro=8000, generic neural sidecar=5000
    let ports = [9000u16, 8880, 9880, 8000, 5000];
    let timeout = Duration::from_millis(200);

    for port in ports {
        let addr = format!("127.0.0.1:{}", port);
        if tokio::time::timeout(timeout, TcpStream::connect(&addr))
            .await
            .is_ok_and(|r| r.is_ok())
        {
            return json!({
                "running": true,
                "url": format!("http://127.0.0.1:{}", port),
            });
        }
    }

    json!({ "running": false })
}

pub async fn readiness_check(State(state): State<AppState>) -> Response {
    let engine_ids = state.engine_registry.list_engines().await;
    let mut health_map = HashMap::new();
    let mut healthy_count = 0;
    let total_count = engine_ids.len();

    let check_futures = engine_ids.iter().map(|id| {
        let registry = state.engine_registry.clone();
        let id = id.clone();
        async move {
            if let Some(engine) = registry.get(&id).await {
                let status = match engine.health_check().await {
                    Ok(true) => "healthy",
                    Ok(false) => "circuit_open",
                    Err(_) => "unreachable",
                };
                (id, Some(status))
            } else {
                (id, None)
            }
        }
    });

    let results = futures::future::join_all(check_futures).await;
    for (id, status) in results {
        if let Some(s) = status {
            if s == "healthy" {
                healthy_count += 1;
            }
            health_map.insert(id, s);
        }
    }

    let overall_status = if total_count == 0 || healthy_count == total_count {
        "ready"
    } else if healthy_count > 0 {
        "degraded"
    } else {
        "unhealthy"
    };

    let status_code = if overall_status == "unhealthy" && total_count > 0 {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };

    let body = json!({
        "status": overall_status,
        "healthy_engines": healthy_count,
        "total_engines": total_count,
        "registered_engines": engine_ids,
        "engine_health": health_map,
        "hardware_tier": state.hardware_profile.assigned_tier.to_string(),
        "arch": state.hardware_profile.arch,
        "os": state.hardware_profile.os,
        "hardware_profile": &state.hardware_profile
    });

    (status_code, Json(body)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use voxforge_core::store::memory::MemoryStore;
    use voxforge_engine::{EngineRegistry, MockTtsEngine};
    use voxforge_hardware::HardwareProbe;

    #[tokio::test]
    async fn test_health_check_payload() {
        let res = health_check().await;
        assert_eq!(res.0["status"], "healthy");
        assert_eq!(res.0["service"], "voxforge");
        assert!(!res.0["version"].as_str().unwrap().is_empty());
        // neural_sidecar field must be present (running: true or false)
        assert!(res.0.get("neural_sidecar").is_some());
    }

    #[tokio::test]
    async fn test_readiness_check_concurrent_evaluation() {
        let registry = Arc::new(EngineRegistry::new());
        let mock_engine = Arc::new(MockTtsEngine::new(24000));
        registry.register(mock_engine).await;

        let store = Arc::new(MemoryStore::new());
        let hardware = HardwareProbe::probe();
        let state = AppState::new(registry, store, hardware, None);

        let resp = readiness_check(State(state)).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
