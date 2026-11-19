use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use voxforge_core::error::ProblemDetails;
use voxforge_pipeline::checkpoint::JobCheckpointManifest;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct ListJobsResponse {
    pub jobs: Vec<JobCheckpointManifest>,
}

#[derive(Debug, Deserialize)]
pub struct CreateJobRequest {
    pub job_id: String,
    pub job_type: String,
    pub title: String,
    pub chapters: Vec<CreateChapterPayload>,
}

#[derive(Debug, Deserialize)]
pub struct CreateChapterPayload {
    pub title: String,
    pub text: String,
    pub voice_id: String,
    pub speed: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct ResumeJobResponse {
    pub manifest: JobCheckpointManifest,
    pub remaining_indices: Vec<usize>,
    pub is_finished: bool,
}

#[derive(Debug, Deserialize)]
pub struct ParseManuscriptRequest {
    pub manuscript_text: String,
    pub title: Option<String>,
    pub default_voice: Option<String>,
    pub default_speed: Option<f32>,
    /// Optional job_id: if set, registers the checkpoint job immediately
    pub job_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ParseManuscriptResponse {
    pub title: String,
    pub total_words: usize,
    pub total_chapters: usize,
    pub estimated_duration_minutes: f32,
    pub chapters: Vec<voxforge_pipeline::checkpoint::ChapterTask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<JobCheckpointManifest>,
}

/// Sanitize a job_id to a safe filesystem segment.
///
/// Uses an allowlist (alphanumeric + `-` + `_`) instead of a denylist so that
/// no variant of path traversal (`../`, `..%2F`, `....//`) can slip through.
/// Returns `"unknown"` for empty or entirely non-safe IDs.
fn sanitize_job_id(id: &str) -> String {
    let safe: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64) // cap length
        .collect();
    if safe.is_empty() || safe == "." || safe == ".." {
        "unknown".to_string()
    } else {
        safe
    }
}

fn get_work_dir(job_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join("voxforge_jobs")
        .join(sanitize_job_id(job_id))
}

/// `GET /v1/jobs/checkpoints` — list recent job checkpoints
pub async fn list_checkpoints_handler(State(_state): State<AppState>) -> Json<ListJobsResponse> {
    let base_dir = std::env::temp_dir().join("voxforge_jobs");
    let mut jobs = Vec::new();

    if let Ok(entries) = std::fs::read_dir(base_dir) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                if let Ok(Some(manifest)) = JobCheckpointManifest::load_from_dir(&entry.path()) {
                    jobs.push(manifest);
                }
            }
        }
    }

    Json(ListJobsResponse { jobs })
}

/// `POST /v1/jobs/checkpoint` — create or register an audiobook/longform render job
pub async fn create_checkpoint_handler(
    State(_state): State<AppState>,
    Json(body): Json<CreateJobRequest>,
) -> Result<(StatusCode, Json<JobCheckpointManifest>), (StatusCode, Json<ProblemDetails>)> {
    if body.chapters.len() > 5000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/too-many-chapters".to_string(),
                title: "Too Many Chapters".to_string(),
                status: 400,
                detail: "Exceeded maximum allowed chapters per job (5,000)".to_string(),
                instance: "/v1/jobs/checkpoint".to_string(),
            }),
        ));
    }

    let work_dir = get_work_dir(&body.job_id);

    let mut manifest = JobCheckpointManifest::new(body.job_id, body.job_type, body.title);
    for ch in body.chapters {
        manifest.add_chapter(ch.title, ch.text, ch.voice_id, ch.speed.unwrap_or(1.0));
    }

    manifest.save_to_dir(&work_dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/io-error".to_string(),
                title: "Failed to Save Checkpoint".to_string(),
                status: 500,
                detail: e.to_string(),
                instance: "/v1/jobs/checkpoint".to_string(),
            }),
        )
    })?;

    Ok((StatusCode::CREATED, Json(manifest)))
}

/// `POST /v1/jobs/{id}/resume` — inspect and resume an interrupted render job
pub async fn resume_job_handler(
    State(_state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<ResumeJobResponse>, (StatusCode, Json<ProblemDetails>)> {
    let work_dir = get_work_dir(&job_id);

    let manifest = JobCheckpointManifest::load_from_dir(&work_dir)
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/io-error".to_string(),
                    title: "Failed to Read Checkpoint".to_string(),
                    status: 500,
                    detail: e.to_string(),
                    instance: format!("/v1/jobs/{job_id}/resume"),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ProblemDetails {
                    problem_type: "https://voxforge.org/errors/not-found".to_string(),
                    title: "Job Checkpoint Not Found".to_string(),
                    status: 404,
                    detail: format!("No resume manifest found for job ID '{job_id}'"),
                    instance: format!("/v1/jobs/{job_id}/resume"),
                }),
            )
        })?;

    let remaining_indices = manifest.get_remaining_indices();
    let is_finished = manifest.is_finished();

    Ok(Json(ResumeJobResponse {
        manifest,
        remaining_indices,
        is_finished,
    }))
}

/// `POST /v1/jobs/parse-manuscript` — parse raw markdown manuscript into chapters
pub async fn parse_manuscript_handler(
    State(_state): State<AppState>,
    Json(body): Json<ParseManuscriptRequest>,
) -> Result<Json<ParseManuscriptResponse>, (StatusCode, Json<ProblemDetails>)> {
    if body.manuscript_text.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/empty-payload".to_string(),
                title: "Missing Manuscript Text".to_string(),
                status: 400,
                detail: "The 'manuscript_text' field cannot be empty".to_string(),
                instance: "/v1/jobs/parse-manuscript".to_string(),
            }),
        ));
    }

    if body.manuscript_text.len() > 10 * 1024 * 1024 {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ProblemDetails {
                problem_type: "https://voxforge.org/errors/payload-too-large".to_string(),
                title: "Manuscript Payload Too Large".to_string(),
                status: 413,
                detail: "Manuscript payload exceeds maximum permitted size of 10MB (~2,000,000 words)".to_string(),
                instance: "/v1/jobs/parse-manuscript".to_string(),
            }),
        ));
    }

    let default_voice = body.default_voice.as_deref().unwrap_or("en-US-AriaNeural");
    let default_speed = body.default_speed.unwrap_or(1.0).clamp(0.5, 2.5);
    let title = body.title.unwrap_or_else(|| "Untitled Audiobook".to_string());

    let chapters = JobCheckpointManifest::parse_manuscript(
        &body.manuscript_text,
        default_voice,
        default_speed,
    );

    let total_words: usize = chapters
        .iter()
        .map(|c| c.text.split_whitespace().count())
        .sum();
    let estimated_duration_minutes = total_words as f32 / 150.0;

    let manifest = if let Some(job_id) = body.job_id {
        let work_dir = get_work_dir(&job_id);
        let mut m = JobCheckpointManifest::new(job_id, "audiobook".to_string(), title.clone());
        for ch in &chapters {
            m.add_chapter(ch.title.clone(), ch.text.clone(), ch.voice_id.clone(), ch.speed);
        }
        let _ = m.save_to_dir(&work_dir);
        Some(m)
    } else {
        None
    };

    Ok(Json(ParseManuscriptResponse {
        title,
        total_words,
        total_chapters: chapters.len(),
        estimated_duration_minutes,
        chapters,
        manifest,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_work_dir_sanitizes_path_traversal() {
        // Classic traversal
        let dir = get_work_dir("../../../etc/passwd");
        let dir_str = dir.to_str().unwrap_or("");
        assert!(!dir_str.contains(".."), "must not contain ..");
        assert!(!dir_str.contains("/etc"), "must not reach /etc");
        // All special chars become '_' or are dropped
        assert!(dir_str.ends_with("______etc_passwd"));

        // Dot-dot variant that the old .replace("..","") would not catch
        let dir2 = get_work_dir(".../...");
        let d2 = dir2.to_str().unwrap_or("");
        assert!(!d2.contains(".."), "triple-dot variant must be neutralized");

        // Null byte
        let dir3 = get_work_dir("valid\x00evil");
        let d3 = dir3.to_str().unwrap_or("");
        assert!(!d3.contains('\x00'));

        // Length cap
        let long_id = "a".repeat(200);
        let dir4 = get_work_dir(&long_id);
        let last_seg = dir4.file_name().unwrap().to_str().unwrap();
        assert!(last_seg.len() <= 64);
    }

    #[test]
    fn test_create_job_request_deserialization() {
        let json = r#"{
            "job_id": "job-101",
            "job_type": "audiobook",
            "title": "Moby Dick",
            "chapters": [
                {
                    "title": "Chapter 1",
                    "text": "Call me Ishmael.",
                    "voice_id": "en-us-male-1",
                    "speed": 1.1
                }
            ]
        }"#;

        let req: CreateJobRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.job_id, "job-101");
        assert_eq!(req.chapters.len(), 1);
        assert_eq!(req.chapters[0].speed, Some(1.1));
    }

    #[test]
    fn test_parse_manuscript_request_deserialization() {
        let json = serde_json::json!({
            "manuscript_text": "# Chapter 1\nHello world",
            "title": "Test Book",
            "default_voice": "mock-narrator",
            "default_speed": 1.05
        });

        let req: ParseManuscriptRequest = serde_json::from_value(json).unwrap();
        assert_eq!(req.title, Some("Test Book".to_string()));
        assert_eq!(req.default_voice, Some("mock-narrator".to_string()));
        assert_eq!(req.default_speed, Some(1.05));
    }
}
