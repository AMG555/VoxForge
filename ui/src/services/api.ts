import {
  AbTestComparison,
  AbTestScenario,
  BlendVoicesPayload,
  BlendVoicesResponse,
  CatalogItem,
  CatalogListPayload,
  CloneVoicePayload,
  HardwareInfo,
  PipelineDefinition,
  RefineVoicePayload,
  RuntimeStatusResponse,
  SystemUpdateApplyResult,
  SystemUpdateInfo,
  Voice,
  VoiceProfile,
} from '../types';

let BASE_URL = '';

export const getApiBaseUrl = (): string => BASE_URL;

export const setApiBaseUrl = (url: string): void => {
  BASE_URL = url.trim().replace(/\/+$/, '');
};

export const getHeaders = (customHeaders: Record<string, string> = {}): Record<string, string> => {
  const token = typeof window !== 'undefined' ? localStorage.getItem('voxforge_api_key') : null;
  const headers: Record<string, string> = { ...customHeaders };
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }
  return headers;
};

export const isAbortError = (err: unknown): boolean =>
  (err instanceof DOMException && err.name === 'AbortError') ||
  (typeof err === 'object' &&
    err !== null &&
    'name' in err &&
    (err as { name?: string }).name === 'AbortError');

export const api = {
  async getHealth(): Promise<{
    status: string;
    version: string;
    neural_sidecar?: { running: boolean; url?: string };
  }> {
    const res = await fetch(`${BASE_URL}/health`, {
      headers: getHeaders(),
    });
    if (!res.ok) throw new Error('Failed to fetch health');
    return res.json();
  },

  async getReadiness(): Promise<HardwareInfo> {
    const res = await fetch(`${BASE_URL}/health/ready`, {
      headers: getHeaders(),
    });
    if (!res.ok) throw new Error('Failed to fetch readiness');
    return res.json();
  },

  async getVoices(language?: string, engine?: string): Promise<Voice[]> {
    const params = new URLSearchParams();
    if (language) params.append('language', language);
    if (engine) params.append('engine', engine);

    const res = await fetch(`${BASE_URL}/v1/voices?${params.toString()}`, {
      headers: getHeaders(),
    });
    if (!res.ok) throw new Error('Failed to fetch voices');
    const data = await res.json();
    return data.voices;
  },

  async synthesizeDirect(options: {
    input: string;
    voice: string;
    model?: string;
    language?: string;
    speed?: number;
    pitch?: number;
    instruct?: string;
    crossfade_ms?: number;
    num_step?: number;
    precision?: number;
    seed?: number;
    signal?: AbortSignal;
  }): Promise<Blob> {
    const res = await fetch(`${BASE_URL}/v1/audio/speech`, {
      method: 'POST',
      signal: options.signal,
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({
        model:
          options.model ||
          (options.voice.startsWith('qwen3') ||
          options.voice.startsWith('cloned-') ||
          options.voice.startsWith('designed-') ||
          options.voice.startsWith('persona-') ||
          options.voice.startsWith('custom-') ||
          options.voice.startsWith('blended-')
            ? 'qwen3-tts'
            : 'edge-tts'),
        input: options.input,
        voice: options.voice,
        language: options.language && options.language !== 'AUTO' ? options.language : undefined,
        speed: options.speed ?? 1.0,
        pitch: options.pitch ?? 0.0,
        instruct: options.instruct,
        num_step: options.num_step,
        precision: options.precision,
        crossfade_ms: options.crossfade_ms,
        seed: options.seed,
        response_format: 'wav',
      }),
    });

    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Synthesis failed' }));
      throw new Error(err.detail || err.title || err.error || 'Synthesis failed');
    }

    return res.blob();
  },

  async executePipeline(
    pipeline: PipelineDefinition,
    text?: string,
    signal?: AbortSignal
  ): Promise<Blob> {
    const res = await fetch(`${BASE_URL}/v1/pipeline/execute`, {
      method: 'POST',
      signal,
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({
        pipeline,
        input_text: text,
      }),
    });

    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Pipeline execution failed' }));
      throw new Error(err.detail || 'Pipeline execution failed');
    }

    return res.blob();
  },

  async runAbTest(
    scenario: AbTestScenario,
    signal?: AbortSignal
  ): Promise<AbTestComparison> {
    const res = await fetch(`${BASE_URL}/v1/qa/ab-test`, {
      method: 'POST',
      signal,
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({
        name: scenario.name,
        text: scenario.text,
        variant_a: {
          text: '',
          voice_id: scenario.variant_a.voice_id,
          speed: scenario.variant_a.speed ?? 1.0,
          pitch: scenario.variant_a.pitch ?? 0.0,
          format: 'wav',
        },
        variant_b: {
          text: '',
          voice_id: scenario.variant_b.voice_id,
          speed: scenario.variant_b.speed ?? 1.0,
          pitch: scenario.variant_b.pitch ?? 0.0,
          format: 'wav',
        },
      }),
    });

    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'A/B Test failed' }));
      throw new Error(err.detail || 'A/B Test failed');
    }

    return res.json();
  },

  async getCatalogDetails(type?: string, installedOnly?: boolean): Promise<CatalogListPayload> {
    const params = new URLSearchParams();
    if (type) params.append('type', type);
    if (installedOnly) params.append('installed_only', 'true');

    const res = await fetch(`${BASE_URL}/v1/catalog/models?${params.toString()}`, {
      headers: getHeaders(),
    });
    if (!res.ok) throw new Error('Failed to fetch catalog models');
    return res.json();
  },

  async getCatalogModels(type?: string, installedOnly?: boolean): Promise<CatalogItem[]> {
    const data = await this.getCatalogDetails(type, installedOnly);
    return data.models;
  },

  async installModel(id: string): Promise<CatalogItem> {
    const res = await fetch(`${BASE_URL}/v1/catalog/models/${encodeURIComponent(id)}/install`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Model install failed' }));
      throw new Error(err.detail || 'Model install failed');
    }
    return res.json();
  },

  async uninstallModel(id: string): Promise<CatalogItem> {
    const res = await fetch(`${BASE_URL}/v1/catalog/models/${encodeURIComponent(id)}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Model uninstall failed' }));
      throw new Error(err.detail || 'Model uninstall failed');
    }
    return res.json();
  },

  async cloneVoice(payload: CloneVoicePayload): Promise<VoiceProfile> {
    const res = await fetch(`${BASE_URL}/v1/voices/clone`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(payload),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Voice cloning failed' }));
      throw new Error(err.detail || err.title || 'Voice cloning failed');
    }
    return res.json();
  },

  async fetchRemoteAudioUrl(url: string): Promise<{
    audio_base64: string;
    filename: string;
    content_type: string;
    size_bytes: number;
    source_url: string;
    normalized_wav: boolean;
  }> {
    const res = await fetch(`${BASE_URL}/v1/voices/fetch-url`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ url }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Remote audio fetch failed' }));
      throw new Error(err.detail || err.title || 'Remote audio fetch failed');
    }
    return res.json();
  },

  async refineVoice(profileId: string, payload: RefineVoicePayload): Promise<VoiceProfile> {
    const res = await fetch(`${BASE_URL}/v1/voices/profiles/${encodeURIComponent(profileId)}/refine`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(payload),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Voice refinement failed' }));
      throw new Error(err.detail || 'Voice refinement failed');
    }
    return res.json();
  },

  async transcribeAudio(
    audioBase64: string,
    model = 'whisper-base',
    language?: string
  ): Promise<{ text: string; duration_seconds?: number; source?: string }> {
    const res = await fetch(`${BASE_URL}/v1/audio/transcriptions`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({
        audio_base64: audioBase64,
        model,
        language: language || undefined,
        response_format: 'json',
      }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Audio transcription failed' }));
      throw new Error(err.detail || 'Audio transcription failed');
    }
    const source = res.headers.get('X-VoxForge-ASR-Source') || undefined;
    const data = await res.json();
    return { ...data, source };
  },

  async refineText(
    text: string,
    options: { remove_fillers?: boolean; fix_repetitions?: boolean; punctuation_pass?: boolean } = {}
  ): Promise<{ refined_text: string; removed_fillers: number; cleaned_repetitions: number }> {
    const res = await fetch(`${BASE_URL}/v1/audio/refine`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({
        text,
        remove_fillers: options.remove_fillers ?? true,
        fix_repetitions: options.fix_repetitions ?? true,
        punctuation_pass: options.punctuation_pass ?? true,
      }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Text refinement failed' }));
      throw new Error(err.detail || 'Text refinement failed');
    }
    return res.json();
  },

  async speechToSpeech(payload: {
    audio_base64: string;
    target_voice: string;
    speed?: number;
    preserve_tempo?: boolean;
    language?: string;
    instruct?: string;
    seed?: number;
    precision?: number;
    signal?: AbortSignal;
  }): Promise<Blob> {
    const res = await fetch(`${BASE_URL}/v1/audio/speech-to-speech`, {
      method: 'POST',
      signal: payload.signal,
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({
        audio_base64: payload.audio_base64,
        target_voice: payload.target_voice,
        speed: payload.speed ?? 1.0,
        preserve_tempo: payload.preserve_tempo ?? true,
        language: payload.language && payload.language !== 'AUTO' ? payload.language : undefined,
        instruct: payload.instruct,
        seed: payload.seed,
        precision: payload.precision,
        response_format: 'wav',
      }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Speech-to-Speech conversion failed' }));
      throw new Error(err.detail || err.title || 'Speech-to-Speech conversion failed');
    }
    return res.blob();
  },

  async assessAudio(audio_base64: string): Promise<{
    duration_seconds: number;
    sample_rate: number;
    channels: number;
    peak_dbfs: number;
    rms_dbfs: number;
    snr_estimate_db: number;
    clipping_detected: boolean;
    is_silent: boolean;
    clarity_rating: 'excellent' | 'good' | 'fair' | 'noisy';
    recommendations: string[];
  }> {
    const res = await fetch(`${BASE_URL}/v1/voices/assess`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ audio_base64 }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Audio assessment failed' }));
      throw new Error(err.detail || 'Audio assessment failed');
    }
    return res.json();
  },

  async designVoice(payload: {
    prompt: string;
    name?: string;
    gender?: 'male' | 'female' | 'neutral';
    language?: string;
    director?: {
      energy: number;
      emotion: number;
      pace: number;
      intimacy: number;
      formality: number;
    };
  }): Promise<Voice> {
    const res = await fetch(`${BASE_URL}/v1/voices/design`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(payload),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Voice design failed' }));
      throw new Error(err.detail || 'Voice design failed');
    }
    return res.json();
  },

  async exportPersona(voiceId: string): Promise<any> {
    const res = await fetch(`${BASE_URL}/v1/voices/${encodeURIComponent(voiceId)}/export`, {
      headers: getHeaders(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Failed to export persona bundle' }));
      throw new Error(err.detail || 'Failed to export persona bundle');
    }
    return res.json();
  },

  async importPersona(bundle: any): Promise<Voice> {
    const res = await fetch(`${BASE_URL}/v1/voices/import`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(bundle),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Failed to import persona bundle' }));
      throw new Error(err.detail || 'Failed to import persona bundle');
    }
    return res.json();
  },

  async blendVoices(payload: BlendVoicesPayload): Promise<BlendVoicesResponse> {
    const res = await fetch(`${BASE_URL}/v1/voices/profiles/blend`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(payload),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Failed to blend voice profiles' }));
      throw new Error(err.detail || 'Failed to blend voice profiles');
    }
    return res.json();
  },

  async getDictionary(): Promise<{ count: number; entries: Array<{ term: string; replacement: string; note?: string }> }> {
    const res = await fetch(`${BASE_URL}/v1/pronunciation/dictionary`, {
      headers: getHeaders(),
    });
    if (!res.ok) throw new Error('Failed to fetch pronunciation dictionary');
    return res.json();
  },

  async upsertDictionary(entry: { term: string; replacement: string; note?: string }): Promise<any> {
    const res = await fetch(`${BASE_URL}/v1/pronunciation/dictionary`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(entry),
    });
    if (!res.ok) throw new Error('Failed to save pronunciation entry');
    return res.json();
  },

  async deleteDictionary(term: string): Promise<void> {
    const res = await fetch(`${BASE_URL}/v1/pronunciation/dictionary/${encodeURIComponent(term)}`, {
      method: 'DELETE',
      headers: getHeaders(),
    });
    if (!res.ok) throw new Error('Failed to delete pronunciation entry');
  },

  async applyPronunciation(text: string, language?: string): Promise<{ original: string; processed: string; replacements_count: number }> {
    const res = await fetch(`${BASE_URL}/v1/pronunciation/apply`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ text, language }),
    });
    if (!res.ok) throw new Error('Failed to apply pronunciation rules');
    return res.json();
  },

  async embedWatermark(audioBase64: string, payload?: number, strength?: number): Promise<{ audio_base64: string; payload: number; repetitions: number; duration_seconds: number }> {
    const res = await fetch(`${BASE_URL}/v1/audio/watermark`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ audio_base64: audioBase64, payload, strength }),
    });
    if (!res.ok) throw new Error('Failed to embed audio watermark');
    return res.json();
  },

  async verifyWatermark(audioBase64: string): Promise<{ is_detected: boolean; confidence: number; payload?: number; signature_match: boolean; sample_rate: number; duration_seconds: number }> {
    const res = await fetch(`${BASE_URL}/v1/audio/verify-watermark`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ audio_base64: audioBase64 }),
    });
    if (!res.ok) throw new Error('Failed to verify audio watermark');
    return res.json();
  },

  async getJobCheckpoints(): Promise<{ jobs: any[] }> {
    const res = await fetch(`${BASE_URL}/v1/jobs/checkpoints`, {
      headers: getHeaders(),
    });
    if (!res.ok) throw new Error('Failed to fetch job checkpoints');
    return res.json();
  },

  async resumeJob(jobId: string): Promise<{ manifest: any; remaining_indices: number[]; is_finished: boolean }> {
    const res = await fetch(`${BASE_URL}/v1/jobs/${encodeURIComponent(jobId)}/resume`, {
      method: 'POST',
      headers: getHeaders(),
    });
    if (!res.ok) throw new Error('Failed to resume job checkpoint');
    return res.json();
  },

  async dubScript(payload: {
    script: Array<{
      speaker: string;
      text: string;
      start_seconds?: number;
      speed?: number;
      pitch?: number;
    }>;
    gap_ms?: number;
    response_format?: 'wav' | 'pcm';
  }): Promise<{
    audio_base64: string;
    total_duration_seconds: number;
    lines: Array<{ speaker: string; start_seconds: number; duration_seconds: number }>;
    sample_rate: number;
  }> {
    const res = await fetch(`${BASE_URL}/v1/audio/dub`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(payload),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Script dubbing failed' }));
      throw new Error(err.detail || 'Script dubbing failed');
    }
    return res.json();
  },

  async verifyVoice(payload: {
    reference_audio_base64?: string;
    test_audio_base64: string;
    voice_profile_id?: string;
  }): Promise<{
    similarity_score: number;
    is_match: boolean;
    confidence_rating: 'high' | 'medium' | 'low';
    reference_f0_hz: number;
    test_f0_hz: number;
    pitch_match_percent: number;
    timbre_match_percent: number;
    formant_match_percent: number;
    cadence_match_percent: number;
    overall_biometric_score: number;
    recommendations: string[];
  }> {
    const res = await fetch(`${BASE_URL}/v1/voices/verify`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(payload),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Voice verification failed' }));
      throw new Error(err.detail || 'Voice verification failed');
    }
    return res.json();
  },

  async checkAudioSignal(audioBase64: string): Promise<{
    metrics: {
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
      snr_estimate_db: number;
    };
    is_clean: boolean;
    warnings_count: number;
    anomalies: Array<{
      kind: 'Clipping' | 'LongSilence' | 'RelativeVolumeJump' | 'DcOffset' | 'EmptyAudio' | 'InvalidSample';
      severity: 'Info' | 'Warning' | 'Critical';
      timestamp_seconds: number;
      duration_seconds?: number;
      value: number;
      message: string;
    }>;
    scan_bounded: boolean;
  }> {
    const res = await fetch(`${BASE_URL}/v1/audio/check`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ audio_base64: audioBase64 }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Audio signal check failed' }));
      throw new Error(err.detail || 'Audio signal check failed');
    }
    return res.json();
  },

  async parseManuscript(options: {
    manuscript_text: string;
    max_words_per_chapter?: number;
  }): Promise<{
    title: string;
    total_chapters: number;
    total_words: number;
    estimated_duration_minutes: number;
    chapters: Array<{
      index: number;
      title: string;
      text: string;
      voice_id?: string;
      speed?: number;
      content_hash?: string;
      word_count?: number;
      estimated_duration_minutes?: number;
      sha256_hash?: string;
    }>;
  }> {
    const res = await fetch(`${BASE_URL}/v1/jobs/parse-manuscript`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify(options),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Manuscript parse failed' }));
      throw new Error(err.detail || 'Manuscript parse failed');
    }
    return res.json();
  },

  async checkSystemUpdate(force = false): Promise<SystemUpdateInfo> {
    const query = force ? '?force=true' : '';
    const res = await fetch(`${BASE_URL}/v1/system/update${query}`, {
      headers: getHeaders(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Failed checking for updates' }));
      throw new Error(err.detail || 'Failed checking for updates');
    }
    return res.json();
  },

  async applySystemUpdate(autoRestart = false): Promise<SystemUpdateApplyResult> {
    const res = await fetch(`${BASE_URL}/v1/system/update/apply`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ auto_restart: autoRestart }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Update installation failed' }));
      throw new Error(err.detail || 'Update installation failed');
    }
    return res.json();
  },

  async restartSystemServer(): Promise<{ status: string; message: string }> {
    const res = await fetch(`${BASE_URL}/v1/system/restart`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({}),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Server restart failed' }));
      throw new Error(err.detail || 'Server restart failed');
    }
    return res.json();
  },

  async getRuntimeStatus(): Promise<RuntimeStatusResponse> {
    const res = await fetch(`${BASE_URL}/v1/system/runtimes`, {
      headers: getHeaders(),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Failed querying host runtimes' }));
      throw new Error(err.detail || 'Failed querying host runtimes');
    }
    return res.json();
  },

  async startSidecar(
    backend: 'auto' | 'omnivoice' | 'chatterbox' | 'cosyvoice' | 'f5tts' | 'kokoro' | 'kitten' | 'canto' = 'auto',
    port?: number
  ): Promise<{ status: string; backend: string; port: number; url: string; message: string }> {
    const res = await fetch(`${BASE_URL}/v1/system/sidecar/start`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ backend, port }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: 'Failed starting neural sidecar' }));
      throw new Error(err.detail || 'Failed starting neural sidecar');
    }
    return res.json();
  },

  async installNativeRuntime(
    target: 'piper'
  ): Promise<{ status: string; target: string; python_required: boolean; binary_path: string; message: string }> {
    const res = await fetch(`${BASE_URL}/v1/system/runtimes/install-native`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ target }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: `Failed installing native ${target} binary` }));
      throw new Error(err.detail || `Failed installing native ${target} binary`);
    }
    return res.json();
  },

  async installRuntimePackage(
    pkg: 'kokoro' | 'piper' | 'whisper' | 'chatterbox' | 'kitten' | 'canto'
  ): Promise<{ status: string; package: string; installed_specs: string[]; message: string }> {
    const res = await fetch(`${BASE_URL}/v1/system/runtimes/install`, {
      method: 'POST',
      headers: getHeaders({ 'Content-Type': 'application/json' }),
      body: JSON.stringify({ package: pkg }),
    });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ detail: `Failed installing ${pkg} runtime` }));
      throw new Error(err.detail || `Failed installing ${pkg} runtime`);
    }
    return res.json();
  },
};
