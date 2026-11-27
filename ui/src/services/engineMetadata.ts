import { HardwareInfo, HardwareProfile, SystemFitAssessment } from '../types';

export interface EngineReferenceInfo {
  id: string;
  name: string;
  category: string;
  description: string;
  docsUrl: string;
  docsLabel: string;
  license: string;
  footprint: string;
  minRamMb: number;
  minVramMb: number;
  pros: string[];
  cons: string[];
  bestFor: string[];
}

export const ENGINE_REFERENCE_MAP: Record<string, EngineReferenceInfo> = {
  'edge-tts': {
    id: 'edge-tts',
    name: 'Microsoft Edge Cloud Relay',
    category: 'Zero-Python Rust Cloud Neural',
    description:
      'Pure-Rust WebSocket relay streaming 130+ high-naturalness Microsoft Azure neural voices (incl. English, Malayalam, Hindi, Arabic, Cantonese HK, Japanese) with sub-second response and zero local model weight footprint.',
    docsUrl:
      'https://learn.microsoft.com/en-us/azure/ai-services/speech-service/language-support?tabs=tts',
    docsLabel: 'Microsoft Neural Voice Docs',
    license: 'Cloud Service Terms',
    footprint: '0 MB local · Zero Python',
    minRamMb: 128,
    minVramMb: 0,
    pros: [
      'Instant startup with zero Python or GB weight downloads',
      'Covers 130+ languages & regional scripts accurately',
      'Clean studio-ready prosody even on low-spec laptops',
    ],
    cons: [
      'Requires active internet connection (not offline/air-gapped)',
      'Preset voices only unless paired with VoxForge cross-lingual DSP cloning',
    ],
    bestFor: [
      'Zero-setup instant multilingual speech (Malayalam, Hindi, Arabic, Cantonese)',
      'Low-RAM / CPU-only laptops without NVIDIA GPUs',
    ],
  },
  'piper-tts': {
    id: 'piper-tts',
    name: 'Piper Local Neural ONNX Engine',
    category: 'Offline Native C++ / Zero Python',
    description:
      'Fast local VITS neural ONNX synthesis running natively via pre-built C++ Piper runtime without Python. Delivers deterministic, completely offline privacy with compact ~27–60 MB per-voice weights.',
    docsUrl: 'https://github.com/rhasspy/piper',
    docsLabel: 'Piper Official Repo',
    license: 'MIT (Binary) / Per-Voice',
    footprint: '~22 MB CLI + ~60 MB/voice · Zero Python',
    minRamMb: 512,
    minVramMb: 0,
    pros: [
      '100% offline, private, and deterministic with zero Python dependency',
      'Faster than real-time on standard laptop CPUs',
      'Tiny disk & RAM footprint (~60 MB per voice)',
    ],
    cons: [
      'Single-speaker preset per weight file (no zero-shot voice cloning)',
      'Requires downloading at least one `.onnx` voice weight from Model Catalog',
    ],
    bestFor: [
      'Air-gapped offline narration & local CPU voice agents',
      'Ultra-low-latency desktop or embedded deployment',
    ],
  },
  kokoro: {
    id: 'kokoro',
    name: 'Kokoro-82M Neural Engine',
    category: 'Lightweight Local StyleTTS2 Sidecar',
    description:
      'Compact 82M-parameter Apache-2.0 neural synthesis engine offering natural conversational inflection across English, Japanese, Mandarin, French, and Spanish via local sidecar (:8880).',
    docsUrl: 'https://huggingface.co/hexgrad/Kokoro-82M',
    docsLabel: 'Kokoro-82M Model Card',
    license: 'Apache-2.0',
    footprint: '~310 MB weights + Optional Kokoro Python sidecar',
    minRamMb: 2048,
    minVramMb: 0,
    pros: [
      'Top naturalness-to-size ratio at just 82M parameters (Apache-2.0)',
      'Comfortable on CPU or entry GPUs without 6+ GB VRAM',
    ],
    cons: [
      'Requires optional Python Kokoro runtime package & port 8880 sidecar',
      'Curated speaker presets only (not open-ended reference cloning)',
    ],
    bestFor: [
      'Studio audiobooks & natural narration on standard CPUs / 4 GB GPUs',
      'Commercial Apache-2.0 offline speech generation',
    ],
  },
  'qwen3-tts': {
    id: 'qwen3-tts',
    name: 'Zero-Shot Voice Cloning & Neural Sidecar',
    category: 'Multi-Backend Cloning + Acoustic Pipeline',
    description:
      'VoxForge flagship zero-shot voice cloning pipeline: runs fast Rust acoustic embedding + cross-lingual synthesis natively, and automatically upgrades to PyTorch neural sidecar backends (OmniVoice 600+, Chatterbox-Turbo/23Lang, F5-TTS, KittenTTS 2, Canto-TTS) when running on port 9000.',
    docsUrl: 'https://huggingface.co/k2-fsa/OmniVoice',
    docsLabel: 'OmniVoice / Neural Docs',
    license: 'MIT / Apache-2.0',
    footprint: '0 MB native Rust mode · 0.5–3.3 GB optional PyTorch sidecars',
    minRamMb: 1024,
    minVramMb: 4096,
    pros: [
      'Instant Rust cross-lingual cloning works zero-Python out of the box',
      'Unlocks true neural cloning (OmniVoice, Chatterbox-Turbo, F5-TTS) with sidecar',
      'Supports 3s cloning, Director prosody controls & Broadcast DSP mastering',
    ],
    cons: [
      'True neural PyTorch cloning (OmniVoice/Chatterbox/F5-TTS) needs ~4 GB VRAM for real-time speed',
      'Without PyTorch sidecar, uses acoustic prosody mirroring + Edge-TTS cross-lingual base',
    ],
    bestFor: [
      'Custom cloned speaker voices & 646-language cross-lingual dubbing',
      'Machines with >=4 GB CUDA/MPS GPU (or CPU batch mode via KittenTTS/Canto-TTS)',
    ],
  },
  'openai-router': {
    id: 'openai-router',
    name: 'OpenAI & OpenRouter Cloud Gateway',
    category: 'BYO-Key Cloud Speech Router',
    description:
      'Direct API router dispatching `/v1/audio/speech` requests to OpenAI TTS (`tts-1`, `tts-1-hd`, `gpt-4o-mini-tts`) or compatible OpenRouter endpoints using your own API key.',
    docsUrl: 'https://platform.openai.com/docs/guides/text-to-speech',
    docsLabel: 'OpenAI TTS Docs',
    license: 'Pay-per-use API Key',
    footprint: '0 MB local',
    minRamMb: 128,
    minVramMb: 0,
    pros: [
      'Zero local GPU or disk overhead on host machine',
      'Drop-in compatibility for OpenAI commercial voices',
    ],
    cons: [
      'Requires paid API key (`OPENAI_API_KEY`) and sends text externally',
      'No local zero-shot cloning',
    ],
    bestFor: [
      'Production cloud fallback or hybrid BYO-key enterprise teams',
    ],
  },
  'mock-tts': {
    id: 'mock-tts',
    name: 'In-Memory Synthesis Test Harness',
    category: 'Developer Diagnostic Signal Generator',
    description:
      'Deterministic 440 Hz mathematical waveform generator used exclusively for CI regression tests, pipeline dry-runs, and audio transport verification (never speech).',
    docsUrl: 'https://github.com/AMG555/VoxForge/blob/main/docs/architecture.md',
    docsLabel: 'VoxForge Architecture Docs',
    license: 'Apache-2.0 OR MIT',
    footprint: '0 MB',
    minRamMb: 64,
    minVramMb: 0,
    pros: [
      'Sub-millisecond deterministic WAV output for automated QA tests',
    ],
    cons: [
      'Produces synthetic test tone rather than human speech',
    ],
    bestFor: [
      'Automated API & DSP pipeline verification only',
    ],
  },
};

export function assessEngineSystemFit(
  engineId: string,
  hardware: HardwareProfile | HardwareInfo | null | undefined
): SystemFitAssessment {
  const ref = ENGINE_REFERENCE_MAP[engineId];
  const profile: HardwareProfile | undefined =
    hardware && 'total_memory_mb' in hardware
      ? (hardware as HardwareProfile)
      : (hardware as HardwareInfo | null | undefined)?.hardware_profile;

  if (!ref) {
    return {
      tier: 'good',
      score: 75,
      label: 'Compatible',
      reason: 'Ready on current host configuration.',
    };
  }

  if (!profile) {
    return {
      tier: ref.minVramMb === 0 ? 'optimal' : 'good',
      score: ref.minVramMb === 0 ? 92 : 76,
      label: ref.minVramMb === 0 ? 'CPU / Cloud Ready' : 'Host Probe Pending',
      reason: `Minimum ${(ref.minRamMb / 1024).toFixed(1)} GB RAM.`,
    };
  }

  const totalRam = profile.total_memory_mb;
  const maxAccelVram = (profile.gpus || [])
    .filter((g) => g.has_cuda || g.has_mps)
    .reduce((max, g) => Math.max(max, g.vram_mb), 0);

  if (totalRam > 0 && totalRam < ref.minRamMb) {
    return {
      tier: 'insufficient',
      score: 15,
      label: 'Low RAM',
      reason: `Needs ${(ref.minRamMb / 1024).toFixed(1)} GB RAM; host detects ${(totalRam / 1024).toFixed(1)} GB.`,
    };
  }

  if (ref.minVramMb > 0) {
    if (maxAccelVram >= ref.minVramMb) {
      const gpuName = profile.gpus.find((g) => g.has_cuda || g.has_mps)?.name || 'GPU';
      return {
        tier: 'optimal',
        score: 94,
        label: 'Best for Your System',
        reason: `${gpuName} (${(maxAccelVram / 1024).toFixed(1)} GB VRAM) accelerates neural sidecars smoothly.`,
      };
    }
    if (maxAccelVram >= 3500) {
      const gpuName = profile.gpus.find((g) => g.has_cuda || g.has_mps)?.name || 'GPU';
      return {
        tier: 'good',
        score: 82,
        label: 'Runs Well (Tight VRAM)',
        reason: `${gpuName} (${(maxAccelVram / 1024).toFixed(1)} GB VRAM) handles Chatterbox-Turbo / Kokoro; heavier models need CPU offload.`,
      };
    }
    return {
      tier: engineId === 'qwen3-tts' ? 'good' : 'cpu_slow',
      score: engineId === 'qwen3-tts' ? 78 : 55,
      label: engineId === 'qwen3-tts' ? 'Rust Native Ready · GPU Ideal' : 'CPU Slow',
      reason: `Rust cross-lingual cloning runs cleanly now; optional PyTorch diffusion models recommend ${(ref.minVramMb / 1024).toFixed(1)} GB CUDA/MPS VRAM.`,
    };
  }

  const ramGb = (totalRam / 1024).toFixed(1);
  return {
    tier: 'optimal',
    score: 96,
    label: 'Best for Your System',
    reason: `Ideal fit for your ${ramGb} GB RAM & ${profile.cpu_logical_threads}-thread CPU (${profile.simd?.avx2 ? 'AVX2 SIMD' : profile.simd?.neon ? 'ARM NEON' : 'CPU'}).`,
  };
}

export function getFitBadgeClasses(tier?: SystemFitAssessment['tier']): string {
  switch (tier) {
    case 'optimal':
      return 'bg-emerald-500/15 text-emerald-300 border-emerald-500/35';
    case 'good':
      return 'bg-sky-500/15 text-sky-300 border-sky-500/35';
    case 'cpu_slow':
      return 'bg-amber-500/15 text-amber-300 border-amber-500/35';
    case 'insufficient':
    default:
      return 'bg-red-500/15 text-red-300 border-red-500/35';
  }
}
