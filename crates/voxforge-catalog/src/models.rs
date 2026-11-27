use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use voxforge_hardware::HardwareProfile;

/// Functional domain classification of a model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    Tts,
    Asr,
    Vad,
    Diarizer,
}

/// Serialization weight format container.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelFormat {
    Onnx,
    PyTorch,
    Safetensors,
    Ggml,
}

/// Installation and runtime state of a catalog model.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    Available,
    Downloading,
    Installed,
    Error,
}

/// Compatibility tier of a model for the active host machine.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SystemFitTier {
    Optimal,
    Good,
    CpuSlow,
    Insufficient,
}

/// Host hardware compatibility assessment and recommendation summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemFitAssessment {
    pub tier: SystemFitTier,
    pub score: u8,
    pub label: String,
    pub reason: String,
}

/// Metadata, storage, documentation, and requirements descriptor for a catalog weight package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogItem {
    pub id: String,
    pub name: String,
    pub description: String,
    pub model_type: ModelType,
    pub format: ModelFormat,
    pub size_bytes: u64,
    pub sha256: String,
    pub download_url: String,
    #[serde(default)]
    pub docs_url: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub pros: Vec<String>,
    #[serde(default)]
    pub cons: Vec<String>,
    #[serde(default)]
    pub best_for: Vec<String>,
    pub min_ram_mb: u64,
    #[serde(default)]
    pub min_vram_mb: u64,
    pub requires_gpu: bool,
    #[serde(default)]
    pub runtime_requirement: String,
    pub supported_languages: Vec<String>,
    pub status: ModelStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_fit: Option<SystemFitAssessment>,
}

/// Evaluate how well a catalog model runs on the current machine's CPU, SIMD, RAM, and GPU/VRAM.
pub fn assess_system_fit(item: &CatalogItem, hardware: &HardwareProfile) -> SystemFitAssessment {
    let total_ram = hardware.total_memory_mb;
    let best_accel = hardware
        .gpus
        .iter()
        .filter(|g| g.has_cuda || g.has_mps || g.vram_mb >= 2048)
        .max_by_key(|g| {
            let bonus = if g.has_cuda || g.has_mps { 100_000 } else { 0 };
            bonus + g.vram_mb
        });
    let cuda_or_mps_vram = hardware
        .gpus
        .iter()
        .filter(|g| g.has_cuda || g.has_mps)
        .map(|g| g.vram_mb)
        .max()
        .unwrap_or(0);

    if total_ram > 0 && total_ram < item.min_ram_mb {
        return SystemFitAssessment {
            tier: SystemFitTier::Insufficient,
            score: 15,
            label: "Low RAM".to_string(),
            reason: format!(
                "Requires at least {:.1} GB RAM, host detects {:.1} GB.",
                item.min_ram_mb as f64 / 1024.0,
                total_ram as f64 / 1024.0
            ),
        };
    }

    if item.requires_gpu && cuda_or_mps_vram == 0 {
        return SystemFitAssessment {
            tier: SystemFitTier::Insufficient,
            score: 20,
            label: "GPU Required".to_string(),
            reason: format!(
                "Requires CUDA or Apple MPS acceleration ({:.1} GB VRAM).",
                item.min_vram_mb as f64 / 1024.0
            ),
        };
    }

    let simd_label = if hardware.simd.avx512 {
        "AVX-512"
    } else if hardware.simd.avx2 {
        "AVX2"
    } else if hardware.simd.neon {
        "ARM NEON"
    } else {
        "scalar CPU"
    };

    if item.min_vram_mb > 0 {
        if cuda_or_mps_vram >= item.min_vram_mb {
            let gpu_name = best_accel
                .map(|g| g.name.as_str())
                .unwrap_or("Hardware GPU");
            return SystemFitAssessment {
                tier: SystemFitTier::Optimal,
                score: 94,
                label: "Best for Your System".to_string(),
                reason: format!(
                    "{gpu_name} ({:.1} GB VRAM) + {:.1} GB RAM meets full GPU acceleration target.",
                    cuda_or_mps_vram as f64 / 1024.0,
                    total_ram as f64 / 1024.0
                ),
            };
        }

        if cuda_or_mps_vram >= (item.min_vram_mb * 3) / 4 || cuda_or_mps_vram >= 3500 {
            let gpu_name = best_accel
                .map(|g| g.name.as_str())
                .unwrap_or("Hardware GPU");
            return SystemFitAssessment {
                tier: SystemFitTier::Good,
                score: 80,
                label: "Runs Well (Tight VRAM)".to_string(),
                reason: format!(
                    "{gpu_name} ({:.1} GB VRAM) can accelerate short segments; keep FP16/CPU offload enabled ({:.1} GB recommended).",
                    cuda_or_mps_vram as f64 / 1024.0,
                    item.min_vram_mb as f64 / 1024.0
                ),
            };
        }

        return SystemFitAssessment {
            tier: SystemFitTier::CpuSlow,
            score: 52,
            label: "CPU Slow (Batch Only)".to_string(),
            reason: format!(
                "Target model recommends {:.1} GB CUDA/MPS VRAM; host will fall back to {} CPU inference ({} threads) — great quality, slower synthesis.",
                item.min_vram_mb as f64 / 1024.0,
                simd_label,
                hardware.cpu_logical_threads
            ),
        };
    }

    let ram_headroom_ok = total_ram == 0 || total_ram >= item.min_ram_mb.saturating_mul(2);
    let simd_fast = hardware.simd.avx2 || hardware.simd.avx512 || hardware.simd.neon;
    if ram_headroom_ok && (simd_fast || hardware.cpu_logical_threads >= 4) {
        let gpu_bonus = if let Some(gpu) = best_accel {
            format!(" Optional GPU assist ready on {}.", gpu.name)
        } else {
            String::new()
        };
        SystemFitAssessment {
            tier: SystemFitTier::Optimal,
            score: 96,
            label: "Best for Your System".to_string(),
            reason: format!(
                "Fits easily in {:.1} GB RAM and runs fast locally on {} ({} threads).{}",
                total_ram as f64 / 1024.0,
                simd_label,
                hardware.cpu_logical_threads,
                gpu_bonus
            ),
        }
    } else {
        SystemFitAssessment {
            tier: SystemFitTier::Good,
            score: 76,
            label: "Compatible".to_string(),
            reason: format!(
                "Meets {:.1} GB minimum RAM on {} CPU ({} threads).",
                item.min_ram_mb as f64 / 1024.0,
                simd_label,
                hardware.cpu_logical_threads
            ),
        }
    }
}
