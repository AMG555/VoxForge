use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        Response,
    },
};
use futures::stream::{self, Stream};
use std::convert::Infallible;

use crate::state::AppState;

/// Streamable HTTP POST endpoint for Model Context Protocol (MCP) JSON-RPC 2.0 requests.
/// Allows Claude Desktop, Cursor, Antigravity, and Windsurf to connect over HTTP directly.
///
/// The [`McpServer`] is built once at startup and stored in [`AppState`] — reusing it here
/// avoids per-request engine re-registration and hardware re-probing (was ~6 Arc allocs +
/// multiple Mutex write-locks on every call).
pub async fn handle_mcp_post(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<Response, StatusCode> {
    let client_id = headers
        .get("x-voxforge-client-id")
        .or_else(|| headers.get("x-client-id"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("generic-agent");

    tracing::debug!(client_id, "Handling incoming MCP request over HTTP");

    if let Some(res_str) = state.mcp_server.handle_message(&body).await {
        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .header("x-voxforge-client-id", client_id)
            .body(Body::from(res_str))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    } else {
        // Notification (e.g. notifications/initialized) - return 204 No Content
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header("x-voxforge-client-id", client_id)
            .body(Body::empty())
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }
}

/// Server-Sent Events (SSE) stream endpoint for MCP event subscriptions.
pub async fn handle_mcp_sse(
    State(_state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = stream::once(async { Ok(Event::default().event("endpoint").data("/mcp")) });

    Sse::new(stream).keep_alive(KeepAlive::default())
}
