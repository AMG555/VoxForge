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
}
