use thiserror::Error;

#[derive(Error, Debug)]
pub enum VoxForgeError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Engine error: {0}")]
    Engine(String),

    #[error("Voice not found: {0}")]
    VoiceNotFound(String),

    #[error("Pipeline validation error: {0}")]
    PipelineValidation(String),

    #[error("Pipeline execution failed at node {node_id}: {reason}")]
    PipelineExecution { node_id: String, reason: String },

    #[error("Audio processing error: {0}")]
    AudioProcessing(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Authentication error: {0}")]
    Unauthorized(String),

    #[error("Rate limit exceeded. Retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Hardware capability error: {0}")]
    HardwareUnsupported(String),

    #[error("Internal server error: {0}")]
    Internal(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, VoxForgeError>;

#[derive(Debug, serde::Serialize)]
pub struct ProblemDetails {
    #[serde(rename = "type")]
    pub problem_type: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
    pub instance: String,
}

impl VoxForgeError {
    pub fn to_problem_details(&self, instance: &str) -> ProblemDetails {
        let (status, title) = match self {
            VoxForgeError::VoiceNotFound(_) => (404, "Voice Not Found"),
            VoxForgeError::PipelineValidation(_) => (422, "Pipeline Validation Error"),
            VoxForgeError::PipelineExecution { .. } => (422, "Pipeline Execution Error"),
            VoxForgeError::Unauthorized(_) => (401, "Unauthorized"),
            VoxForgeError::RateLimited { .. } => (429, "Rate Limit Exceeded"),
            VoxForgeError::HardwareUnsupported(_) => (400, "Hardware Unsupported"),
            VoxForgeError::Config(_) => (400, "Bad Configuration"),
            _ => (500, "Internal Server Error"),
        };

        ProblemDetails {
            problem_type: format!(
                "https://voxforge.org/errors/{}",
                title.to_lowercase().replace(' ', "-")
            ),
            title: title.to_string(),
            status,
            detail: self.to_string(),
            instance: instance.to_string(),
        }
    }

    /// Classify a synthesis-time error into an actionable user hint.
    ///
    /// Returns `(http_status, user_detail)` based on the error message so callers
    /// can distinguish OOM, network failures, and timeouts from generic engine bugs.
    pub fn classify_synthesis_error(&self) -> (u16, String) {
        let msg = self.to_string();
        let low = msg.to_lowercase();

        // Out-of-memory: CUDA / CPU allocator exhausted
        if low.contains("out of memory")
            || low.contains("not enough memory")
            || low.contains("cannot allocate memory")
            || low.contains("cuda_error_out_of_memory")
        {
            return (
                503,
                format!(
                    "The TTS engine ran out of memory. Try a shorter input, restart \
                     the server to release cached models, or use a lighter engine. \
                     Underlying: {msg}"
                ),
            );
        }

        // Network / connectivity failures (Edge TTS, OpenAI router, model downloads)
        if low.contains("connection")
            || low.contains("network")
            || low.contains("dns")
            || low.contains("tls")
            || low.contains("ssl")
            || low.contains("no such host")
            || low.contains("failed to lookup")
        {
            return (
                503,
                format!(
                    "A network request to an upstream TTS service failed. Check \
                     your internet connection or switch to a local engine (kokoro, piper). \
                     Underlying: {msg}"
                ),
            );
        }

        // Timeout
        if low.contains("timed out") || low.contains("timeout") || low.contains("deadline") {
            return (
                504,
                format!(
                    "The TTS engine did not respond in time. Try a shorter input or \
                     a faster engine. Underlying: {msg}"
                ),
            );
        }

        // Configuration / missing model weights
        if low.contains("not found")
            || low.contains("no such file")
            || low.contains("model path")
            || low.contains("weights")
        {
            return (
                503,
                format!(
                    "A required model file or configuration is missing. Run \
                     `voxforge models install <id>` to download it. Underlying: {msg}"
                ),
            );
        }

        // Generic engine error
        (500, msg)
    }
}
