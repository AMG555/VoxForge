export type NodeType =
  | 'text_input'
  | 'character_voice'
  | 'speaker_parser'
  | 'voice_assigner'
  | 'synthesizer'
  | 'audio_filter'
  | 'audio_merge'
  | 'output_sink'
  | 'asr_transcriber'
  | 'diarization'
  | 'document_chunker'
  | 'audio_time_stretch'
  | 'audio_mux'
  | 'watermark'
  | 'voice_conversion';

export type NodeExecutionStatus = 'idle' | 'waiting' | 'running' | 'success' | 'error' | 'bypassed';

export interface NodeExecutionState {
  status: NodeExecutionStatus;
  latencyMs?: number;
  message?: string;
  outputPreview?: any;
}

export interface PipelineNode {
  id: string;
  name: string;
  node_type: NodeType;
  params: Record<string, any>;
  position?: { x: number; y: number };
  disabled?: boolean;
}


export interface PipelineEdge {
  id: string;
  from_node: string;
  to_node: string;
  from_port?: string;
  to_port?: string;
}

export interface PipelineDefinition {
  id: string;
  name: string;
  description?: string;
  nodes: PipelineNode[];
  edges: PipelineEdge[];
  created_at: string;
  updated_at: string;
}

export interface Voice {
  id: string;
  name: string;
  engine_id: string;
  language: string;
  gender: 'male' | 'female' | 'neutral';
  sample_rate_hz: number;
  tags: string[];
  description?: string;
  // Voice cloning quality and versioning fields
  similarity_score?: number;
  version?: number;
  sample_count?: number;
  refined_at?: string;
}

export interface HardwareInfo {
  status: string;
  hardware_tier: string;
  registered_engines: string[];
  arch: string;
  os: string;
}

export interface AudioQualityMetrics {
  duration_seconds: number;
  sample_rate: number;
  channels: number;
  total_samples: number;
  peak_amplitude: number;
  peak_dbfs: number;
  rms_amplitude: number;
  rms_dbfs: number;
  clipping_samples_count: number;
  is_silent: boolean;
}

export interface VariantResult {
  engine_id: string;
  voice_id: string;
  latency_ms: number;
  audio_duration_seconds: number;
  realtime_factor: number;
  metrics: AudioQualityMetrics;
}

export interface AbTestComparison {
  scenario_name: string;
  variant_a: VariantResult;
  variant_b: VariantResult;
  latency_delta_ms: number;
  speedup_ratio: number;
  rms_delta_db: number;
  faster_variant: string;
  recommended_variant: string;
  summary: string;
}

export interface AbTestScenario {
  name: string;
  text: string;
  variant_a: {
    voice_id: string;
    speed?: number;
    pitch?: number;
  };
  variant_b: {
    voice_id: string;
    speed?: number;
    pitch?: number;
  };
}

export interface CatalogItem {
  id: string;
  name: string;
  description: string;
  model_type: 'tts' | 'asr' | 'vad' | 'diarizer';
  format: string;
  size_bytes: number;
  min_ram_mb: number;
  requires_gpu: boolean;
  supported_languages: string[];
  status: 'available' | 'downloading' | 'installed' | 'error' | 'Available' | 'Downloading' | 'Installed' | 'Error' | string;
  installed_at?: string;
  local_path?: string;
}

export interface EngineHealthInfo {
  id: string;
  name: string;
  status: 'ready' | 'weights_required' | 'key_required' | 'mock' | 'error';
  status_label: string;
  description: string;
  action_type?: 'none' | 'install_model' | 'configure_key';
  action_target?: string;
}

/** Full voice profile returned by clone / refine / profile CRUD endpoints. */
export interface VoiceProfile {
  id: string;
  name: string;
  engine_id: string;
  description?: string;
  language: string;
  gender?: 'male' | 'female' | 'neutral';
  reference_audio_path?: string;
  reference_audio_base64?: string;
  reference_transcript?: string;
  embedding?: number[];
  clone_capabilities?: string[];
  similarity_score?: number;
  sample_count: number;
  version: number;
  metadata: Record<string, string>;
  created_at: string;
  refined_at?: string;
}

export interface CloneVoicePayload {
  name: string;
  engine_id?: string;
  reference_audio_base64?: string;
  reference_audio_url?: string;
  reference_transcript?: string;
  additional_reference_audio_base64?: string[];
  language?: string;
  gender?: 'male' | 'female' | 'neutral';
  description?: string;
  metadata?: Record<string, string>;
}

export interface RefineVoicePayload {
  audio_base64: string;
  transcript?: string;
}

export interface DirectorTaxonomy {
  energy: number;
  emotion: number;
  pace: number;
  intimacy: number;
  formality: number;
}

export interface VoiceDesignPayload {
  prompt: string;
  name?: string;
  gender?: 'male' | 'female' | 'neutral';
  language?: string;
  director?: DirectorTaxonomy;
}

export interface PersonaBundle {
  format_version: number;
  profile: {
    id: string;
    name: string;
    engine_id: string;
    description?: string;
    language: string;
    gender?: 'male' | 'female' | 'neutral';
    embedding?: number[];
    metadata?: Record<string, string>;
  };
  prosody_defaults?: DirectorTaxonomy;
  avatar_base64?: string;
}

export interface BlendVoicesPayload {
  name: string;
  profile_id_a: string;
  profile_id_b: string;
  weight_b?: number;
  description?: string;
}

export interface BlendVoicesResponse {
  blended_profile: VoiceProfile;
  interpolation_weight_b: number;
  cosine_similarity_to_a: number;
  cosine_similarity_to_b: number;
}

export interface SystemUpdateInfo {
  current_version: string;
  latest_version?: string;
  latest_tag?: string;
  update_available: boolean;
  check_status: 'ok' | 'disabled' | 'rate_limited' | 'offline' | string;
  release_name?: string;
  release_url: string;
  release_notes?: string;
  published_at?: string;
  checked_at: string;
  platform: string;
  arch: string;
  asset_name: string;
  asset_url?: string;
  asset_size_bytes?: number;
  checksum_sha256?: string;
  checksums_url?: string;
  install_mode: 'cargo_dev' | 'windows_installer' | 'standalone_binary' | string;
  executable_path: string;
  can_self_update: boolean;
  update_hint: string;
  error?: string;
}

export interface SystemUpdateApplyResult {
  status: 'applied' | 'up_to_date' | 'dev_build' | string;
  previous_version: string;
  new_version: string;
  asset_name: string;
  sha256_verified: boolean;
  sha256: string;
  executable_path: string;
  restart_required: boolean;
  restarting: boolean;
  message: string;
}

export interface RuntimeEndpointStatus {
  running: boolean;
  url?: string;
}

export interface PythonPackagesStatus {
  torch: boolean;
  whisper: boolean;
  omnivoice: boolean;
  kokoro: boolean;
  piper: boolean;
  edge_tts: boolean;
  chatterbox?: boolean;
  kitten?: boolean;
  canto?: boolean;
}

export interface RuntimeStatusResponse {
  rust_core_ready?: boolean;
  python_available: boolean;
  python_executable?: string;
  sidecar_script_available: boolean;
  sidecar_script_path?: string;
  neural_sidecar: RuntimeEndpointStatus;
  kokoro_sidecar: RuntimeEndpointStatus;
  piper_cli_available: boolean;
  piper_native_installed?: boolean;
  piper_binary_path?: string;
  ffmpeg_available: boolean;
  python_packages: PythonPackagesStatus;
  installed_model_files: string[];
}

export interface DesktopConfig {
  setup_completed: boolean;
  runtime_profile: 'rust_core' | 'core_neural' | 'full_cloning' | 'remote' | string;
  runtime_dir: string;
  custom_python?: string | null;
  remote_server_url?: string | null;
  telemetry_opt_in: boolean;
}

export interface DiscoveredPython {
  path: string;
  version: string;
  label: string;
  is_managed: boolean;
}

export interface RuntimeProfileSpec {
  id: 'rust_core' | 'core_neural' | 'full_cloning' | string;
  title: string;
  subtitle: string;
  disk_estimate_gb: number;
  requires_python: boolean;
  packages: string[];
  recommended: boolean;
}

export interface HardwareAccelerator {
  name: string;
  vram_mb: number;
  has_cuda: boolean;
  has_mps: boolean;
  has_directml: boolean;
}

export interface HardwareProfileFull {
  os: string;
  arch: string;
  cpu_brand: string;
  cpu_physical_cores: number;
  cpu_logical_threads: number;
  total_memory_mb: number;
  available_memory_mb: number;
  gpus: HardwareAccelerator[];
  assigned_tier: string;
  recommended_engines: string[];
}

export interface SetupStatusResponse {
  config: DesktopConfig;
  default_runtime_dir: string;
  managed_venv_python?: string | null;
  managed_venv_ready: boolean;
  uv_available: boolean;
  uv_path?: string | null;
  disk_available_bytes: number;
  disk_total_bytes: number;
  hardware: HardwareProfileFull;
  discovered_pythons: DiscoveredPython[];
  installed_packages: PythonPackagesStatus;
  profiles: RuntimeProfileSpec[];
}

export interface SetupStepProgress {
  id: string;
  label: string;
  status: 'pending' | 'running' | 'done' | 'failed' | 'cancelled' | string;
  detail?: string | null;
}

export interface InstallJobSnapshot {
  job_id?: string | null;
  status: 'idle' | 'running' | 'completed' | 'failed' | 'cancelled' | string;
  profile: string;
  runtime_dir: string;
  current_step: string;
  percent: number;
  elapsed_secs: number;
  steps: SetupStepProgress[];
  logs: string[];
  error?: string | null;
}



