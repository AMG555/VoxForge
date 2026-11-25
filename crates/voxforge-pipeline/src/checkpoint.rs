use chrono::Utc;
use hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub const RESUME_MANIFEST_FILE: &str = "resume.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    Rendering,
    Completed {
        output_path: String,
        duration_ms: u64,
        sha256: String,
    },
    Failed {
        error: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterTask {
    pub index: usize,
    pub title: String,
    pub text: String,
    pub voice_id: String,
    pub speed: f32,
    pub content_hash: String,
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCheckpointManifest {
    pub version: u32,
    pub job_id: String,
    pub job_type: String,
    pub title: String,
    pub total_chapters: usize,
    pub completed_chapters: usize,
    pub created_at: String,
    pub updated_at: String,
    pub chapters: Vec<ChapterTask>,
}

impl JobCheckpointManifest {
    pub fn new(job_id: String, job_type: String, title: String) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            version: 1,
            job_id,
            job_type,
            title,
            total_chapters: 0,
            completed_chapters: 0,
            created_at: now.clone(),
            updated_at: now,
            chapters: Vec::new(),
        }
    }

    /// Add a chapter task and compute its content-addressable hash
    pub fn add_chapter(&mut self, title: String, text: String, voice_id: String, speed: f32) {
        let index = self.chapters.len();
        let content_hash = Self::compute_hash(&text, &voice_id, speed);
        self.chapters.push(ChapterTask {
            index,
            title,
            text,
            voice_id,
            speed,
            content_hash,
            status: TaskStatus::Pending,
        });
        self.total_chapters = self.chapters.len();
    }

    /// Compute deterministic SHA-256 hash of chapter synthesis arguments
    pub fn compute_hash(text: &str, voice_id: &str, speed: f32) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        hasher.update(b"|");
        hasher.update(voice_id.as_bytes());
        hasher.update(b"|");
        hasher.update(format!("{:.2}", speed).as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Mark a chapter as successfully completed
    pub fn mark_completed(
        &mut self,
        index: usize,
        output_path: String,
        duration_ms: u64,
        file_sha256: String,
    ) {
        if let Some(task) = self.chapters.get_mut(index) {
            task.status = TaskStatus::Completed {
                output_path,
                duration_ms,
                sha256: file_sha256,
            };
            self.completed_chapters = self
                .chapters
                .iter()
                .filter(|c| matches!(c.status, TaskStatus::Completed { .. }))
                .count();
            self.updated_at = Utc::now().to_rfc3339();
        }
    }

    /// Mark a chapter as failed
    pub fn mark_failed(&mut self, index: usize, error: String) {
        if let Some(task) = self.chapters.get_mut(index) {
            task.status = TaskStatus::Failed { error };
            self.updated_at = Utc::now().to_rfc3339();
        }
    }

    /// Returns list of chapter indices that have not yet been successfully completed
    pub fn get_remaining_indices(&self) -> Vec<usize> {
        self.chapters
            .iter()
            .enumerate()
            .filter(|(_, c)| !matches!(c.status, TaskStatus::Completed { .. }))
            .map(|(i, _)| i)
            .collect()
    }

    /// True if all chapters are rendered
    pub fn is_finished(&self) -> bool {
        self.completed_chapters == self.total_chapters && self.total_chapters > 0
    }

    /// Atomically save manifest to job work directory
    pub fn save_to_dir(&self, work_dir: &Path) -> std::io::Result<PathBuf> {
        fs::create_dir_all(work_dir)?;
        let manifest_path = work_dir.join(RESUME_MANIFEST_FILE);
        let temp_path = work_dir.join(format!("{}.tmp", RESUME_MANIFEST_FILE));

        let data = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        fs::write(&temp_path, data)?;
        fs::rename(&temp_path, &manifest_path)?;

        Ok(manifest_path)
    }

    /// Load manifest from job work directory
    pub fn load_from_dir(work_dir: &Path) -> std::io::Result<Option<Self>> {
        let manifest_path = work_dir.join(RESUME_MANIFEST_FILE);
        if !manifest_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&manifest_path)?;
        let manifest: Self = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        Ok(Some(manifest))
    }

    /// Automatically parse a raw markdown manuscript or text document into structured chapters.
    ///
    /// Detects Markdown headings (`#`, `##`, `###`) and `Chapter <number>` lines.
    /// If no chapter headings are detected, splits along paragraph boundaries into ~1,000-word sections.
    pub fn parse_manuscript(
        raw_text: &str,
        default_voice: &str,
        default_speed: f32,
    ) -> Vec<ChapterTask> {
        let mut chapters: Vec<ChapterTask> = Vec::new();
        let lines: Vec<&str> = raw_text.lines().collect();

        // Pass 1: detect if any chapter headings exist
        let mut heading_indices: Vec<(usize, String)> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("# ")
                || trimmed.starts_with("## ")
                || trimmed.starts_with("### ")
            {
                let title = trimmed.trim_start_matches('#').trim().to_string();
                if !title.is_empty() {
                    heading_indices.push((i, title));
                }
            } else if trimmed.to_lowercase().starts_with("chapter ") {
                heading_indices.push((i, trimmed.to_string()));
            }
        }

        if !heading_indices.is_empty() {
            for (idx, (line_idx, title)) in heading_indices.iter().enumerate() {
                let start_line = line_idx + 1;
                let end_line = if idx + 1 < heading_indices.len() {
                    heading_indices[idx + 1].0
                } else {
                    lines.len()
                };

                let chapter_lines = &lines[start_line..end_line];
                let chapter_text = chapter_lines.join("\n").trim().to_string();

                if !chapter_text.is_empty() {
                    let index = chapters.len();
                    let content_hash =
                        Self::compute_hash(&chapter_text, default_voice, default_speed);
                    chapters.push(ChapterTask {
                        index,
                        title: title.clone(),
                        text: chapter_text,
                        voice_id: default_voice.to_string(),
                        speed: default_speed,
                        content_hash,
                        status: TaskStatus::Pending,
                    });
                }
            }
        } else {
            // Split into ~1,000-word sections along paragraph boundaries
            let paragraphs: Vec<&str> = raw_text.split("\n\n").collect();
            let mut current_chunk = Vec::new();
            let mut current_word_count = 0usize;

            for p in paragraphs {
                let trimmed = p.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let words = trimmed.split_whitespace().count();
                current_chunk.push(trimmed);
                current_word_count += words;

                if current_word_count >= 1000 {
                    let text = current_chunk.join("\n\n");
                    let index = chapters.len();
                    let title = format!("Section {}", index + 1);
                    let content_hash = Self::compute_hash(&text, default_voice, default_speed);
                    chapters.push(ChapterTask {
                        index,
                        title,
                        text,
                        voice_id: default_voice.to_string(),
                        speed: default_speed,
                        content_hash,
                        status: TaskStatus::Pending,
                    });
                    current_chunk.clear();
                    current_word_count = 0;
                }
            }

            if !current_chunk.is_empty() {
                let text = current_chunk.join("\n\n");
                let index = chapters.len();
                let title = format!("Section {}", index + 1);
                let content_hash = Self::compute_hash(&text, default_voice, default_speed);
                chapters.push(ChapterTask {
                    index,
                    title,
                    text,
                    voice_id: default_voice.to_string(),
                    speed: default_speed,
                    content_hash,
                    status: TaskStatus::Pending,
                });
            }
        }

        chapters
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_manifest_lifecycle() {
        let mut manifest = JobCheckpointManifest::new(
            "job-audiobook-001".to_string(),
            "audiobook".to_string(),
            "The Odyssey".to_string(),
        );

        manifest.add_chapter(
            "Chapter 1".to_string(),
            "Tell me, O muse...".to_string(),
            "en-US-AriaNeural".to_string(),
            1.0,
        );
        manifest.add_chapter(
            "Chapter 2".to_string(),
            "When the child of morning...".to_string(),
            "en-US-AriaNeural".to_string(),
            1.0,
        );

        assert_eq!(manifest.total_chapters, 2);
        assert_eq!(manifest.completed_chapters, 0);
        assert_eq!(manifest.get_remaining_indices(), vec![0, 1]);

        // Complete chapter 1
        manifest.mark_completed(
            0,
            "/tmp/out_1.wav".to_string(),
            12500,
            "abc123sha".to_string(),
        );
        assert_eq!(manifest.completed_chapters, 1);
        assert_eq!(manifest.get_remaining_indices(), vec![1]);
        assert!(!manifest.is_finished());

        // Complete chapter 2
        manifest.mark_completed(
            1,
            "/tmp/out_2.wav".to_string(),
            14200,
            "def456sha".to_string(),
        );
        assert_eq!(manifest.completed_chapters, 2);
        assert_eq!(manifest.get_remaining_indices(), Vec::<usize>::new());
        assert!(manifest.is_finished());
    }

    #[test]
    fn test_parse_manuscript_markdown() {
        let markdown = r#"
# Chapter 1: The Call to Adventure
It was a quiet morning in the valley. The wind whispered through ancient pines.

# Chapter 2: The Crossing
The journey across the mountain was perilous. Shadows danced upon the rock face.
"#;
        let chapters = JobCheckpointManifest::parse_manuscript(markdown, "mock-narrator", 1.0);
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].title, "Chapter 1: The Call to Adventure");
        assert!(chapters[0].text.contains("quiet morning in the valley"));
        assert_eq!(chapters[0].voice_id, "mock-narrator");
        assert_eq!(chapters[1].title, "Chapter 2: The Crossing");
        assert!(chapters[1].text.contains("journey across the mountain"));
    }

    #[test]
    fn test_parse_manuscript_unstructured() {
        let text = "This is a single block of narrative text without any headings.\n\nIt continues for a second paragraph.";
        let chapters = JobCheckpointManifest::parse_manuscript(text, "mock-narrator", 1.0);
        assert_eq!(chapters.len(), 1);
        assert_eq!(chapters[0].title, "Section 1");
        assert!(chapters[0].text.contains("single block of narrative"));
    }
}
