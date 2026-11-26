use std::sync::Arc;
use voxforge_asr::AsrRegistry;
use voxforge_benchmark::store::BenchmarkStore;
use voxforge_catalog::ModelCatalogStore;
use voxforge_core::store::DataStore;
use voxforge_engine::{EngineRegistry, VoiceIdentityResolver, VoiceProfileStore};
use voxforge_hardware::HardwareProfile;
use voxforge_mcp::McpServer;
use voxforge_pipeline::PipelineExecutor;
use voxforge_pronunciation::{ProcessorConfig, PronunciationProcessor};
use voxforge_router::VoiceRouter;
use voxforge_worker::{WorkerClient, WorkerPool};

use crate::metrics::MetricsCollector;
use crate::middleware::RateLimiter;
use crate::routes::voice_ci::ProfileStore;

#[derive(Clone)]
pub struct AppState {
    pub engine_registry: Arc<EngineRegistry>,
    pub voice_router: Arc<VoiceRouter>,
    pub voice_identities: Arc<VoiceIdentityResolver>,
    pub voice_profiles: Arc<VoiceProfileStore>,
    pub asr_registry: Arc<AsrRegistry>,
    pub model_catalog: Arc<ModelCatalogStore>,
    pub pronunciation: Arc<PronunciationProcessor>,
    pub benchmark_store: Arc<BenchmarkStore>,
    pub voice_ci_store: ProfileStore,
    pub worker_pool: Arc<WorkerPool>,
    pub worker_client: Arc<WorkerClient>,
    pub data_store: Arc<dyn DataStore>,
    pub hardware_profile: HardwareProfile,
    pub pipeline_executor: Arc<PipelineExecutor>,
    pub api_key: Option<String>,
    pub metrics: Arc<MetricsCollector>,
    pub rate_limiter: RateLimiter,
    /// Pre-built MCP server — shared across all requests to avoid per-request engine re-registration.
    pub mcp_server: Arc<McpServer>,
}

impl AppState {
    pub fn new(
        engine_registry: Arc<EngineRegistry>,
        data_store: Arc<dyn DataStore>,
        hardware_profile: HardwareProfile,
        api_key: Option<String>,
    ) -> Self {
        let asr_registry = Arc::new(AsrRegistry::with_defaults());
        let pipeline_executor = Arc::new(
            PipelineExecutor::new(engine_registry.clone()).with_asr_registry(asr_registry.clone()),
        );
        let metrics = Arc::new(MetricsCollector::new());
        let voice_router = Arc::new(VoiceRouter::new((*engine_registry).clone()));
        let voice_identities = Arc::new(VoiceIdentityResolver::with_defaults());
        let voice_profiles = Arc::new(VoiceProfileStore::with_defaults());
        let model_catalog = Arc::new(ModelCatalogStore::with_curated_models());
        let pronunciation = Arc::new(PronunciationProcessor::new(ProcessorConfig::default()));
        let benchmark_store = Arc::new(BenchmarkStore::new());
        let voice_ci_store = ProfileStore::new();
        let worker_pool = Arc::new(WorkerPool::new());
        let worker_client = Arc::new(WorkerClient::new());
        let rate_limiter = RateLimiter::from_env();

        // Build the MCP server once and share it across all HTTP requests via Arc.
        // Previously, mcp_http.rs constructed a new McpServer on EVERY request, which
        // re-registered all engines, re-probed hardware, and re-initialized all stores
        // (6 Arc allocations + multiple Mutex write-locks) for each MCP call.
        let mcp_server = Arc::new(McpServer {
            engine_registry: engine_registry.clone(),
            voice_profiles: voice_profiles.clone(),
            asr_registry: asr_registry.clone(),
            model_catalog: model_catalog.clone(),
            pipeline_executor: pipeline_executor.clone(),
            pronunciation: pronunciation.clone(),
            hardware: hardware_profile.clone(),
            worker_pool: worker_pool.clone(),
        });

        Self {
            engine_registry,
            voice_router,
            voice_identities,
            voice_profiles,
            asr_registry,
            model_catalog,
            pronunciation,
            benchmark_store,
            voice_ci_store,
            worker_pool,
            worker_client,
            data_store,
            hardware_profile,
            pipeline_executor,
            api_key,
            metrics,
            rate_limiter,
            mcp_server,
        }
    }

    /// Override the rate limiter threshold (e.g. for deterministic testing).
    pub fn with_rate_limiter(mut self, rate_limiter: RateLimiter) -> Self {
        self.rate_limiter = rate_limiter;
        self
    }

    /// Override the model catalog store (e.g. simulated offline mode in tests).
    pub fn with_model_catalog(mut self, catalog: Arc<ModelCatalogStore>) -> Self {
        self.model_catalog = catalog;
        self
    }
}
