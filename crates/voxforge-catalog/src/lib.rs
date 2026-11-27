//! VoxForge Model Catalog and Weights Lifecycle Manager.
//!
//! Provides curated registries of open-source TTS, ASR, and VAD model weights,
//! hardware compatibility matching, SHA-256 integrity validation, and installation
//! tracking.

pub mod mirror_race;
pub mod models;
pub mod store;

pub use mirror_race::{MirrorCandidate, MirrorProbeResult, MirrorRaceManager, MirrorRaceResult};
pub use models::{
    assess_system_fit, CatalogItem, ModelFormat, ModelStatus, ModelType, SystemFitAssessment,
    SystemFitTier,
};
pub use store::ModelCatalogStore;
