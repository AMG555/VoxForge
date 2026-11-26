import React, { useEffect, useState } from 'react';
import {
  Cpu,
  Server,
  Zap,
  CheckCircle2,
  Activity,
  ShieldCheck,
  ArrowUpCircle,
  Loader2,
  Play,
  Download,
  AlertTriangle,
} from 'lucide-react';
import { HardwareInfo, RuntimeStatusResponse, SystemUpdateInfo } from '../../types';
import { api } from '../../services/api';

interface HardwareInspectorProps {
  hardware: HardwareInfo | null;
  onNavigateTab?: (tab: 'workflows' | 'canvas' | 'timeline' | 'voices' | 'catalog' | 'hardware' | 'qa') => void;
  updateInfo?: SystemUpdateInfo | null;
  onOpenUpdates?: () => void;
}

interface EngineDescriptor {
  name: string;
  category: string;
  description: string;
  statusBadge: {
    label: string;
    bg: string;
    text: string;
    border: string;
  };
  action?: {
    label: string;
    tab: 'catalog' | 'voices';
  };
}

const ENGINE_DESCRIPTORS: Record<string, EngineDescriptor> = {
  'edge-tts': {
    name: 'Microsoft Edge Cloud Relay',
    category: 'Cloud WebSocket Relay',
    description: 'High-fidelity zero-cost neural speech relay with real-time streaming chunking.',
    statusBadge: {
      label: 'ONLINE & READY',
      bg: 'bg-emerald-500/10',
      text: 'text-emerald-400',
      border: 'border-emerald-500/30',
    },
  },
  'piper-tts': {
    name: 'Piper Local Neural ONNX Engine',
    category: 'Offline On-Device Neural',
    description: 'Ultra-fast native ONNX runtime running locally without cloud dependency. Requires model weights.',
    statusBadge: {
      label: 'WEIGHTS CONFIGURED',
      bg: 'bg-amber-500/10',
      text: 'text-amber-400',
      border: 'border-amber-500/30',
    },
    action: {
      label: 'Manage in Catalog',
      tab: 'catalog',
    },
  },
  kokoro: {
    name: 'Kokoro-82M Neural Engine',
    category: 'Local ONNX / Python Sidecar',
    description: '82M-parameter natural voice engine (af_heart, af_bella, etc.). Requires local Kokoro sidecar running on port 8880.',
    statusBadge: {
      label: 'SIDECAR REQUIRED',
      bg: 'bg-amber-500/10',
      text: 'text-amber-400',
      border: 'border-amber-500/30',
    },
    action: {
      label: 'Open Catalog',
      tab: 'catalog',
    },
  },
  'openai-router': {
    name: 'OpenRouter & OpenAI Cloud Gateway',
    category: 'Upstream Model Router',
    description: 'Dynamic load-balancer and fall-through proxy for OpenRouter and OpenAI neural voices.',
    statusBadge: {
      label: 'ROUTER READY',
      bg: 'bg-sky-500/10',
      text: 'text-sky-400',
      border: 'border-sky-500/30',
    },
    action: {
      label: 'Configure Key',
      tab: 'voices',
    },
  },
  'qwen3-tts': {
    name: 'Qwen3 Zero-Shot Voice Cloner',
    category: 'Acoustic Conditioning Transformer',
    description: 'Speaker embedding extraction and zero-shot voice cloning transformer pipeline.',
    statusBadge: {
      label: 'CLONING READY',
      bg: 'bg-purple-500/10',
      text: 'text-purple-400',
      border: 'border-purple-500/30',
    },
  },
  'mock-tts': {
    name: 'In-Memory Synthesis Benchmark Harness',
    category: 'Mathematical Test Harness',
    description: 'Deterministic sine wave harmonic generator for unit testing, buffer verification, and latency benchmarks.',
    statusBadge: {
      label: 'BENCHMARK HARNESS',
      bg: 'bg-slate-500/10',
      text: 'text-slate-400',
      border: 'border-slate-500/30',
    },
  },
};

export const HardwareInspector: React.FC<HardwareInspectorProps> = ({
  hardware,
  onNavigateTab,
  updateInfo,
  onOpenUpdates,
}) => {
  const [runtimes, setRuntimes] = useState<RuntimeStatusResponse | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [actionNotice, setActionNotice] = useState<{ text: string; isError?: boolean } | null>(null);

  const refreshRuntimes = async () => {
    try {
      const data = await api.getRuntimeStatus();
      setRuntimes(data);
    } catch {
      // Non-fatal in offline dev
    }
  };

  useEffect(() => {
    refreshRuntimes();
  }, []);

  const handleStartSidecar = async (backend: 'auto' | 'omnivoice' | 'kokoro', port: number) => {
    setBusyAction(`start-${backend}`);
    setActionNotice(null);
    try {
      const res = await api.startSidecar(backend, port);
      setActionNotice({ text: res.message });
      setTimeout(refreshRuntimes, 1200);
    } catch (err: any) {
      setActionNotice({ text: err?.message || 'Failed starting sidecar', isError: true });
    } finally {
      setBusyAction(null);
    }
  };

  const handleInstallRuntime = async (pkg: 'kokoro' | 'piper' | 'whisper') => {
    setBusyAction(`install-${pkg}`);
    setActionNotice(null);
    try {
      const res = await api.installRuntimePackage(pkg);
      setActionNotice({ text: res.message });
      await refreshRuntimes();
    } catch (err: any) {
      setActionNotice({ text: err?.message || `Failed installing ${pkg}`, isError: true });
    } finally {
      setBusyAction(null);
    }
  };

  const registeredEngines = hardware?.registered_engines || [
    'edge-tts',
    'kokoro',
    'piper-tts',
    'openai-router',
    'qwen3-tts',
    'mock-tts',
  ];

  const hasAnyPiperWeights = Boolean(
    runtimes?.installed_model_files.some((f) => f.endsWith('.onnx') && !f.includes('whisper') && !f.includes('silero'))
  );

  return (
    <div className="flex-1 p-6 lg:p-8 pb-36 overflow-y-auto bg-[#0B0E14] space-y-6">
      <div className="w-full max-w-6xl mx-auto space-y-6">
        <div>
          <span className="text-[11px] font-mono uppercase text-amber-500 tracking-wider">
            System Telemetry & Engines
          </span>
          <h2 className="text-xl font-bold text-white mt-0.5">Hardware Capability Profile</h2>
          <p className="text-xs text-[#94A3B8] font-mono mt-1">
            Automated host probe detecting SIMD vector extensions, accelerators, and active synthesis engines.
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="bg-[#121820] border border-[#242E3D] p-4 rounded-lg space-y-2">
            <div className="flex items-center space-x-2 text-[#94A3B8]">
              <Server className="w-4 h-4 text-sky-400" />
              <span className="text-xs font-mono uppercase">Assigned Tier</span>
            </div>
            <p className="text-lg font-bold font-mono text-white">
              {hardware?.hardware_tier || 'TIER_3_PRO'}
            </p>
            <p className="text-[11px] text-[#64748B]">Optimal local and cloud engines unlocked</p>
          </div>

          <div className="bg-[#121820] border border-[#242E3D] p-4 rounded-lg space-y-2">
            <div className="flex items-center space-x-2 text-[#94A3B8]">
              <Cpu className="w-4 h-4 text-emerald-400" />
              <span className="text-xs font-mono uppercase">Platform & Kernel</span>
            </div>
            <p className="text-lg font-bold font-mono text-white">
              {hardware ? `${hardware.os} (${hardware.arch})` : 'Windows (x86_64)'}
            </p>
            <p className="text-[11px] text-[#64748B]">Native kernel SIMD vector extensions</p>
          </div>

          <div className="bg-[#121820] border border-[#242E3D] p-4 rounded-lg space-y-2">
            <div className="flex items-center space-x-2 text-[#94A3B8]">
              <Zap className="w-4 h-4 text-amber-400" />
              <span className="text-xs font-mono uppercase">Registered Engines</span>
            </div>
            <p className="text-lg font-bold font-mono text-white">
              {registeredEngines.length} Online
            </p>
            <p className="text-[11px] text-[#64748B]">Multi-engine dispatch active</p>
          </div>
        </div>

        <div className="bg-[#121820] border border-[#242E3D] rounded-lg overflow-hidden">
          <div className="px-4 py-3 border-b border-[#242E3D] bg-[#0B0E14]/40 flex items-center justify-between">
            <h3 className="text-xs font-semibold text-white uppercase tracking-wider flex items-center space-x-2">
              <Activity className="w-4 h-4 text-amber-500" />
              <span>Registered Speech Synthesis Engines</span>
            </h3>
            <span className="text-[10px] font-mono text-[#64748B]">
              Real-time diagnostic telemetry
            </span>
          </div>

          <div className="divide-y divide-[#242E3D]">
            {registeredEngines.map((engineId) => {
              const baseDesc = ENGINE_DESCRIPTORS[engineId] || {
                name: engineId,
                category: 'Synthesis Engine',
                description: 'Speech generation engine registered in kernel.',
                statusBadge: {
                  label: 'ONLINE',
                  bg: 'bg-emerald-500/10',
                  text: 'text-emerald-400',
                  border: 'border-emerald-500/30',
                },
              };

              let statusBadge = baseDesc.statusBadge;
              let isReady = true;
              if (engineId === 'kokoro' && runtimes) {
                isReady = runtimes.kokoro_sidecar.running;
                statusBadge = isReady
                  ? { label: 'SIDECAR LIVE (:8880)', bg: 'bg-emerald-500/10', text: 'text-emerald-400', border: 'border-emerald-500/30' }
                  : runtimes.python_packages.kokoro
                  ? { label: 'INSTALLED (STOPPED)', bg: 'bg-amber-500/10', text: 'text-amber-300', border: 'border-amber-500/30' }
                  : { label: 'NOT INSTALLED', bg: 'bg-red-500/10', text: 'text-red-300', border: 'border-red-500/30' };
              } else if (engineId === 'piper-tts' && runtimes) {
                isReady = runtimes.piper_cli_available && hasAnyPiperWeights;
                statusBadge = isReady
                  ? { label: 'RUNTIME & MODEL READY', bg: 'bg-emerald-500/10', text: 'text-emerald-400', border: 'border-emerald-500/30' }
                  : !runtimes.piper_cli_available
                  ? { label: 'PIPER CLI MISSING', bg: 'bg-red-500/10', text: 'text-red-300', border: 'border-red-500/30' }
                  : { label: 'WEIGHTS MISSING', bg: 'bg-amber-500/10', text: 'text-amber-300', border: 'border-amber-500/30' };
              } else if (engineId === 'qwen3-tts' && runtimes) {
                statusBadge = runtimes.neural_sidecar.running
                  ? { label: 'NEURAL SIDECAR LIVE (:9000)', bg: 'bg-emerald-500/10', text: 'text-emerald-400', border: 'border-emerald-500/30' }
                  : runtimes.python_packages.omnivoice
                  ? { label: 'OMNIVOICE READY (STOPPED)', bg: 'bg-amber-500/10', text: 'text-amber-300', border: 'border-amber-500/30' }
                  : { label: 'CROSS-LINGUAL MODE', bg: 'bg-purple-500/10', text: 'text-purple-300', border: 'border-purple-500/30' };
              }

              return (
                <div key={engineId} className="px-4 py-3.5 flex flex-col md:flex-row md:items-center justify-between gap-3 hover:bg-[#1A222D]/40 transition-colors">
                  <div className="flex items-start space-x-3 min-w-0">
                    {isReady ? (
                      <CheckCircle2 className="w-4 h-4 text-emerald-400 flex-shrink-0 mt-0.5" />
                    ) : (
                      <AlertTriangle className="w-4 h-4 text-amber-400 flex-shrink-0 mt-0.5" />
                    )}
                    <div>
                      <div className="flex items-center space-x-2">
                        <span className="text-xs font-mono font-bold text-white">{baseDesc.name}</span>
                        <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-[#0B0E14] text-[#94A3B8] border border-[#242E3D]">
                          {engineId}
                        </span>
                      </div>
                      <p className="text-[11px] text-[#94A3B8] mt-1 leading-relaxed">
                        {baseDesc.description}
                      </p>
                    </div>
                  </div>

                  <div className="flex items-center flex-wrap gap-2 flex-shrink-0 self-start md:self-auto">
                    {engineId === 'kokoro' && runtimes && !runtimes.kokoro_sidecar.running && (
                      runtimes.python_packages.kokoro ? (
                        <button
                          type="button"
                          disabled={busyAction !== null}
                          onClick={() => handleStartSidecar('kokoro', 8880)}
                          className="inline-flex items-center space-x-1 text-[10px] font-mono px-2 py-1 rounded bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border border-emerald-500/40"
                        >
                          {busyAction === 'start-kokoro' ? <Loader2 className="w-3 h-3 animate-spin" /> : <Play className="w-3 h-3" />}
                          <span>Start Kokoro</span>
                        </button>
                      ) : runtimes.python_available && (
                        <button
                          type="button"
                          disabled={busyAction !== null}
                          onClick={() => handleInstallRuntime('kokoro')}
                          className="inline-flex items-center space-x-1 text-[10px] font-mono px-2 py-1 rounded bg-amber-500/15 hover:bg-amber-500/25 text-amber-300 border border-amber-500/40"
                        >
                          {busyAction === 'install-kokoro' ? <Loader2 className="w-3 h-3 animate-spin" /> : <Download className="w-3 h-3" />}
                          <span>Install Kokoro</span>
                        </button>
                      )
                    )}
                    {engineId === 'piper-tts' && runtimes && !runtimes.piper_cli_available && runtimes.python_available && (
                      <button
                        type="button"
                        disabled={busyAction !== null}
                        onClick={() => handleInstallRuntime('piper')}
                        className="inline-flex items-center space-x-1 text-[10px] font-mono px-2 py-1 rounded bg-amber-500/15 hover:bg-amber-500/25 text-amber-300 border border-amber-500/40"
                      >
                        {busyAction === 'install-piper' ? <Loader2 className="w-3 h-3 animate-spin" /> : <Download className="w-3 h-3" />}
                        <span>Install Piper CLI</span>
                      </button>
                    )}
                    {engineId === 'qwen3-tts' && runtimes && !runtimes.neural_sidecar.running && runtimes.python_packages.omnivoice && (
                      <button
                        type="button"
                        disabled={busyAction !== null}
                        onClick={() => handleStartSidecar('omnivoice', 9000)}
                        className="inline-flex items-center space-x-1 text-[10px] font-mono px-2 py-1 rounded bg-purple-500/15 hover:bg-purple-500/25 text-purple-200 border border-purple-500/40"
                      >
                        {busyAction === 'start-omnivoice' ? <Loader2 className="w-3 h-3 animate-spin" /> : <Play className="w-3 h-3" />}
                        <span>Start OmniVoice</span>
                      </button>
                    )}
                    {baseDesc.action && onNavigateTab && (
                      <button
                        onClick={() => onNavigateTab(baseDesc.action!.tab)}
                        className="text-[10px] font-mono px-2 py-1 rounded bg-[#1A222D] hover:bg-[#242E3D] text-amber-400 border border-amber-500/30 hover:border-amber-400 transition-colors"
                      >
                        {baseDesc.action.label}
                      </button>
                    )}
                    <span
                      className={`text-[10px] font-mono font-bold px-2 py-0.5 rounded border ${statusBadge.bg} ${statusBadge.text} ${statusBadge.border}`}
                    >
                      {statusBadge.label}
                    </span>
                  </div>
                </div>
              );
            })}
          </div>
          {actionNotice && (
            <div className={`px-4 py-2.5 border-t border-[#242E3D] text-xs font-mono ${actionNotice.isError ? 'bg-red-950/30 text-red-300' : 'bg-emerald-950/30 text-emerald-300'}`}>
              {actionNotice.text}
            </div>
          )}
        </div>

        {/* Software Release & Enterprise Self-Update Card */}
        <div className="bg-[#121820] border border-[#242E3D] rounded-lg p-4 flex flex-col sm:flex-row sm:items-center justify-between gap-4">
          <div className="flex items-start space-x-3">
            <div className="p-2 rounded-lg bg-amber-500/10 border border-amber-500/30 text-amber-400 shrink-0">
              <ShieldCheck className="w-5 h-5" />
            </div>
            <div>
              <div className="flex items-center space-x-2">
                <span className="text-xs font-mono font-bold text-white uppercase">
                  Release Channel & Binary Integrity
                </span>
                <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-[#0B0E14] text-[#94A3B8] border border-[#242E3D]">
                  v{updateInfo?.current_version || __APP_VERSION__}
                </span>
                {updateInfo?.update_available && updateInfo.latest_version && (
                  <span className="text-[10px] font-mono font-bold px-2 py-0.5 rounded bg-amber-500/15 text-amber-300 border border-amber-500/40">
                    v{updateInfo.latest_version} Available
                  </span>
                )}
              </div>
              <p className="text-[11px] text-[#94A3B8] font-mono mt-1">
                {updateInfo?.update_hint ||
                  'Checks GitHub Releases with SHA-256 digest verification and atomic executable swap.'}
              </p>
            </div>
          </div>

          {onOpenUpdates && (
            <button
              type="button"
              onClick={onOpenUpdates}
              className="inline-flex items-center space-x-1.5 px-3.5 py-2 rounded bg-[#1A222D] hover:bg-[#242E3D] border border-[#3B485C] text-xs font-mono text-amber-400 hover:text-amber-300 transition-colors shrink-0 self-start sm:self-auto"
            >
              <ArrowUpCircle className="w-4 h-4" />
              <span>{updateInfo?.update_available ? 'Inspect & Install Update' : 'Check Releases'}</span>
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
