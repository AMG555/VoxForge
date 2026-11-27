use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use axum::{extract::Query, http::StatusCode, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use voxforge_core::error::ProblemDetails;

const DEFAULT_RELEASE_API_URL: &str =
    "https://api.github.com/repos/AMG555/VoxForge/releases/latest";
const UPDATE_CACHE_TTL: Duration = Duration::from_secs(900);
const MAX_UPDATE_BINARY_BYTES: usize = 128 * 1024 * 1024;

static UPDATE_CACHE: OnceLock<RwLock<Option<(Instant, String, UpdateCheckResponse)>>> =
    OnceLock::new();
static APPLIED_UPDATE: OnceLock<RwLock<Option<UpdateApplyResponse>>> = OnceLock::new();

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateCheckQuery {
    pub force: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateApplyRequest {
    pub auto_restart: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResponse {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub latest_tag: Option<String>,
    pub update_available: bool,
    pub check_status: String,
    pub release_name: Option<String>,
    pub release_url: String,
    pub release_notes: Option<String>,
    pub published_at: Option<String>,
    pub checked_at: String,
    pub platform: String,
    pub arch: String,
    pub asset_name: String,
    pub asset_url: Option<String>,
    pub asset_size_bytes: Option<u64>,
    pub checksum_sha256: Option<String>,
    pub checksums_url: Option<String>,
    pub install_mode: String,
    pub executable_path: String,
    pub can_self_update: bool,
    pub update_hint: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateApplyResponse {
    pub status: String,
    pub previous_version: String,
    pub new_version: String,
    pub asset_name: String,
    pub sha256_verified: bool,
    pub sha256: String,
    pub executable_path: String,
    pub restart_required: bool,
    pub restarting: bool,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubReleaseAsset {
    name: String,
    size: u64,
    browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubReleasePayload {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    assets: Vec<GitHubReleaseAsset>,
}

fn cache_store() -> &'static RwLock<Option<(Instant, String, UpdateCheckResponse)>> {
    UPDATE_CACHE.get_or_init(|| RwLock::new(None))
}

/// Parse semantic version into `(major, minor, patch, is_prerelease)`.
pub fn parse_semver(raw: &str) -> Option<(u64, u64, u64, bool)> {
    let trimmed = raw.trim().trim_start_matches(['v', 'V']);
    if trimmed.is_empty() {
        return None;
    }
    let (core, is_pre) = match trimmed.split_once('-') {
        Some((base, pre)) => (base, !pre.is_empty()),
        None => (trimmed, false),
    };
    let core = core.split('+').next().unwrap_or(core);
    let mut parts = core.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    Some((major, minor, patch, is_pre))
}

/// Returns `true` strictly when `candidate` is newer than `current`.
pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    match (parse_semver(current), parse_semver(candidate)) {
        (Some((cmaj, cmin, cpat, cpre)), Some((nmaj, nmin, npat, npre))) => {
            if (nmaj, nmin, npat) != (cmaj, cmin, cpat) {
                return (nmaj, nmin, npat) > (cmaj, cmin, cpat);
            }
            cpre && !npre
        }
        _ => false,
    }
}

/// Extract expected lowercase SHA-256 hex digest for `asset_name` from `checksums.txt`.
pub fn parse_checksums_txt(content: &str, asset_name: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut split = trimmed.split_whitespace();
        let hash = match split.next() {
            Some(h) if h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()) => {
                h.to_ascii_lowercase()
            }
            _ => continue,
        };
        let fname = match split.next() {
            Some(f) => f.trim_start_matches('*').trim_start_matches("./"),
            None => continue,
        };
        if fname == asset_name {
            return Some(hash);
        }
    }
    None
}

fn is_desktop_executable(exe_path: &Path) -> bool {
    exe_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase().contains("voxforge-desktop"))
        .unwrap_or(false)
}

/// Map runtime OS/CPU architecture to release asset preference list.
pub fn resolve_platform_asset_candidates(
    os: &str,
    arch: &str,
) -> (&'static str, &'static [&'static str]) {
    resolve_platform_asset_candidates_for_binary(os, arch, false)
}

pub fn resolve_platform_asset_candidates_for_binary(
    os: &str,
    arch: &str,
    is_desktop: bool,
) -> (&'static str, &'static [&'static str]) {
    if is_desktop {
        return match (os, arch) {
            ("windows", "x86_64" | "aarch64") => (
                "voxforge-desktop-windows-x86_64.exe",
                &[
                    "voxforge-desktop-windows-x86_64.exe",
                    "voxforge-desktop.exe",
                    "voxforge-windows-x86_64.exe",
                ],
            ),
            ("macos", "aarch64" | "x86_64") => (
                "voxforge-desktop-macos-universal",
                &["voxforge-desktop-macos-universal", "voxforge-macos-universal"],
            ),
            ("linux", "x86_64") => (
                "voxforge-desktop-linux-x86_64",
                &["voxforge-desktop-linux-x86_64", "voxforge-linux-x86_64"],
            ),
            _ => ("voxforge-desktop", &["voxforge-desktop", "voxforge"]),
        };
    }

    match (os, arch) {
        ("windows", "x86_64" | "aarch64") => (
            "voxforge-windows-x86_64.exe",
            &["voxforge-windows-x86_64.exe", "voxforge.exe"],
        ),
        ("macos", "aarch64") => (
            "voxforge-macos-universal",
            &[
                "voxforge-macos-universal",
                "voxforge-macos-aarch64",
                "voxforge-macos-arm64",
            ],
        ),
        ("macos", "x86_64") => (
            "voxforge-macos-universal",
            &["voxforge-macos-universal", "voxforge-macos-x86_64"],
        ),
        ("linux", "x86_64") => ("voxforge-linux-x86_64", &["voxforge-linux-x86_64"]),
        ("linux", "aarch64") => ("voxforge-linux-aarch64", &["voxforge-linux-aarch64"]),
        _ => ("voxforge", &["voxforge"]),
    }
}

/// Classify whether the current executable is a Cargo dev build, Windows installer, or standalone binary.
pub fn detect_install_mode(exe_path: &Path) -> (&'static str, bool) {
    let norm = exe_path.to_string_lossy().replace('\\', "/").to_lowercase();
    if norm.contains("/target/debug/")
        || norm.contains("/target/release/")
        || norm.contains("/target/release-small/")
    {
        return ("cargo_dev", false);
    }

    let parent_writable = exe_path.parent().map(is_dir_writable).unwrap_or(false);

    if cfg!(windows)
        && (norm.contains("/program files/")
            || norm.ends_with("/voxforge/voxforge.exe")
            || norm.ends_with("/voxforge/voxforge-desktop.exe"))
    {
        return ("windows_installer", parent_writable);
    }

    ("standalone_binary", parent_writable)
}

fn is_dir_writable(dir: &Path) -> bool {
    if !dir.exists() {
        return false;
    }
    let probe = dir.join(format!(
        ".voxforge_write_probe_{}",
        uuid::Uuid::new_v4().simple()
    ));
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Enforce strict host allowlist on release binary downloads to prevent SSRF or redirect hijacking.
pub fn is_trusted_release_download_url(download_url: &str, api_url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(download_url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("");
    let path = parsed.path();

    if parsed.scheme() == "https"
        && ((host.eq_ignore_ascii_case("github.com")
            && path.starts_with("/AMG555/VoxForge/releases/"))
            || host.eq_ignore_ascii_case("objects.githubusercontent.com")
            || host.eq_ignore_ascii_case("release-assets.githubusercontent.com"))
    {
        return true;
    }

    // Permit exact same origin as configured VOXFORGE_RELEASE_API_URL (enterprise mirrors / mock tests)
    if api_url != DEFAULT_RELEASE_API_URL {
        if let Ok(api_parsed) = reqwest::Url::parse(api_url) {
            return parsed.scheme() == api_parsed.scheme()
                && parsed.host_str() == api_parsed.host_str()
                && parsed.port_or_known_default() == api_parsed.port_or_known_default();
        }
    }

    false
}

fn applied_update_store() -> &'static RwLock<Option<UpdateApplyResponse>> {
    APPLIED_UPDATE.get_or_init(|| RwLock::new(None))
}

/// Remove `.old` backup left behind after a Windows self-update swap.
pub fn cleanup_stale_update_backups() {
    if let Ok(exe) = std::env::current_exe() {
        let backup = backup_path_for(&exe);
        if backup.exists() {
            let _ = std::fs::remove_file(&backup);
        }
        if let (Some(parent), Some(file_name)) = (
            exe.parent(),
            exe.file_name().and_then(|n| n.to_str()),
        ) {
            let prefix = format!("{file_name}.");
            if let Ok(entries) = std::fs::read_dir(parent) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    if name_str.starts_with(&prefix) && name_str.ends_with(".old") {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }
}

fn backup_path_for(exe: &Path) -> PathBuf {
    let file_name = exe
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("voxforge");
    exe.with_file_name(format!("{file_name}.old"))
}

fn release_api_url() -> String {
    std::env::var("VOXFORGE_RELEASE_API_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_RELEASE_API_URL.to_string())
}

fn is_update_check_disabled() -> bool {
    matches!(
        std::env::var("VOXFORGE_DISABLE_UPDATE_CHECK")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
}

fn build_update_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("VoxForge/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| format!("Failed initializing update HTTP client: {e}"))
}

fn build_base_response(check_status: &str, error: Option<String>) -> UpdateCheckResponse {
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();
    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("voxforge"));
    let is_desktop = is_desktop_executable(&exe_path);
    let (primary_asset, _) = resolve_platform_asset_candidates_for_binary(&os, &arch, is_desktop);
    let (install_mode, writable) = detect_install_mode(&exe_path);

    let update_hint = match install_mode {
        "cargo_dev" => {
            "Running from Cargo build directory. Run `git pull && npm run ui:build && cargo build --release --bin voxforge` or install a release binary.".to_string()
        }
        mode if writable => format!("In-place SHA-256 verified update supported ({mode})."),
        _ => "Install directory requires elevated permissions; run the one-line installer or download the setup package.".to_string(),
    };

    UpdateCheckResponse {
        current_version: env!("CARGO_PKG_VERSION").to_string(),
        latest_version: None,
        latest_tag: None,
        update_available: false,
        check_status: check_status.to_string(),
        release_name: None,
        release_url: "https://github.com/AMG555/VoxForge/releases/latest".to_string(),
        release_notes: None,
        published_at: None,
        checked_at: Utc::now().to_rfc3339(),
        platform: os,
        arch,
        asset_name: primary_asset.to_string(),
        asset_url: None,
        asset_size_bytes: None,
        checksum_sha256: None,
        checksums_url: None,
        install_mode: install_mode.to_string(),
        executable_path: exe_path.display().to_string(),
        can_self_update: writable && install_mode != "cargo_dev",
        update_hint,
        error,
    }
}

/// Query GitHub Releases for newer versions, using a 15-minute in-memory TTL cache unless `force` is true.
pub async fn check_for_updates(force: bool) -> UpdateCheckResponse {
    if is_update_check_disabled() {
        return build_base_response(
            "disabled",
            Some("Release update checks disabled via VOXFORGE_DISABLE_UPDATE_CHECK".to_string()),
        );
    }

    let api_url = release_api_url();
    if !force {
        let guard = cache_store().read().await;
        if let Some((cached_at, ref cached_api, ref cached_resp)) = *guard {
            if cached_api == &api_url && cached_at.elapsed() < UPDATE_CACHE_TTL {
                return cached_resp.clone();
            }
        }
    }

    let fresh = check_for_updates_from_url(&api_url).await;
    if fresh.check_status == "ok" {
        let mut guard = cache_store().write().await;
        *guard = Some((Instant::now(), api_url, fresh.clone()));
    }
    fresh
}

pub async fn check_for_updates_from_url(api_url: &str) -> UpdateCheckResponse {
    let mut resp = build_base_response("ok", None);
    let client = match build_update_client() {
        Ok(c) => c,
        Err(e) => {
            resp.check_status = "offline".to_string();
            resp.error = Some(e);
            return resp;
        }
    };

    let mut req = client
        .get(api_url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json");
    if let Ok(token) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")) {
        let t = token.trim();
        if !t.is_empty() {
            req = req.bearer_auth(t);
        }
    }

    let http_res = match req.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("Update check failed ({api_url}): {e}");
            resp.check_status = "offline".to_string();
            resp.error = Some(format!("Unable to reach release endpoint: {e}"));
            return resp;
        }
    };

    if http_res.status() == StatusCode::FORBIDDEN
        || http_res.status() == StatusCode::TOO_MANY_REQUESTS
    {
        resp.check_status = "rate_limited".to_string();
        resp.error = Some(
            "GitHub API rate limit reached. Try again later or open Releases directly.".to_string(),
        );
        return resp;
    }

    if !http_res.status().is_success() {
        resp.check_status = "offline".to_string();
        resp.error = Some(format!(
            "Release server returned HTTP {}",
            http_res.status()
        ));
        return resp;
    }

    let release: GitHubReleasePayload = match http_res.json().await {
        Ok(p) => p,
        Err(e) => {
            resp.check_status = "offline".to_string();
            resp.error = Some(format!("Malformed release metadata: {e}"));
            return resp;
        }
    };

    let clean_latest = release.tag_name.trim_start_matches(['v', 'V']).to_string();
    resp.latest_tag = Some(release.tag_name.clone());
    resp.latest_version = Some(clean_latest.clone());
    resp.update_available = is_newer_version(&resp.current_version, &clean_latest);
    resp.release_name = release
        .name
        .clone()
        .or_else(|| Some(release.tag_name.clone()));
    if let Some(url) = release.html_url {
        resp.release_url = url;
    }
    resp.release_notes = release.body.map(|b| b.chars().take(1600).collect());
    resp.published_at = release.published_at;

    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("voxforge"));
    let (_, candidates) = resolve_platform_asset_candidates_for_binary(
        &resp.platform,
        &resp.arch,
        is_desktop_executable(&exe_path),
    );
    let selected_asset = candidates
        .iter()
        .find_map(|name| release.assets.iter().find(|a| a.name == *name));

    if let Some(asset) = selected_asset {
        resp.asset_name = asset.name.clone();
        resp.asset_url = Some(asset.browser_download_url.clone());
        resp.asset_size_bytes = Some(asset.size);
    }

    if let Some(sums_asset) = release.assets.iter().find(|a| a.name == "checksums.txt") {
        resp.checksums_url = Some(sums_asset.browser_download_url.clone());
        if is_trusted_release_download_url(&sums_asset.browser_download_url, api_url) {
            if let Ok(sums_res) = client.get(&sums_asset.browser_download_url).send().await {
                if sums_res.status().is_success() {
                    if let Ok(sums_txt) = sums_res.text().await {
                        resp.checksum_sha256 = parse_checksums_txt(&sums_txt, &resp.asset_name);
                    }
                }
            }
        }
    }

    resp
}

/// Download the latest verified release binary, verify SHA-256 and execute health check, then swap atomically.
pub async fn download_verify_and_install_update(
) -> Result<UpdateApplyResponse, (StatusCode, String)> {
    if let Some(existing) = applied_update_store().read().await.clone() {
        return Ok(existing);
    }

    let api_url = release_api_url();
    let check = check_for_updates(true).await;
    if check.check_status != "ok" {
        return Err((
            StatusCode::BAD_GATEWAY,
            check
                .error
                .unwrap_or_else(|| "Release lookup failed".to_string()),
        ));
    }

    let latest_version = check
        .latest_version
        .clone()
        .unwrap_or_else(|| check.current_version.clone());

    if check.install_mode == "cargo_dev" {
        return Ok(UpdateApplyResponse {
            status: "dev_build".to_string(),
            previous_version: check.current_version.clone(),
            new_version: latest_version,
            asset_name: check.asset_name,
            sha256_verified: false,
            sha256: check.checksum_sha256.unwrap_or_default(),
            executable_path: check.executable_path,
            restart_required: false,
            restarting: false,
            message: check.update_hint,
        });
    }

    if !check.update_available {
        return Ok(UpdateApplyResponse {
            status: "up_to_date".to_string(),
            previous_version: check.current_version.clone(),
            new_version: check.current_version,
            asset_name: check.asset_name,
            sha256_verified: false,
            sha256: check.checksum_sha256.unwrap_or_default(),
            executable_path: check.executable_path,
            restart_required: false,
            restarting: false,
            message: "VoxForge is already running the latest release.".to_string(),
        });
    }

    if !check.can_self_update {
        return Err((StatusCode::FORBIDDEN, check.update_hint));
    }

    let asset_url = check.asset_url.clone().ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!(
                "No pre-built asset '{}' found in release {}",
                check.asset_name,
                check.latest_tag.as_deref().unwrap_or("latest")
            ),
        )
    })?;

    if !is_trusted_release_download_url(&asset_url, &api_url) {
        return Err((
            StatusCode::FORBIDDEN,
            format!("Blocked untrusted download host: {asset_url}"),
        ));
    }

    let expected_sha = check.checksum_sha256.clone().ok_or_else(|| {
        (
            StatusCode::BAD_GATEWAY,
            format!(
                "Release checksums.txt missing SHA-256 entry for '{}'; aborting unverified update.",
                check.asset_name
            ),
        )
    })?;

    let client = reqwest::Client::builder()
        .user_agent(concat!("VoxForge/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("HTTP client error: {e}"),
            )
        })?;

    let mut resp = client.get(&asset_url).send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed downloading release binary: {e}"),
        )
    })?;

    if !resp.status().is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!("Binary download returned HTTP {}", resp.status()),
        ));
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Stream download failed: {e}"),
        )
    })? {
        if bytes.len() + chunk.len() > MAX_UPDATE_BINARY_BYTES {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                "Release binary exceeded 128 MB safety cap".to_string(),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }

    let actual_sha = hex::encode(Sha256::digest(&bytes));
    if actual_sha != expected_sha {
        return Err((
            StatusCode::BAD_GATEWAY,
            format!(
                "SHA-256 integrity mismatch for '{}': expected {expected_sha}, got {actual_sha}",
                check.asset_name
            ),
        ));
    }

    let exe_path = std::env::current_exe().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Cannot locate executable: {e}"),
        )
    })?;

    apply_binary_bytes_atomically(&exe_path, &bytes)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let response = UpdateApplyResponse {
        status: "applied".to_string(),
        previous_version: check.current_version,
        new_version: latest_version.clone(),
        asset_name: check.asset_name,
        sha256_verified: true,
        sha256: actual_sha,
        executable_path: exe_path.display().to_string(),
        restart_required: true,
        restarting: false,
        message: format!(
            "Verified SHA-256 and installed VoxForge v{latest_version}. Restart VoxForge to activate."
        ),
    };
    *applied_update_store().write().await = Some(response.clone());
    Ok(response)
}

async fn apply_binary_bytes_atomically(exe_path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = exe_path
        .parent()
        .ok_or_else(|| "Executable has no parent directory".to_string())?;
    let staged = parent.join(format!(
        ".voxforge_update_{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));

    tokio::fs::write(&staged, bytes)
        .await
        .map_err(|e| format!("Failed writing staged binary: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755)).map_err(|e| {
            let _ = std::fs::remove_file(&staged);
            format!("Failed setting executable permissions: {e}")
        })?;
    }

    // Verify downloaded binary executes cleanly before replacing active binary.
    let probe = tokio::process::Command::new(&staged)
        .arg("--version")
        .output()
        .await;
    match probe {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let _ = std::fs::remove_file(&staged);
            return Err(format!(
                "Staged binary verification exited with status {}",
                out.status
            ));
        }
        Err(e) => {
            let _ = std::fs::remove_file(&staged);
            return Err(format!("Failed executing staged binary health check: {e}"));
        }
    }

    #[cfg(windows)]
    {
        let mut backup = backup_path_for(exe_path);
        if backup.exists() && std::fs::remove_file(&backup).is_err() {
            let file_name = exe_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("voxforge.exe");
            let tag = &uuid::Uuid::new_v4().simple().to_string()[..8];
            backup = exe_path.with_file_name(format!("{file_name}.{tag}.old"));
        }
        std::fs::rename(exe_path, &backup).map_err(|e| {
            let _ = std::fs::remove_file(&staged);
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                format!(
                    "Failed moving active executable to backup: {e}. Close any other running VoxForge instances or run the one-line installer as Administrator."
                )
            } else {
                format!("Failed moving active executable to backup: {e}")
            }
        })?;
        if let Err(e) = std::fs::rename(&staged, exe_path) {
            let _ = std::fs::rename(&backup, exe_path);
            let _ = std::fs::remove_file(&staged);
            return Err(format!("Failed activating updated executable: {e}"));
        }
    }

    #[cfg(not(windows))]
    {
        std::fs::rename(&staged, exe_path).map_err(|e| {
            let _ = std::fs::remove_file(&staged);
            format!("Failed replacing executable atomically: {e}")
        })?;
    }

    Ok(())
}

/// GET /v1/system/update
pub async fn get_update_status(Query(query): Query<UpdateCheckQuery>) -> Json<UpdateCheckResponse> {
    Json(check_for_updates(query.force.unwrap_or(false)).await)
}

/// POST /v1/system/update/apply
pub async fn apply_update_handler(
    body: axum::body::Bytes,
) -> Result<Json<UpdateApplyResponse>, (StatusCode, Json<ProblemDetails>)> {
    let req: UpdateApplyRequest = if body.is_empty() {
        UpdateApplyRequest::default()
    } else {
        serde_json::from_slice(&body).unwrap_or_default()
    };
    let auto_restart = req.auto_restart.unwrap_or(false);
    let mut res = download_verify_and_install_update()
        .await
        .map_err(|(status, detail)| {
            (
                status,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/update-failed".to_string(),
                    title: "Self-Update Failed".to_string(),
                    status: status.as_u16(),
                    detail,
                    instance: "/v1/system/update/apply".to_string(),
                }),
            )
        })?;

    if auto_restart && res.status == "applied" && schedule_self_restart().is_ok() {
        res.restarting = true;
    }

    Ok(Json(res))
}

/// POST /v1/system/restart
pub async fn restart_server_handler() -> Result<Json<Value>, (StatusCode, Json<ProblemDetails>)> {
    let exe_path = std::env::current_exe().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/restart-failed".to_string(),
                title: "Restart Failed".to_string(),
                status: 500,
                detail: format!("Cannot resolve current executable: {e}"),
                instance: "/v1/system/restart".to_string(),
            }),
        )
    })?;

    let (mode, _) = detect_install_mode(&exe_path);
    if mode == "cargo_dev" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/restart-unsupported".to_string(),
                title: "Restart Skipped in Cargo Dev Mode".to_string(),
                status: 400,
                detail: "Self-restart is disabled while running inside a Cargo build directory."
                    .to_string(),
                instance: "/v1/system/restart".to_string(),
            }),
        ));
    }

    schedule_self_restart().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/restart-failed".to_string(),
                title: "Restart Failed".to_string(),
                status: 500,
                detail: e,
                instance: "/v1/system/restart".to_string(),
            }),
        )
    })?;

    Ok(Json(json!({
        "status": "restarting",
        "message": "Spawned updated VoxForge process; current server exiting cleanly."
    })))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeEndpointStatus {
    pub running: bool,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PythonPackagesStatus {
    pub torch: bool,
    pub whisper: bool,
    pub omnivoice: bool,
    pub kokoro: bool,
    pub piper: bool,
    pub edge_tts: bool,
    #[serde(default)]
    pub chatterbox: bool,
    #[serde(default)]
    pub kitten: bool,
    #[serde(default)]
    pub canto: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeStatusResponse {
    pub rust_core_ready: bool,
    pub python_available: bool,
    pub python_executable: Option<String>,
    pub sidecar_script_available: bool,
    pub sidecar_script_path: Option<String>,
    pub neural_sidecar: RuntimeEndpointStatus,
    pub kokoro_sidecar: RuntimeEndpointStatus,
    pub piper_cli_available: bool,
    pub piper_native_installed: bool,
    pub piper_binary_path: Option<String>,
    pub ffmpeg_available: bool,
    pub python_packages: PythonPackagesStatus,
    pub installed_model_files: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StartSidecarRequest {
    pub backend: Option<String>,
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstallRuntimePackageRequest {
    pub package: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstallNativeRuntimeRequest {
    pub target: String,
}

const EMBEDDED_NEURAL_SIDECAR_PY: &str = include_str!("../../../../scripts/neural_sidecar.py");

fn materialize_embedded_sidecar_to(data_dir: &Path) -> std::io::Result<PathBuf> {
    let target = data_dir.join("scripts").join("neural_sidecar.py");
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let needs_write = match std::fs::read_to_string(&target) {
        Ok(existing) => existing != EMBEDDED_NEURAL_SIDECAR_PY,
        Err(_) => true,
    };
    if needs_write {
        std::fs::write(&target, EMBEDDED_NEURAL_SIDECAR_PY)?;
    }
    Ok(target)
}

fn find_sidecar_script() -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe.parent().map(|p| p.to_path_buf());
        for _ in 0..4 {
            if let Some(dir) = cur {
                roots.push(dir.clone());
                cur = dir.parent().map(|p| p.to_path_buf());
            }
        }
    }
    for root in roots {
        let candidate = root.join("scripts").join("neural_sidecar.py");
        if candidate.is_file() {
            return Some(candidate);
        }
        let sibling = root.join("neural_sidecar.py");
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    materialize_embedded_sidecar_to(&crate::server::resolve_default_data_dir()).ok()
}

pub(crate) fn suppress_console_window(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

pub(crate) async fn check_command_exists(cmd: &str, args: &[&str]) -> bool {
    let mut command = tokio::process::Command::new(cmd);
    suppress_console_window(&mut command);
    let future = command
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    tokio::time::timeout(Duration::from_secs(2), future)
        .await
        .ok()
        .and_then(|r| r.ok())
        .map(|s| s.success())
        .unwrap_or(false)
}

pub(crate) async fn probe_python_executable() -> Option<String> {
    if let Ok(managed) = std::env::var("VOXFORGE_MANAGED_PYTHON") {
        let trimmed = managed.trim();
        if !trimmed.is_empty()
            && Path::new(trimmed).exists()
            && check_command_exists(trimmed, &["--version"]).await
        {
            return Some(trimmed.to_string());
        }
    }

    let candidates = if cfg!(windows) {
        ["python", "py"]
    } else {
        ["python3", "python"]
    };
    for cand in candidates {
        if check_command_exists(cand, &["--version"]).await {
            return Some(cand.to_string());
        }
    }
    None
}

pub(crate) async fn probe_python_packages(python_exe: Option<&str>) -> PythonPackagesStatus {
    let mut status = PythonPackagesStatus {
        torch: false,
        whisper: false,
        omnivoice: false,
        kokoro: false,
        piper: false,
        edge_tts: false,
        chatterbox: false,
        kitten: false,
        canto: false,
    };
    let Some(exe) = python_exe else {
        return status;
    };

    let snippet = "import importlib.util, json; print(json.dumps({p: importlib.util.find_spec(p) is not None for p in ('torch','whisper','omnivoice','kokoro','piper','edge_tts','chatterbox','kittenml','canto_tts')}))";
    let mut command = tokio::process::Command::new(exe);
    suppress_console_window(&mut command);
    let future = command.args(["-c", snippet]).output();
    if let Ok(Ok(output)) = tokio::time::timeout(Duration::from_secs(3), future).await {
        if output.status.success() {
            if let Ok(parsed) = serde_json::from_slice::<Value>(&output.stdout) {
                status.torch = parsed["torch"].as_bool().unwrap_or(false);
                status.whisper = parsed["whisper"].as_bool().unwrap_or(false);
                status.omnivoice = parsed["omnivoice"].as_bool().unwrap_or(false);
                status.kokoro = parsed["kokoro"].as_bool().unwrap_or(false);
                status.piper = parsed["piper"].as_bool().unwrap_or(false);
                status.edge_tts = parsed["edge_tts"].as_bool().unwrap_or(false);
                status.chatterbox = parsed["chatterbox"].as_bool().unwrap_or(false);
                status.kitten = parsed["kittenml"].as_bool().unwrap_or(false);
                status.canto = parsed["canto_tts"].as_bool().unwrap_or(false);
            }
        }
    }
    status
}

async fn probe_http_health(url: &str) -> RuntimeEndpointStatus {
    let health = format!("{}/health", url.trim_end_matches('/'));
    let Ok(client) = reqwest::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
    else {
        return RuntimeEndpointStatus {
            running: false,
            url: None,
        };
    };
    match client.get(&health).send().await {
        Ok(resp) if resp.status().is_success() => RuntimeEndpointStatus {
            running: true,
            url: Some(url.to_string()),
        },
        _ => RuntimeEndpointStatus {
            running: false,
            url: None,
        },
    }
}

pub fn resolve_sidecar_launch_args(
    backend: Option<&str>,
    port: Option<u16>,
) -> Result<(String, u16), String> {
    let chosen_backend = backend
        .map(|b| b.trim().to_ascii_lowercase())
        .filter(|b| !b.is_empty())
        .unwrap_or_else(|| "auto".to_string());
    if !matches!(
        chosen_backend.as_str(),
        "auto" | "omnivoice" | "cosyvoice" | "f5tts" | "kokoro" | "chatterbox" | "kitten" | "canto"
    ) {
        return Err(format!(
            "Unsupported sidecar backend '{chosen_backend}'. Allowed: auto, omnivoice, cosyvoice, f5tts, kokoro, chatterbox, kitten, canto."
        ));
    }

    let chosen_port = match port {
        Some(p @ (9000 | 8880 | 9880 | 8000)) => p,
        Some(other) => {
            return Err(format!(
                "Unsupported sidecar port {other}. Allowed ports: 9000, 8880, 9880, 8000."
            ))
        }
        None if chosen_backend == "kokoro" => 8880,
        None => 9000,
    };

    Ok((chosen_backend, chosen_port))
}

pub fn resolve_runtime_install_packages(target: &str) -> Result<Vec<&'static str>, String> {
    match target.trim().to_ascii_lowercase().as_str() {
        "kokoro" => Ok(vec!["kokoro>=0.9.4", "soundfile", "misaki[en]"]),
        "piper" => Ok(vec!["piper-tts", "pathvalidate"]),
        "whisper" => Ok(vec!["openai-whisper"]),
        "chatterbox" => Ok(vec!["chatterbox-tts"]),
        "kitten" => Ok(vec!["kittenml"]),
        "canto" => Ok(vec!["canto-tts"]),
        other => Err(format!(
            "Unknown runtime package '{other}'. Allowed: kokoro, piper, whisper, chatterbox, kitten, canto."
        )),
    }
}

pub fn resolve_native_piper_archive_url(os: &str, arch: &str) -> Result<&'static str, String> {
    match (os, arch) {
        ("windows", "x86_64" | "aarch64") => {
            Ok("https://github.com/rhasspy/piper/releases/latest/download/piper_windows_amd64.zip")
        }
        ("linux", "x86_64") => Ok(
            "https://github.com/rhasspy/piper/releases/latest/download/piper_linux_x86_64.tar.gz",
        ),
        ("linux", "aarch64") => Ok(
            "https://github.com/rhasspy/piper/releases/latest/download/piper_linux_aarch64.tar.gz",
        ),
        ("macos", "aarch64") => Ok(
            "https://github.com/rhasspy/piper/releases/latest/download/piper_macos_aarch64.tar.gz",
        ),
        ("macos", "x86_64") => {
            Ok("https://github.com/rhasspy/piper/releases/latest/download/piper_macos_x64.tar.gz")
        }
        _ => Err(format!(
            "No standalone native Piper archive for platform {os}/{arch}."
        )),
    }
}

/// GET /v1/system/runtimes — inspect Python libraries, CLI tools, sidecar state, and local model files.
pub async fn get_runtime_status() -> Json<RuntimeStatusResponse> {
    let neural_url = std::env::var("VOXFORGE_SIDECAR_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:9000".to_string());
    let kokoro_url = std::env::var("VOXFORGE_KOKORO_URL")
        .or_else(|_| std::env::var("KOKORO_API_URL"))
        .unwrap_or_else(|_| "http://127.0.0.1:8880".to_string());

    let (python_executable, piper_cli, ffmpeg_available, neural_sidecar, kokoro_sidecar) = tokio::join!(
        probe_python_executable(),
        check_command_exists("piper", &["--version"]),
        check_command_exists("ffmpeg", &["-version"]),
        probe_http_health(&neural_url),
        probe_http_health(&kokoro_url),
    );
    let python_packages = probe_python_packages(python_executable.as_deref()).await;
    let managed_piper = voxforge_engine::piper::managed_piper_binary_path();
    let piper_native_installed = managed_piper.is_some();
    let resolved_piper_bin = managed_piper
        .map(|p| p.to_string_lossy().to_string())
        .or_else(|| piper_cli.then(|| "piper".to_string()));
    let piper_cli_available = piper_native_installed || piper_cli || python_packages.piper;
    let script_path = find_sidecar_script();

    let models_dir = std::env::var("VOXFORGE_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("models"));
    let mut installed_model_files = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(&models_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(ft) = entry.file_type().await {
                if ft.is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        installed_model_files.push(name.to_string());
                    }
                }
            }
        }
    }
    installed_model_files.sort();

    Json(RuntimeStatusResponse {
        rust_core_ready: true,
        python_available: python_executable.is_some(),
        python_executable,
        sidecar_script_available: script_path.is_some(),
        sidecar_script_path: script_path.map(|p| p.to_string_lossy().to_string()),
        neural_sidecar,
        kokoro_sidecar,
        piper_cli_available,
        piper_native_installed,
        piper_binary_path: resolved_piper_bin,
        ffmpeg_available,
        python_packages,
        installed_model_files,
    })
}

/// POST /v1/system/sidecar/start — start the local Python neural sidecar in the background.
pub async fn start_sidecar_handler(
    body: axum::body::Bytes,
) -> Result<Json<Value>, (StatusCode, Json<ProblemDetails>)> {
    let req: StartSidecarRequest = if body.is_empty() {
        StartSidecarRequest::default()
    } else {
        serde_json::from_slice(&body).unwrap_or_default()
    };

    let (backend, port) =
        resolve_sidecar_launch_args(req.backend.as_deref(), req.port).map_err(|detail| {
            (
                StatusCode::BAD_REQUEST,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/invalid-sidecar-config".to_string(),
                    title: "Invalid Sidecar Launch Config".to_string(),
                    status: 400,
                    detail,
                    instance: "/v1/system/sidecar/start".to_string(),
                }),
            )
        })?;

    let target_url = format!("http://127.0.0.1:{port}");
    if probe_http_health(&target_url).await.running {
        return Ok(Json(json!({
            "status": "already_running",
            "backend": backend,
            "port": port,
            "url": target_url,
            "message": format!("Neural sidecar ({backend}) is already active on {target_url}")
        })));
    }

    let python_exe = probe_python_executable().await.ok_or_else(|| {
        (
            StatusCode::FAILED_DEPENDENCY,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/python-not-found".to_string(),
                title: "Python Runtime Missing".to_string(),
                status: StatusCode::FAILED_DEPENDENCY.as_u16(),
                detail: "Python executable was not found on PATH. Install Python 3.10+ to run the local neural sidecar.".to_string(),
                instance: "/v1/system/sidecar/start".to_string(),
            }),
        )
    })?;

    let script_path = find_sidecar_script().ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/sidecar-script-missing".to_string(),
                title: "Sidecar Script Not Found".to_string(),
                status: 404,
                detail: "Could not locate or materialize scripts/neural_sidecar.py.".to_string(),
                instance: "/v1/system/sidecar/start".to_string(),
            }),
        )
    })?;

    let log_dir = crate::server::resolve_default_data_dir().join("logs");
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/sidecar-log-failed".to_string(),
                title: "Sidecar Launch Error".to_string(),
                status: 500,
                detail: format!(
                    "Failed to create sidecar log directory {}: {e}",
                    log_dir.display()
                ),
                instance: "/v1/system/sidecar/start".to_string(),
            }),
        ));
    }
    let log_path = log_dir.join(format!("neural_sidecar_{port}.log"));
    let log_file_out = std::fs::File::create(&log_path).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/sidecar-log-failed".to_string(),
                title: "Sidecar Launch Error".to_string(),
                status: 500,
                detail: format!(
                    "Failed to create sidecar log file {}: {e}",
                    log_path.display()
                ),
                instance: "/v1/system/sidecar/start".to_string(),
            }),
        )
    })?;
    let log_file_err = log_file_out.try_clone().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/sidecar-log-failed".to_string(),
                title: "Sidecar Launch Error".to_string(),
                status: 500,
                detail: format!("Failed cloning sidecar log handle: {e}"),
                instance: "/v1/system/sidecar/start".to_string(),
            }),
        )
    })?;

    let mut cmd = std::process::Command::new(&python_exe);
    cmd.arg(&script_path).arg("--port").arg(port.to_string());
    if backend != "auto" {
        cmd.arg("--backend").arg(&backend);
    }
    cmd.stdout(std::process::Stdio::from(log_file_out))
        .stderr(std::process::Stdio::from(log_file_err));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let mut child = cmd.spawn().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/sidecar-spawn-failed".to_string(),
                title: "Sidecar Spawn Failed".to_string(),
                status: 500,
                detail: format!("Failed spawning neural sidecar: {e}"),
                instance: "/v1/system/sidecar/start".to_string(),
            }),
        )
    })?;

    tokio::time::sleep(Duration::from_millis(800)).await;
    if let Ok(Some(exit_status)) = child.try_wait() {
        let log_tail = tokio::fs::read_to_string(&log_path)
            .await
            .unwrap_or_default();
        return Err((
            StatusCode::FAILED_DEPENDENCY,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/sidecar-exited-early".to_string(),
                title: "Neural Sidecar Exited Immediately".to_string(),
                status: StatusCode::FAILED_DEPENDENCY.as_u16(),
                detail: format!(
                    "Sidecar exited with status {exit_status}. Check dependencies or logs ({}): {}",
                    log_path.display(),
                    log_tail
                        .lines()
                        .rev()
                        .take(6)
                        .collect::<Vec<_>>()
                        .join(" | ")
                ),
                instance: "/v1/system/sidecar/start".to_string(),
            }),
        ));
    }

    let is_up = probe_http_health(&target_url).await.running;
    Ok(Json(json!({
        "status": if is_up { "running" } else { "starting" },
        "backend": backend,
        "port": port,
        "url": target_url,
        "log_path": log_path.to_string_lossy(),
        "message": if is_up {
            format!("Neural sidecar ({backend}) is live at {target_url}")
        } else {
            format!("Neural sidecar ({backend}) is starting & loading weights on {target_url}; check readiness shortly.")
        }
    })))
}

/// POST /v1/system/runtimes/install — install allowlisted Python speech runtime package via pip.
pub async fn install_runtime_package_handler(
    Json(payload): Json<InstallRuntimePackageRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ProblemDetails>)> {
    let packages = resolve_runtime_install_packages(&payload.package).map_err(|detail| {
        (
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-runtime-package".to_string(),
                title: "Invalid Runtime Package".to_string(),
                status: 400,
                detail,
                instance: "/v1/system/runtimes/install".to_string(),
            }),
        )
    })?;

    let python_exe = probe_python_executable().await.ok_or_else(|| {
        (
            StatusCode::FAILED_DEPENDENCY,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/python-not-found".to_string(),
                title: "Python Runtime Missing".to_string(),
                status: StatusCode::FAILED_DEPENDENCY.as_u16(),
                detail:
                    "Python executable not found on PATH; cannot install Python speech packages."
                        .to_string(),
                instance: "/v1/system/runtimes/install".to_string(),
            }),
        )
    })?;

    let mut args = vec!["-m", "pip", "install", "--quiet"];
    args.extend(packages.iter().copied());

    let output = tokio::time::timeout(
        Duration::from_secs(180),
        tokio::process::Command::new(&python_exe)
            .args(&args)
            .output(),
    )
    .await
    .map_err(|_| {
        (
            StatusCode::GATEWAY_TIMEOUT,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/runtime-install-timeout".to_string(),
                title: "Runtime Installation Timed Out".to_string(),
                status: StatusCode::GATEWAY_TIMEOUT.as_u16(),
                detail: format!(
                    "pip install for '{}' exceeded 180 seconds.",
                    payload.package
                ),
                instance: "/v1/system/runtimes/install".to_string(),
            }),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/runtime-install-error".to_string(),
                title: "Runtime Installation Error".to_string(),
                status: 500,
                detail: format!("Failed invoking pip install: {e}"),
                instance: "/v1/system/runtimes/install".to_string(),
            }),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/runtime-install-failed".to_string(),
                title: "Runtime Package Install Failed".to_string(),
                status: StatusCode::BAD_GATEWAY.as_u16(),
                detail: format!(
                    "pip install for '{}' failed ({}): {}",
                    payload.package,
                    output.status,
                    stderr.lines().rev().take(6).collect::<Vec<_>>().join(" | ")
                ),
                instance: "/v1/system/runtimes/install".to_string(),
            }),
        ));
    }

    Ok(Json(json!({
        "status": "installed",
        "package": payload.package,
        "installed_specs": packages,
        "message": format!("Successfully installed runtime package '{}'.", payload.package)
    })))
}

/// POST /v1/system/runtimes/install-native — install zero-Python native runtime binary (e.g. Piper C++ engine).
pub async fn install_native_runtime_handler(
    Json(payload): Json<InstallNativeRuntimeRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ProblemDetails>)> {
    let target = payload.target.trim().to_ascii_lowercase();
    if target != "piper" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-native-runtime".to_string(),
                title: "Unsupported Native Runtime".to_string(),
                status: 400,
                detail: format!(
                    "Unknown native binary target '{target}'. Supported targets: piper."
                ),
                instance: "/v1/system/runtimes/install-native".to_string(),
            }),
        ));
    }

    if let Some(existing) = voxforge_engine::piper::managed_piper_binary_path() {
        return Ok(Json(json!({
            "status": "already_installed",
            "target": "piper",
            "python_required": false,
            "binary_path": existing.to_string_lossy(),
            "message": format!("Native Piper runtime already ready at {}", existing.display())
        })));
    }

    let archive_url = resolve_native_piper_archive_url(
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
    .map_err(|detail| {
        (
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/unsupported-platform".to_string(),
                title: "Unsupported Platform".to_string(),
                status: 400,
                detail,
                instance: "/v1/system/runtimes/install-native".to_string(),
            }),
        )
    })?;

    let bin_root = crate::server::resolve_default_data_dir().join("bin");
    tokio::fs::create_dir_all(&bin_root).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/native-install-dir".to_string(),
                title: "Failed Creating Bin Directory".to_string(),
                status: 500,
                detail: format!("Failed creating {}: {e}", bin_root.display()),
                instance: "/v1/system/runtimes/install-native".to_string(),
            }),
        )
    })?;

    let client = reqwest::Client::builder()
        .user_agent(concat!("VoxForge/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/http-client".to_string(),
                    title: "HTTP Client Error".to_string(),
                    status: 500,
                    detail: e.to_string(),
                    instance: "/v1/system/runtimes/install-native".to_string(),
                }),
            )
        })?;

    let resp = client.get(archive_url).send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/native-download-failed".to_string(),
                title: "Native Binary Download Failed".to_string(),
                status: StatusCode::BAD_GATEWAY.as_u16(),
                detail: format!("Failed downloading Piper archive from {archive_url}: {e}"),
                instance: "/v1/system/runtimes/install-native".to_string(),
            }),
        )
    })?;

    if !resp.status().is_success() {
        return Err((
            StatusCode::BAD_GATEWAY,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/native-download-failed".to_string(),
                title: "Native Binary Download Failed".to_string(),
                status: StatusCode::BAD_GATEWAY.as_u16(),
                detail: format!("Piper release download returned HTTP {}", resp.status()),
                instance: "/v1/system/runtimes/install-native".to_string(),
            }),
        ));
    }

    let bytes = resp.bytes().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/native-download-failed".to_string(),
                title: "Native Binary Read Failed".to_string(),
                status: StatusCode::BAD_GATEWAY.as_u16(),
                detail: format!("Failed reading Piper archive payload: {e}"),
                instance: "/v1/system/runtimes/install-native".to_string(),
            }),
        )
    })?;

    let ext = if archive_url.ends_with(".zip") {
        "zip"
    } else {
        "tar.gz"
    };
    let archive_path = bin_root.join(format!("piper_download.{ext}"));
    tokio::fs::write(&archive_path, &bytes).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/native-extract-failed".to_string(),
                title: "Archive Write Failed".to_string(),
                status: 500,
                detail: format!("Failed writing {}: {e}", archive_path.display()),
                instance: "/v1/system/runtimes/install-native".to_string(),
            }),
        )
    })?;

    let extract_res = if archive_url.ends_with(".zip") {
        let script = format!(
            "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
            archive_path.display().to_string().replace('\'', "''"),
            bin_root.display().to_string().replace('\'', "''")
        );
        let mut cmd = tokio::process::Command::new("powershell");
        suppress_console_window(&mut cmd);
        cmd.args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .await
    } else {
        let mut cmd = tokio::process::Command::new("tar");
        suppress_console_window(&mut cmd);
        cmd.arg("-xzf")
            .arg(&archive_path)
            .arg("-C")
            .arg(&bin_root)
            .output()
            .await
    };

    let _ = tokio::fs::remove_file(&archive_path).await;

    let out = extract_res.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/native-extract-failed".to_string(),
                title: "Archive Extract Failed".to_string(),
                status: 500,
                detail: format!("Failed extracting Piper archive: {e}"),
                instance: "/v1/system/runtimes/install-native".to_string(),
            }),
        )
    })?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/native-extract-failed".to_string(),
                title: "Archive Extract Failed".to_string(),
                status: 500,
                detail: format!("Extractor failed ({}): {stderr}", out.status),
                instance: "/v1/system/runtimes/install-native".to_string(),
            }),
        ));
    }

    let resolved = voxforge_engine::piper::managed_piper_binary_path().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/native-binary-missing".to_string(),
                title: "Binary Missing After Extract".to_string(),
                status: 500,
                detail: format!(
                    "Extracted Piper archive into {}, but executable was not found.",
                    bin_root.display()
                ),
                instance: "/v1/system/runtimes/install-native".to_string(),
            }),
        )
    })?;

    Ok(Json(json!({
        "status": "installed",
        "target": "piper",
        "python_required": false,
        "binary_path": resolved.to_string_lossy(),
        "message": format!("Installed standalone native Piper binary (Zero Python) at {}", resolved.display())
    })))
}

fn schedule_self_restart() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| format!("Cannot locate exe: {e}"))?;
    let args: Vec<String> = std::env::args().skip(1).collect();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(250)).await;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let escaped_exe = exe.to_string_lossy().replace('"', "\"\"");
            let escaped_args = args
                .iter()
                .map(|a| format!("\"{}\"", a.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" ");
            let cmd_line = if escaped_args.is_empty() {
                format!("ping -n 2 127.0.0.1 > nul & start \"\" \"{escaped_exe}\"")
            } else {
                format!("ping -n 2 127.0.0.1 > nul & start \"\" \"{escaped_exe}\" {escaped_args}")
            };
            let _ = std::process::Command::new("cmd")
                .args(["/C", &cmd_line])
                .creation_flags(0x0800_0000)
                .spawn();
        }
        #[cfg(not(windows))]
        {
            let _ = std::process::Command::new(&exe).args(&args).spawn();
        }
        std::process::exit(0);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};

    #[test]
    fn test_semver_parsing_and_comparison() {
        assert_eq!(parse_semver("v0.6.0"), Some((0, 6, 0, false)));
        assert_eq!(parse_semver("0.5.0"), Some((0, 5, 0, false)));
        assert_eq!(parse_semver("1.2.3-rc.1"), Some((1, 2, 3, true)));
        assert_eq!(parse_semver(""), None);

        assert!(is_newer_version("0.5.0", "0.6.0"));
        assert!(is_newer_version("0.5.0", "v0.5.1"));
        assert!(is_newer_version("0.6.0-rc.1", "0.6.0"));
        assert!(!is_newer_version("0.6.0", "0.6.0"));
        assert!(!is_newer_version("0.6.0", "0.5.0"));
        assert!(!is_newer_version("0.6.0", "0.6.0-rc.1"));
    }

    #[test]
    fn test_parse_checksums_txt_extracts_exact_asset_hash() {
        let content = "\
f82d8e7479d0c4d0a55eb522abc4eed38bdc926a6f56c1f46c37ae59e3f79f0f  voxforge-windows-x86_64.exe\n\
0AA4BD3F6243ECDF8F87520B1D06DCC049F1DBFAA1BA457C5AFD20345B5A778F *./VoxForge_Setup_x64.exe\n";

        assert_eq!(
            parse_checksums_txt(content, "voxforge-windows-x86_64.exe").as_deref(),
            Some("f82d8e7479d0c4d0a55eb522abc4eed38bdc926a6f56c1f46c37ae59e3f79f0f")
        );
        assert_eq!(
            parse_checksums_txt(content, "VoxForge_Setup_x64.exe").as_deref(),
            Some("0aa4bd3f6243ecdf8f87520b1d06dcc049f1dbfaa1ba457c5afd20345b5a778f")
        );
        assert_eq!(parse_checksums_txt(content, "missing.exe"), None);
    }

    #[test]
    fn test_trusted_download_url_allowlist() {
        assert!(is_trusted_release_download_url(
            "https://github.com/AMG555/VoxForge/releases/download/v0.6.0/voxforge.exe",
            DEFAULT_RELEASE_API_URL
        ));
        assert!(is_trusted_release_download_url(
            "https://objects.githubusercontent.com/github-production-release-asset/12345",
            DEFAULT_RELEASE_API_URL
        ));
        assert!(!is_trusted_release_download_url(
            "https://evil.example.com/voxforge.exe",
            DEFAULT_RELEASE_API_URL
        ));
        assert!(!is_trusted_release_download_url(
            "http://github.com/AMG555/VoxForge/releases/download/v0.6.0/voxforge.exe",
            DEFAULT_RELEASE_API_URL
        ));
    }

    #[tokio::test]
    async fn test_check_for_updates_from_mock_github_release() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let (primary, _) =
            resolve_platform_asset_candidates(std::env::consts::OS, std::env::consts::ARCH);
        let expected_hash = "a".repeat(64);
        let sums_body = format!("{expected_hash}  {primary}\n");

        let asset_url = format!("{base}/download/{primary}");
        let sums_url = format!("{base}/download/checksums.txt");
        let release_json = serde_json::json!({
            "tag_name": "v99.0.0",
            "name": "VoxForge v99.0.0",
            "body": "Enterprise self-update test release",
            "published_at": "2026-11-25T00:00:00Z",
            "html_url": format!("{base}/releases/tag/v99.0.0"),
            "assets": [
                {
                    "name": primary,
                    "size": 1234567,
                    "browser_download_url": asset_url
                },
                {
                    "name": "checksums.txt",
                    "size": sums_body.len(),
                    "browser_download_url": sums_url
                }
            ]
        });

        let router = Router::new()
            .route(
                "/releases/latest",
                get(move || {
                    let payload = release_json.clone();
                    async move { Json(payload) }
                }),
            )
            .route(
                "/download/checksums.txt",
                get(move || {
                    let text = sums_body.clone();
                    async move { text }
                }),
            );

        let srv = tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });

        let res = check_for_updates_from_url(&format!("{base}/releases/latest")).await;
        srv.abort();

        assert_eq!(res.check_status, "ok");
        assert_eq!(res.latest_version.as_deref(), Some("99.0.0"));
        assert!(res.update_available);
        assert_eq!(res.asset_name, primary);
        assert_eq!(res.checksum_sha256.as_deref(), Some(expected_hash.as_str()));
    }

    #[test]
    fn test_runtime_allowlists() {
        assert_eq!(
            resolve_runtime_install_packages("kokoro").unwrap(),
            vec!["kokoro>=0.9.4", "soundfile", "misaki[en]"]
        );
        assert_eq!(
            resolve_runtime_install_packages("piper").unwrap(),
            vec!["piper-tts", "pathvalidate"]
        );
        assert_eq!(
            resolve_runtime_install_packages("chatterbox").unwrap(),
            vec!["chatterbox-tts"]
        );
        assert_eq!(
            resolve_runtime_install_packages("kitten").unwrap(),
            vec!["kittenml"]
        );
        assert_eq!(
            resolve_runtime_install_packages("canto").unwrap(),
            vec!["canto-tts"]
        );
        assert!(resolve_runtime_install_packages("arbitrary; rm -rf /").is_err());

        assert_eq!(
            resolve_sidecar_launch_args(Some("kokoro"), None).unwrap(),
            ("kokoro".to_string(), 8880)
        );
        assert_eq!(
            resolve_sidecar_launch_args(Some("omnivoice"), Some(9000)).unwrap(),
            ("omnivoice".to_string(), 9000)
        );
        assert_eq!(
            resolve_sidecar_launch_args(Some("chatterbox"), Some(9000)).unwrap(),
            ("chatterbox".to_string(), 9000)
        );
        assert_eq!(
            resolve_sidecar_launch_args(Some("kitten"), Some(9000)).unwrap(),
            ("kitten".to_string(), 9000)
        );
        assert_eq!(
            resolve_sidecar_launch_args(Some("canto"), Some(9000)).unwrap(),
            ("canto".to_string(), 9000)
        );
        assert!(resolve_sidecar_launch_args(Some("bash"), Some(9000)).is_err());
        assert!(resolve_sidecar_launch_args(Some("auto"), Some(22)).is_err());

        assert!(resolve_native_piper_archive_url("windows", "x86_64")
            .unwrap()
            .ends_with("piper_windows_amd64.zip"));
        assert!(resolve_native_piper_archive_url("macos", "aarch64")
            .unwrap()
            .ends_with("piper_macos_aarch64.tar.gz"));
        assert!(resolve_native_piper_archive_url("linux", "x86_64")
            .unwrap()
            .ends_with("piper_linux_x86_64.tar.gz"));
    }

    #[test]
    fn test_embedded_sidecar_script_materializes() {
        let dir =
            std::env::temp_dir().join(format!("voxforge_sidecar_test_{}", uuid::Uuid::new_v4()));
        let path = materialize_embedded_sidecar_to(&dir).unwrap();
        assert!(path.is_file());
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("--backend"));
        assert!(content.contains("--port"));
        assert_eq!(content, EMBEDDED_NEURAL_SIDECAR_PY);
        // Re-running is idempotent
        let path2 = materialize_embedded_sidecar_to(&dir).unwrap();
        assert_eq!(path, path2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
