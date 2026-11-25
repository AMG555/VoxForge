use crate::state::AppState;
use axum::{
    extract::{Request, State},
    http::{header, HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use subtle::ConstantTimeEq;
use voxforge_core::error::ProblemDetails;

/// Thread-safe token-bucket rate limiter keyed by client address.
#[derive(Clone, Debug)]
pub struct RateLimiter {
    /// Requests per minute permitted per key. `0` disables limiting.
    rpm: u32,
    buckets: Arc<Mutex<HashMap<String, TokenBucket>>>,
}

#[derive(Clone, Copy, Debug)]
struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    /// Create a rate limiter allowing `rpm` requests per minute per client key (`0` = disabled).
    pub fn new(rpm: u32) -> Self {
        Self {
            rpm,
            buckets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Build a rate limiter from `VOXFORGE_RATE_LIMIT_RPM` (default: 600 RPM; `0` = disabled).
    pub fn from_env() -> Self {
        let rpm = std::env::var("VOXFORGE_RATE_LIMIT_RPM")
            .ok()
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(600);
        Self::new(rpm)
    }

    pub fn limit(&self) -> u32 {
        self.rpm
    }

    /// Check and consume one request token for `client_key`.
    /// Returns `(allowed, remaining_tokens, retry_after_secs)`.
    pub fn check(&self, client_key: &str) -> (bool, u32, u64) {
        if self.rpm == 0 {
            return (true, u32::MAX, 0);
        }

        let capacity = self.rpm as f64;
        let refill_per_sec = capacity / 60.0;
        let now = Instant::now();

        let mut map = match self.buckets.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        // Bound map cardinality against distributed IP floods
        if map.len() > 4096 && !map.contains_key(client_key) {
            map.retain(|_, b| now.duration_since(b.last_refill).as_secs() < 120);
            if map.len() > 4096 {
                map.clear();
            }
        }

        let bucket = map.entry(client_key.to_string()).or_insert(TokenBucket {
            tokens: capacity,
            last_refill: now,
        });

        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * refill_per_sec).min(capacity);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            (true, bucket.tokens.floor().max(0.0) as u32, 0)
        } else {
            let deficit = 1.0 - bucket.tokens;
            let retry_after = (deficit / refill_per_sec).ceil().max(1.0) as u64;
            (false, 0, retry_after)
        }
    }
}

fn extract_client_key(req: &Request) -> String {
    if let Some(xff) = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
    {
        if let Some(first) = xff
            .split(',')
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return first.to_string();
        }
    }
    if let Some(real_ip) = req.headers().get("x-real-ip").and_then(|h| h.to_str().ok()) {
        let trimmed = real_ip.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "local".to_string()
}

pub async fn rate_limit_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let limit = state.rate_limiter.limit();
    if limit == 0 {
        return next.run(req).await;
    }

    let path = req.uri().path();
    let is_rate_limited_route =
        path.starts_with("/v1/") || path == "/mcp" || path.starts_with("/mcp/");
    if !is_rate_limited_route {
        return next.run(req).await;
    }

    let client_key = extract_client_key(&req);
    let (allowed, remaining, retry_after) = state.rate_limiter.check(&client_key);

    if !allowed {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/rate-limited".to_string(),
            title: "Too Many Requests".to_string(),
            status: StatusCode::TOO_MANY_REQUESTS.as_u16(),
            detail: format!(
                "Rate limit exceeded ({limit} requests/minute). Retry after {retry_after}s."
            ),
            instance: path.to_string(),
        };
        let mut resp = (StatusCode::TOO_MANY_REQUESTS, Json(err)).into_response();
        let headers = resp.headers_mut();
        if let Ok(v) = HeaderValue::from_str(&retry_after.to_string()) {
            headers.insert(header::RETRY_AFTER, v);
        }
        if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
            headers.insert(HeaderName::from_static("x-ratelimit-limit"), v);
        }
        headers.insert(
            HeaderName::from_static("x-ratelimit-remaining"),
            HeaderValue::from_static("0"),
        );
        return resp;
    }

    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    if let Ok(v) = HeaderValue::from_str(&limit.to_string()) {
        headers.insert(HeaderName::from_static("x-ratelimit-limit"), v);
    }
    if let Ok(v) = HeaderValue::from_str(&remaining.to_string()) {
        headers.insert(HeaderName::from_static("x-ratelimit-remaining"), v);
    }
    response
}

pub async fn request_id_and_metrics(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let req_id = req
        .headers()
        .get("x-request-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let path = req.uri().path().to_string();
    state.metrics.inc_active_requests();

    let mut response = next.run(req).await;

    state.metrics.dec_active_requests();
    let status = response.status().as_u16();
    state.metrics.record_request(&path, status).await;

    if let Ok(val) = HeaderValue::from_str(&req_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static("x-request-id"), val);
    }

    response
}

pub async fn security_headers(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_string();
    let is_docs = path == "/docs" || path.starts_with("/docs/");
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::X_FRAME_OPTIONS,
        if is_docs {
            HeaderValue::from_static("SAMEORIGIN")
        } else {
            HeaderValue::from_static("DENY")
        },
    );
    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static("max-age=31536000; includeSubDomains"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        if is_docs {
            HeaderValue::from_static(
                "default-src 'self' https://cdn.jsdelivr.net; script-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://cdn.jsdelivr.net; font-src 'self' https://fonts.gstatic.com https://fonts.scalar.com data:; img-src 'self' data: https:; connect-src 'self' https:;"
            )
        } else {
            HeaderValue::from_static(
                "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com data:; media-src 'self' blob: data:; connect-src 'self' ws: wss: http: https:; img-src 'self' data: https:;"
            )
        },
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );

    response
}

pub async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ProblemDetails>)> {
    let expected_key = match &state.api_key {
        Some(key) if !key.is_empty() => key,
        _ => return Ok(next.run(req).await),
    };

    let path = req.uri().path();
    // Allow static UI assets, health checks, metrics, and documentation without API key
    let requires_auth = path.starts_with("/v1/") || path == "/mcp" || path.starts_with("/mcp/");
    if !requires_auth {
        return Ok(next.run(req).await);
    }

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    let api_key_header = req
        .headers()
        .get("x-api-key")
        .or_else(|| req.headers().get("x-voxforge-api-key"))
        .and_then(|h| h.to_str().ok());

    let query_token = req.uri().query().and_then(|q| {
        q.split('&').find_map(|pair| {
            let mut parts = pair.split('=');
            let key = parts.next()?;
            let val = parts.next()?;
            if key == "api_key" || key == "token" {
                Some(val)
            } else {
                None
            }
        })
    });

    let token = if let Some(auth) = auth_header {
        if let Some(stripped) = auth.strip_prefix("Bearer ") {
            Some(stripped)
        } else {
            Some(auth)
        }
    } else if let Some(key) = api_key_header {
        Some(key)
    } else {
        query_token
    };

    let is_authorized = match token {
        Some(t) => {
            let token_bytes = t.as_bytes();
            let expected_bytes = expected_key.as_bytes();
            token_bytes.ct_eq(expected_bytes).into()
        }
        None => false,
    };

    if is_authorized {
        Ok(next.run(req).await)
    } else {
        let err = ProblemDetails {
            problem_type: "https://voxforge.org/errors/unauthorized".to_string(),
            title: "Unauthorized".to_string(),
            status: StatusCode::UNAUTHORIZED.as_u16(),
            detail: "Invalid or missing Bearer API token or x-api-key".to_string(),
            instance: path.to_string(),
        };
        Err((StatusCode::UNAUTHORIZED, Json(err)))
    }
}

/// Protect local endpoints against DNS rebinding and malicious browser origins.
pub async fn host_validation(
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ProblemDetails>)> {
    if let Some(host) = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
    {
        let hostname = host.split(':').next().unwrap_or("").trim();

        let is_local = hostname == "localhost"
            || hostname == "127.0.0.1"
            || hostname == "[::1]"
            || hostname.ends_with(".localhost")
            || hostname.is_empty();

        let allowed_env = std::env::var("VOXFORGE_ALLOWED_HOSTS").unwrap_or_default();
        let is_allowed_by_env = !allowed_env.is_empty()
            && allowed_env
                .split(',')
                .any(|allowed| allowed.trim().eq_ignore_ascii_case(hostname));

        if !is_local && !is_allowed_by_env {
            let err = ProblemDetails {
                problem_type: "https://voxforge.org/errors/forbidden-host".to_string(),
                title: "Invalid Host Header".to_string(),
                status: StatusCode::FORBIDDEN.as_u16(),
                detail: format!(
                    "Host '{}' is not permitted. Blocked to prevent DNS rebinding attacks.",
                    hostname
                ),
                instance: req.uri().path().to_string(),
            };
            return Err((StatusCode::FORBIDDEN, Json(err)));
        }
    }

    Ok(next.run(req).await)
}
