use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sysinfo::Disks;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;
use voxforge_core::error::ProblemDetails;
use voxforge_hardware::HardwareProfile;

use crate::routes::system::{
    check_command_exists, probe_python_executable, probe_python_packages, suppress_console_window,
    PythonPackagesStatus,
};
use crate::state::AppState;

const MAX_LOG_LINES: usize = 250;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopConfig {
    pub setup_completed: bool,
    pub runtime_profile: String,
    pub runtime_dir: String,
    pub custom_python: Option<String>,
    pub remote_server_url: Option<String>,
    pub telemetry_opt_in: bool,
}

impl Default for DesktopConfig {
    fn default() -> Self {
        Self {
            setup_completed: false,
            runtime_profile: "core_neural".to_string(),
            runtime_dir: default_runtime_dir().to_string_lossy().to_string(),
            custom_python: None,
            remote_server_url: None,
            telemetry_opt_in: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredPython {
    pub path: String,
    pub version: String,
    pub label: String,
    pub is_managed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProfileSpec {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub disk_estimate_gb: f32,
    pub requires_python: bool,
    pub packages: Vec<String>,
    pub recommended: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupStatusResponse {
    pub config: DesktopConfig,
    pub default_runtime_dir: String,
    pub managed_venv_python: Option<String>,
    pub managed_venv_ready: bool,
    pub uv_available: bool,
    pub uv_path: Option<String>,
    pub disk_available_bytes: u64,
    pub disk_total_bytes: u64,
    pub hardware: HardwareProfile,
    pub discovered_pythons: Vec<DiscoveredPython>,
    pub installed_packages: PythonPackagesStatus,
    pub profiles: Vec<RuntimeProfileSpec>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SaveSetupConfigRequest {
    pub setup_completed: Option<bool>,
    pub runtime_profile: Option<String>,
    pub runtime_dir: Option<String>,
    pub custom_python: Option<String>,
    pub remote_server_url: Option<String>,
    pub telemetry_opt_in: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProbePythonRequest {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestRemoteRequest {
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartSetupInstallRequest {
    pub profile: Option<String>,
    pub runtime_dir: Option<String>,
    pub custom_python: Option<String>,
    pub telemetry_opt_in: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupStepProgress {
    pub id: String,
    pub label: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallJobSnapshot {
    pub job_id: Option<String>,
    pub status: String,
    pub profile: String,
    pub runtime_dir: String,
    pub current_step: String,
    pub percent: u8,
    pub elapsed_secs: u64,
    pub steps: Vec<SetupStepProgress>,
    pub logs: Vec<String>,
    pub error: Option<String>,
}

struct InstallJobManager {
    snapshot: InstallJobSnapshot,
    started_at: Option<Instant>,
    cancel_requested: bool,
    active_child_pid: Option<u32>,
}

impl Default for InstallJobManager {
    fn default() -> Self {
        Self {
            snapshot: InstallJobSnapshot {
                job_id: None,
                status: "idle".to_string(),
                profile: "core_neural".to_string(),
                runtime_dir: default_runtime_dir().to_string_lossy().to_string(),
                current_step: "checking_env".to_string(),
                percent: 0,
                elapsed_secs: 0,
                steps: initial_steps(),
                logs: Vec::new(),
                error: None,
            },
            started_at: None,
            cancel_requested: false,
            active_child_pid: None,
        }
    }
}

static JOB_MANAGER: OnceLock<Mutex<InstallJobManager>> = OnceLock::new();

fn job_manager() -> &'static Mutex<InstallJobManager> {
    JOB_MANAGER.get_or_init(|| Mutex::new(InstallJobManager::default()))
}

fn initial_steps() -> Vec<SetupStepProgress> {
    vec![
        SetupStepProgress {
            id: "checking_env".to_string(),
            label: "Checking environment...".to_string(),
            status: "pending".to_string(),
            detail: None,
        },
        SetupStepProgress {
            id: "preparing_uv".to_string(),
            label: "Preparing uv (Python package manager)".to_string(),
            status: "pending".to_string(),
            detail: None,
        },
        SetupStepProgress {
            id: "creating_venv".to_string(),
            label: "Provisioning isolated Python 3.11 runtime".to_string(),
            status: "pending".to_string(),
            detail: None,
        },
        SetupStepProgress {
            id: "installing_packages".to_string(),
            label: "Installing AI speech libraries".to_string(),
            status: "pending".to_string(),
            detail: None,
        },
        SetupStepProgress {
            id: "verifying_env".to_string(),
            label: "Verifying the environment".to_string(),
            status: "pending".to_string(),
            detail: None,
        },
    ]
}

fn active_data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("VOXFORGE_DATA_DIR") {
        let p = PathBuf::from(dir);
        if !p.as_os_str().is_empty() {
            return p;
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("VoxForge");
        }
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    home.join(".voxforge")
}

pub fn default_runtime_dir() -> PathBuf {
    active_data_dir().join("runtime")
}

pub fn load_desktop_config_from(data_dir: &Path) -> DesktopConfig {
    let path = data_dir.join("desktop-config.json");
    let mut cfg = match std::fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str::<DesktopConfig>(&raw).unwrap_or_default(),
        Err(_) => DesktopConfig::default(),
    };
    if cfg.runtime_dir.trim().is_empty() {
        cfg.runtime_dir = data_dir.join("runtime").to_string_lossy().to_string();
    }
    cfg
}

pub fn load_desktop_config() -> DesktopConfig {
    load_desktop_config_from(&active_data_dir())
}

pub fn save_desktop_config_to(data_dir: &Path, cfg: &DesktopConfig) -> Result<(), String> {
    let path = data_dir.join("desktop-config.json");
    std::fs::create_dir_all(data_dir)
        .map_err(|e| format!("Failed creating config directory: {e}"))?;
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| format!("Failed serializing desktop config: {e}"))?;
    std::fs::write(&path, json).map_err(|e| format!("Failed saving desktop config: {e}"))
}

pub fn save_desktop_config(cfg: &DesktopConfig) -> Result<(), String> {
    save_desktop_config_to(&active_data_dir(), cfg)
}

pub fn venv_python_path(runtime_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        runtime_dir.join(".venv").join("Scripts").join("python.exe")
    } else {
        runtime_dir.join(".venv").join("bin").join("python")
    }
}

pub fn resolve_managed_python(cfg: &DesktopConfig) -> Option<PathBuf> {
    let venv_py = venv_python_path(Path::new(&cfg.runtime_dir));
    if venv_py.is_file() {
        return Some(venv_py);
    }
    if let Some(ref custom) = cfg.custom_python {
        let p = Path::new(custom.trim());
        if !custom.trim().is_empty() && p.exists() {
            return Some(p.to_path_buf());
        }
    }
    None
}

/// Apply persisted managed Python path so all Rust engines (`Piper`, sidecars, Whisper) pick it up automatically.
pub fn apply_desktop_config_env() {
    let cfg = load_desktop_config();
    std::env::set_var("VOXFORGE_RUNTIME_DIR", &cfg.runtime_dir);
    if let Some(py) = resolve_managed_python(&cfg) {
        std::env::set_var("VOXFORGE_MANAGED_PYTHON", py);
    } else {
        std::env::remove_var("VOXFORGE_MANAGED_PYTHON");
    }
}

pub fn resolve_profile_packages(profile: &str) -> Result<Vec<&'static str>, String> {
    match profile.trim().to_ascii_lowercase().as_str() {
        "rust_core" => Ok(Vec::new()),
        "core_neural" => Ok(vec![
            "kokoro>=0.9.4",
            "soundfile",
            "misaki[en]",
            "piper-tts",
            "pathvalidate",
            "openai-whisper",
        ]),
        "full_cloning" => Ok(vec![
            "torch",
            "torchaudio",
            "omnivoice",
            "kokoro>=0.9.4",
            "soundfile",
            "misaki[en]",
            "piper-tts",
            "pathvalidate",
            "openai-whisper",
        ]),
        other => Err(format!(
            "Unknown runtime profile '{other}'. Allowed: rust_core, core_neural, full_cloning, remote."
        )),
    }
}

pub fn runtime_profile_specs() -> Vec<RuntimeProfileSpec> {
    vec![
        RuntimeProfileSpec {
            id: "rust_core".to_string(),
            title: "Instant Rust Core".to_string(),
            subtitle: "Zero Python setup. Edge-TTS 130+ languages, DSP mastering & acoustic cloning ready now.".to_string(),
            disk_estimate_gb: 0.0,
            requires_python: false,
            packages: Vec::new(),
            recommended: false,
        },
        RuntimeProfileSpec {
            id: "core_neural".to_string(),
            title: "Core Neural Studio".to_string(),
            subtitle: "Isolated Python 3.11 + Kokoro-82M + Piper ONNX + Whisper ASR for offline speech.".to_string(),
            disk_estimate_gb: 1.4,
            requires_python: true,
            packages: resolve_profile_packages("core_neural")
                .unwrap_or_default()
                .into_iter()
                .map(String::from)
                .collect(),
            recommended: true,
        },
        RuntimeProfileSpec {
            id: "full_cloning".to_string(),
            title: "Full Cloning Studio".to_string(),
            subtitle: "Everything in Core Neural plus PyTorch + OmniVoice 646-language zero-shot neural cloning.".to_string(),
            disk_estimate_gb: 4.5,
            requires_python: true,
            packages: resolve_profile_packages("full_cloning")
                .unwrap_or_default()
                .into_iter()
                .map(String::from)
                .collect(),
            recommended: false,
        },
    ]
}

fn query_disk_space_for_path(target: &Path) -> (u64, u64) {
    let disks = Disks::new_with_refreshed_list();
    let normalized = std::fs::canonicalize(target)
        .unwrap_or_else(|_| target.to_path_buf())
        .to_string_lossy()
        .to_string();

    let mut best_match_len = 0usize;
    let mut best_avail = 0u64;
    let mut best_total = 0u64;

    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy();
        let norm_lower = normalized.to_ascii_lowercase();
        let mount_lower = mount.to_ascii_lowercase();
        if norm_lower.starts_with(&mount_lower) && mount.len() >= best_match_len {
            best_match_len = mount.len();
            best_avail = disk.available_space();
            best_total = disk.total_space();
        }
    }

    (best_avail, best_total)
}

async fn find_uv_executable() -> Option<String> {
    let local_bin = active_data_dir()
        .join("bin")
        .join(if cfg!(windows) { "uv.exe" } else { "uv" });
    if local_bin.is_file() {
        let s = local_bin.to_string_lossy().to_string();
        if check_command_exists(&s, &["--version"]).await {
            return Some(s);
        }
    }
    if check_command_exists("uv", &["--version"]).await {
        return Some("uv".to_string());
    }
    None
}

async fn read_python_version(exe: &str) -> Option<String> {
    let mut cmd = tokio::process::Command::new(exe);
    suppress_console_window(&mut cmd);
    let out = tokio::time::timeout(Duration::from_secs(2), cmd.arg("--version").output())
        .await
        .ok()?
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let err_text = String::from_utf8_lossy(&out.stderr);
    let ver = if text.trim().is_empty() {
        err_text.trim()
    } else {
        text.trim()
    };
    Some(ver.to_string())
}

async fn discover_python_interpreters(runtime_dir: &Path) -> Vec<DiscoveredPython> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let venv_py = venv_python_path(runtime_dir);
    let mut candidates: Vec<(String, String, bool)> = Vec::new();
    if venv_py.is_file() {
        candidates.push((
            venv_py.to_string_lossy().to_string(),
            "Managed VoxForge Runtime".to_string(),
            true,
        ));
    }

    let where_cmd = if cfg!(windows) { "where" } else { "which" };
    for name in if cfg!(windows) {
        ["python", "python3"]
    } else {
        ["python3", "python"]
    } {
        let mut cmd = tokio::process::Command::new(where_cmd);
        suppress_console_window(&mut cmd);
        if let Ok(Ok(out)) =
            tokio::time::timeout(Duration::from_secs(2), cmd.arg(name).output()).await
        {
            if out.status.success() {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() && !trimmed.contains("WindowsApps") {
                        candidates.push((trimmed.to_string(), "System PATH".to_string(), false));
                    }
                }
            }
        }
    }

    for (path, label, is_managed) in candidates {
        let key = path.to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        if let Some(version) = read_python_version(&path).await {
            found.push(DiscoveredPython {
                path,
                version,
                label,
                is_managed,
            });
        }
        if found.len() >= 6 {
            break;
        }
    }

    found
}

/// GET /v1/system/setup-status — full onboarding & runtime status snapshot.
pub async fn get_setup_status(State(state): State<AppState>) -> Json<SetupStatusResponse> {
    let cfg = load_desktop_config();
    let rt_dir = PathBuf::from(&cfg.runtime_dir);
    let venv_py = venv_python_path(&rt_dir);
    let managed_venv_ready = venv_py.is_file();

    let (uv_path, active_py, discovered_pythons) = tokio::join!(
        find_uv_executable(),
        probe_python_executable(),
        discover_python_interpreters(&rt_dir),
    );
    let installed_packages = probe_python_packages(active_py.as_deref()).await;
    let (disk_available_bytes, disk_total_bytes) = query_disk_space_for_path(&rt_dir);

    Json(SetupStatusResponse {
        config: cfg,
        default_runtime_dir: default_runtime_dir().to_string_lossy().to_string(),
        managed_venv_python: managed_venv_ready.then(|| venv_py.to_string_lossy().to_string()),
        managed_venv_ready,
        uv_available: uv_path.is_some(),
        uv_path,
        disk_available_bytes,
        disk_total_bytes,
        hardware: state.hardware_profile.clone(),
        discovered_pythons,
        installed_packages,
        profiles: runtime_profile_specs(),
    })
}

/// POST /v1/system/setup/config — persist desktop setup configuration.
pub async fn save_setup_config_handler(
    Json(req): Json<SaveSetupConfigRequest>,
) -> Result<Json<DesktopConfig>, (StatusCode, Json<ProblemDetails>)> {
    let mut cfg = load_desktop_config();

    if let Some(profile) = req.runtime_profile {
        let trimmed = profile.trim().to_ascii_lowercase();
        if trimmed != "remote" {
            resolve_profile_packages(&trimmed).map_err(|detail| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ProblemDetails {
                        problem_type: "https://voxforge.org/errors/invalid-runtime-profile"
                            .to_string(),
                        title: "Invalid Runtime Profile".to_string(),
                        status: 400,
                        detail,
                        instance: "/v1/system/setup/config".to_string(),
                    }),
                )
            })?;
        }
        cfg.runtime_profile = trimmed;
    }

    if let Some(dir) = req.runtime_dir {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            cfg.runtime_dir = trimmed.to_string();
        }
    }

    if let Some(py) = req.custom_python {
        let trimmed = py.trim();
        cfg.custom_python = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }

    if let Some(remote) = req.remote_server_url {
        let trimmed = remote.trim();
        cfg.remote_server_url = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.trim_end_matches('/').to_string())
        };
    }

    if let Some(done) = req.setup_completed {
        cfg.setup_completed = done;
    }

    if let Some(telemetry) = req.telemetry_opt_in {
        cfg.telemetry_opt_in = telemetry;
    }

    save_desktop_config(&cfg).map_err(|detail| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/config-save-failed".to_string(),
                title: "Failed Saving Config".to_string(),
                status: 500,
                detail,
                instance: "/v1/system/setup/config".to_string(),
            }),
        )
    })?;

    apply_desktop_config_env();
    Ok(Json(cfg))
}

/// POST /v1/system/setup/probe-python — inspect a user-chosen Python interpreter.
pub async fn probe_python_handler(Json(req): Json<ProbePythonRequest>) -> Json<Value> {
    let trimmed = req.path.trim();
    if trimmed.is_empty() || !Path::new(trimmed).exists() {
        return Json(json!({
            "valid": false,
            "path": trimmed,
            "error": "Python executable not found at specified path."
        }));
    }

    match read_python_version(trimmed).await {
        Some(version) => {
            let packages = probe_python_packages(Some(trimmed)).await;
            Json(json!({
                "valid": true,
                "path": trimmed,
                "version": version,
                "packages": packages,
            }))
        }
        None => Json(json!({
            "valid": false,
            "path": trimmed,
            "error": "Selected file did not respond as a valid Python interpreter."
        })),
    }
}

/// POST /v1/system/setup/test-remote — verify connectivity to another VoxForge workstation.
pub async fn test_remote_handler(
    Json(req): Json<TestRemoteRequest>,
) -> Result<Json<Value>, (StatusCode, Json<ProblemDetails>)> {
    let raw = req.url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(raw).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-url".to_string(),
                title: "Invalid Remote Server URL".to_string(),
                status: 400,
                detail: "Enter a valid http:// or https:// VoxForge server URL.".to_string(),
                instance: "/v1/system/setup/test-remote".to_string(),
            }),
        )
    })?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-url".to_string(),
                title: "Unsupported URL Scheme".to_string(),
                status: 400,
                detail: "Only http:// and https:// URLs are supported.".to_string(),
                instance: "/v1/system/setup/test-remote".to_string(),
            }),
        ));
    }

    let health_url = format!("{raw}/health");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/http-client".to_string(),
                    title: "HTTP Client Error".to_string(),
                    status: 500,
                    detail: e.to_string(),
                    instance: "/v1/system/setup/test-remote".to_string(),
                }),
            )
        })?;

    match client.get(&health_url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: Value = resp.json().await.unwrap_or_else(|_| json!({}));
            Ok(Json(json!({
                "reachable": true,
                "url": raw,
                "status": body["status"].as_str().unwrap_or("ok"),
                "version": body["version"].as_str().unwrap_or("unknown"),
            })))
        }
        Ok(resp) => Ok(Json(json!({
            "reachable": false,
            "url": raw,
            "error": format!("Remote server returned HTTP {}", resp.status()),
        }))),
        Err(e) => Ok(Json(json!({
            "reachable": false,
            "url": raw,
            "error": format!("Connection failed: {e}"),
        }))),
    }
}

async fn append_log(line: impl Into<String>) {
    let mut guard = JOB_MANAGER
        .get_or_init(|| Mutex::new(InstallJobManager::default()))
        .lock()
        .await;
    let mut deque: VecDeque<String> = guard.snapshot.logs.iter().cloned().collect();
    deque.push_back(line.into());
    while deque.len() > MAX_LOG_LINES {
        deque.pop_front();
    }
    guard.snapshot.logs = deque.into_iter().collect();
}

async fn update_step(step_id: &str, status: &str, detail: Option<String>, percent: u8) {
    let mut guard = job_manager().lock().await;
    guard.snapshot.current_step = step_id.to_string();
    guard.snapshot.percent = percent;
    for step in &mut guard.snapshot.steps {
        if step.id == step_id {
            step.status = status.to_string();
            if detail.is_some() {
                step.detail = detail.clone();
            }
        }
    }
}

async fn is_cancelled() -> bool {
    job_manager().lock().await.cancel_requested
}

async fn run_logged_command(
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<(), String> {
    let mut cmd = tokio::process::Command::new(program);
    suppress_console_window(&mut cmd);
    cmd.args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (k, v) in envs {
        cmd.env(k, v);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Failed starting '{program}': {e}"))?;

    if let Some(pid) = child.id() {
        job_manager().lock().await.active_child_pid = Some(pid);
    }

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let out_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    append_log(line).await;
                }
            }
        }
    });

    let err_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if !line.trim().is_empty() {
                    append_log(line).await;
                }
            }
        }
    });

    let status = loop {
        if is_cancelled().await {
            let _ = child.kill().await;
            let _ = out_task.await;
            let _ = err_task.await;
            job_manager().lock().await.active_child_pid = None;
            return Err("Installation cancelled by user.".to_string());
        }
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => tokio::time::sleep(Duration::from_millis(120)).await,
            Err(e) => {
                job_manager().lock().await.active_child_pid = None;
                return Err(format!("Failed polling '{program}': {e}"));
            }
        }
    };

    let _ = out_task.await;
    let _ = err_task.await;
    job_manager().lock().await.active_child_pid = None;

    if status.success() {
        Ok(())
    } else {
        Err(format!("Command '{program}' exited with {status}"))
    }
}

async fn bootstrap_uv_if_missing() -> Result<String, String> {
    if let Some(uv) = find_uv_executable().await {
        append_log(format!("Found uv package manager: {uv}")).await;
        return Ok(uv);
    }

    append_log("uv not found on PATH; bootstrapping official Astral uv binary...").await;
    let bin_dir = active_data_dir().join("bin");
    std::fs::create_dir_all(&bin_dir).map_err(|e| format!("Failed creating bin directory: {e}"))?;

    #[cfg(target_os = "windows")]
    {
        let script = format!(
            "$env:UV_INSTALL_DIR = '{}'; irm https://astral.sh/uv/install.ps1 | iex",
            bin_dir.display().to_string().replace('\'', "''")
        );
        run_logged_command(
            "powershell",
            &[
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                &script,
            ],
            &[],
        )
        .await?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let bin_str = bin_dir.to_string_lossy().to_string();
        let cmd = format!(
            "UV_INSTALL_DIR=\"{}\" curl -LsSf https://astral.sh/uv/install.sh | sh",
            bin_str
        );
        run_logged_command("sh", &["-c", &cmd], &[]).await?;
    }

    find_uv_executable()
        .await
        .ok_or_else(|| "uv bootstrap completed but executable was not found.".to_string())
}

async fn execute_install_pipeline(
    profile: String,
    runtime_dir: PathBuf,
    custom_python: Option<String>,
    telemetry_opt_in: bool,
) {
    let result: Result<(), String> = async {
        update_step("checking_env", "running", None, 8).await;
        append_log(format!("Target environment: {}", runtime_dir.display())).await;
        std::fs::create_dir_all(&runtime_dir).map_err(|e| {
            format!(
                "Cannot create runtime directory '{}': {e}",
                runtime_dir.display()
            )
        })?;

        let packages = resolve_profile_packages(&profile)?;
        if packages.is_empty() {
            update_step("checking_env", "done", None, 100).await;
            let mut cfg = load_desktop_config();
            cfg.setup_completed = true;
            cfg.runtime_profile = "rust_core".to_string();
            cfg.runtime_dir = runtime_dir.to_string_lossy().to_string();
            cfg.telemetry_opt_in = telemetry_opt_in;
            save_desktop_config(&cfg)?;
            apply_desktop_config_env();
            return Ok(());
        }
        update_step("checking_env", "done", None, 18).await;

        if is_cancelled().await {
            return Err("Installation cancelled by user.".to_string());
        }

        // Step 2: Prepare uv or fallback to custom python
        update_step("preparing_uv", "running", None, 25).await;
        let uv_res = bootstrap_uv_if_missing().await;
        let uv_exe = match uv_res {
            Ok(uv) => {
                update_step("preparing_uv", "done", Some(uv.clone()), 38).await;
                Some(uv)
            }
            Err(err) => {
                append_log(format!(
                    "uv unavailable ({err}); trying Python venv fallback..."
                ))
                .await;
                update_step(
                    "preparing_uv",
                    "done",
                    Some("Python venv fallback".to_string()),
                    38,
                )
                .await;
                None
            }
        };

        if is_cancelled().await {
            return Err("Installation cancelled by user.".to_string());
        }

        // Step 3: Create isolated Python 3.11 virtual environment
        update_step("creating_venv", "running", None, 44).await;
        let venv_dir = runtime_dir.join(".venv");
        let venv_py = venv_python_path(&runtime_dir);
        if !venv_py.is_file() {
            let venv_str = venv_dir.to_string_lossy().to_string();
            if let Some(ref uv) = uv_exe {
                let py_spec = custom_python
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("3.11");
                append_log(format!(
                    "Creating isolated venv at {venv_str} (Python {py_spec})..."
                ))
                .await;
                run_logged_command(uv, &["venv", &venv_str, "--python", py_spec, "--seed"], &[])
                    .await?;
            } else {
                let fallback_py = custom_python
                    .clone()
                    .or_else(|| {
                        if cfg!(windows) {
                            Some("python".to_string())
                        } else {
                            Some("python3".to_string())
                        }
                    })
                    .unwrap_or_else(|| "python".to_string());
                append_log(format!("Creating Python venv using {fallback_py}...")).await;
                run_logged_command(&fallback_py, &["-m", "venv", &venv_str], &[]).await?;
            }
        } else {
            append_log(format!(
                "Reusing existing venv Python: {}",
                venv_py.display()
            ))
            .await;
        }
        if !venv_py.is_file() {
            return Err(format!(
                "Virtual environment Python missing after creation: {}",
                venv_py.display()
            ));
        }
        update_step("creating_venv", "done", None, 58).await;

        if is_cancelled().await {
            return Err("Installation cancelled by user.".to_string());
        }

        // Step 4: Install packages
        update_step(
            "installing_packages",
            "running",
            Some(packages.join(", ")),
            64,
        )
        .await;
        let py_str = venv_py.to_string_lossy().to_string();
        if let Some(ref uv) = uv_exe {
            let mut args = vec!["pip", "install", "--python", py_str.as_str()];
            args.extend(packages.iter().copied());
            append_log(format!("Running: uv pip install {}", packages.join(" "))).await;
            run_logged_command(uv, &args, &[("UV_HTTP_TIMEOUT", "600")]).await?;
        } else {
            let mut args = vec!["-m", "pip", "install"];
            args.extend(packages.iter().copied());
            append_log(format!("Running: pip install {}", packages.join(" "))).await;
            run_logged_command(&py_str, &args, &[]).await?;
        }
        update_step("installing_packages", "done", None, 90).await;

        if is_cancelled().await {
            return Err("Installation cancelled by user.".to_string());
        }

        // Step 5: Verify imports and persist configuration
        update_step("verifying_env", "running", None, 95).await;
        append_log("Verifying installed neural libraries...").await;
        let pkgs = probe_python_packages(Some(&py_str)).await;
        append_log(format!(
            "Verification: kokoro={}, piper={}, whisper={}, omnivoice={}, torch={}",
            pkgs.kokoro, pkgs.piper, pkgs.whisper, pkgs.omnivoice, pkgs.torch
        ))
        .await;

        let mut cfg = load_desktop_config();
        cfg.setup_completed = true;
        cfg.runtime_profile = profile.clone();
        cfg.runtime_dir = runtime_dir.to_string_lossy().to_string();
        cfg.custom_python = custom_python.filter(|s| !s.trim().is_empty());
        cfg.telemetry_opt_in = telemetry_opt_in;
        save_desktop_config(&cfg)?;
        apply_desktop_config_env();

        update_step("verifying_env", "done", None, 100).await;
        append_log("VoxForge local neural runtime is ready.").await;
        Ok(())
    }
    .await;

    let mut guard = job_manager().lock().await;
    if let Some(started) = guard.started_at {
        guard.snapshot.elapsed_secs = started.elapsed().as_secs();
    }
    match result {
        Ok(()) => {
            guard.snapshot.status = "completed".to_string();
            guard.snapshot.percent = 100;
            guard.snapshot.error = None;
        }
        Err(msg) => {
            let cancelled = guard.cancel_requested;
            guard.snapshot.status = if cancelled {
                "cancelled".to_string()
            } else {
                "failed".to_string()
            };
            guard.snapshot.error = Some(msg.clone());
            let cur = guard.snapshot.current_step.clone();
            for s in &mut guard.snapshot.steps {
                if s.id == cur && s.status == "running" {
                    s.status = if cancelled {
                        "cancelled".to_string()
                    } else {
                        "failed".to_string()
                    };
                    s.detail = Some(msg.clone());
                }
            }
        }
    }
}

/// POST /v1/system/setup/install — start background uv/Python runtime provisioning.
pub async fn start_setup_install_handler(
    Json(req): Json<StartSetupInstallRequest>,
) -> Result<Json<InstallJobSnapshot>, (StatusCode, Json<ProblemDetails>)> {
    let profile = req
        .profile
        .unwrap_or_else(|| "core_neural".to_string())
        .trim()
        .to_ascii_lowercase();
    resolve_profile_packages(&profile).map_err(|detail| {
        (
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/invalid-runtime-profile".to_string(),
                title: "Invalid Runtime Profile".to_string(),
                status: 400,
                detail,
                instance: "/v1/system/setup/install".to_string(),
            }),
        )
    })?;

    let runtime_dir = req
        .runtime_dir
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_runtime_dir);

    let custom_python = req
        .custom_python
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let telemetry_opt_in = req.telemetry_opt_in.unwrap_or(false);

    let snapshot = {
        let mut guard = job_manager().lock().await;
        if guard.snapshot.status == "running" {
            return Err((
                StatusCode::CONFLICT,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/install-in-progress".to_string(),
                    title: "Runtime Install Already Running".to_string(),
                    status: 409,
                    detail: "Wait for or cancel the active runtime setup job first.".to_string(),
                    instance: "/v1/system/setup/install".to_string(),
                }),
            ));
        }

        guard.cancel_requested = false;
        guard.active_child_pid = None;
        guard.started_at = Some(Instant::now());
        guard.snapshot = InstallJobSnapshot {
            job_id: Some(uuid::Uuid::new_v4().to_string()),
            status: "running".to_string(),
            profile: profile.clone(),
            runtime_dir: runtime_dir.to_string_lossy().to_string(),
            current_step: "checking_env".to_string(),
            percent: 5,
            elapsed_secs: 0,
            steps: initial_steps(),
            logs: vec![format!("Starting setup for profile '{profile}'...")],
            error: None,
        };
        guard.snapshot.clone()
    };

    tokio::spawn(execute_install_pipeline(
        profile,
        runtime_dir,
        custom_python,
        telemetry_opt_in,
    ));

    Ok(Json(snapshot))
}

/// GET /v1/system/setup/progress — poll active or last runtime install progress + streamed logs.
pub async fn get_setup_progress_handler() -> Json<InstallJobSnapshot> {
    let mut guard = job_manager().lock().await;
    if guard.snapshot.status == "running" {
        if let Some(started) = guard.started_at {
            guard.snapshot.elapsed_secs = started.elapsed().as_secs();
        }
    }
    Json(guard.snapshot.clone())
}

/// POST /v1/system/setup/cancel — request cancellation and terminate any active subprocess.
pub async fn cancel_setup_install_handler() -> Json<InstallJobSnapshot> {
    let mut guard = job_manager().lock().await;
    if guard.snapshot.status == "running" {
        guard.cancel_requested = true;
        guard
            .snapshot
            .logs
            .push("Cancellation requested by user...".to_string());
    }
    Json(guard.snapshot.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_package_allowlists() {
        assert!(resolve_profile_packages("rust_core").unwrap().is_empty());
        let core = resolve_profile_packages("core_neural").unwrap();
        assert!(core.contains(&"piper-tts"));
        assert!(core.contains(&"openai-whisper"));
        assert!(core.iter().any(|p| p.starts_with("kokoro")));

        let full = resolve_profile_packages("full_cloning").unwrap();
        assert!(full.contains(&"torch"));
        assert!(full.contains(&"omnivoice"));
        assert!(resolve_profile_packages("rm -rf /").is_err());
    }

    #[test]
    fn test_desktop_config_persistence_and_env() {
        let tmp = std::env::temp_dir().join(format!(
            "voxforge_setup_cfg_{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&tmp).unwrap();

        let rt = tmp.join("runtime");
        let cfg = DesktopConfig {
            setup_completed: true,
            runtime_profile: "core_neural".to_string(),
            runtime_dir: rt.to_string_lossy().to_string(),
            custom_python: None,
            remote_server_url: None,
            telemetry_opt_in: false,
        };
        save_desktop_config_to(&tmp, &cfg).expect("should write desktop-config.json");
        let reloaded = load_desktop_config_from(&tmp);
        assert!(reloaded.setup_completed);
        assert_eq!(reloaded.runtime_profile, "core_neural");
        assert_eq!(reloaded.runtime_dir, cfg.runtime_dir);
        assert!(resolve_managed_python(&reloaded).is_none());

        let fake_py = venv_python_path(&rt);
        std::fs::create_dir_all(fake_py.parent().unwrap()).unwrap();
        std::fs::write(&fake_py, b"stub").unwrap();
        assert_eq!(
            resolve_managed_python(&reloaded).as_deref(),
            Some(fake_py.as_path())
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[tokio::test]
    async fn test_setup_endpoints_and_validation() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use std::sync::Arc;
        use tower::ServiceExt;
        use voxforge_core::store::memory::MemoryStore;
        use voxforge_engine::EngineRegistry;
        use voxforge_hardware::HardwareProbe;

        let state = AppState::new(
            Arc::new(EngineRegistry::new()),
            Arc::new(MemoryStore::new()),
            HardwareProbe::probe(),
            None,
        );
        let app = crate::create_app(state);

        let status_res = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/system/setup-status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(status_res.status(), StatusCode::OK);

        let bad_remote = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/system/setup/test-remote")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"url":"ftp://invalid.local"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_remote.status(), StatusCode::BAD_REQUEST);

        let bad_profile = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/system/setup/install")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"profile":"malicious_pkg"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad_profile.status(), StatusCode::BAD_REQUEST);
    }
}
