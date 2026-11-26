import React, { useState, useEffect } from 'react';
import {
  Download,
  CheckCircle2,
  Cpu,
  HardDrive,
  Filter,
  AlertCircle,
  RefreshCw,
  Layers,
  Trash2,
  Play,
  Loader2,
  Terminal,
} from 'lucide-react';
import { CatalogItem, RuntimeStatusResponse } from '../../types';
import { api } from '../../services/api';

export const ModelCatalog: React.FC = () => {
  const [models, setModels] = useState<CatalogItem[]>([]);
  const [runtimes, setRuntimes] = useState<RuntimeStatusResponse | null>(null);
  const [filterType, setFilterType] = useState<string>('all');
  const [installedOnly, setInstalledOnly] = useState<boolean>(false);
  const [loading, setLoading] = useState<boolean>(true);
  const [installingId, setInstallingId] = useState<string | null>(null);
  const [runtimeBusy, setRuntimeBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<{ text: string; type: 'success' | 'error' } | null>(null);

  const fetchRuntimes = async () => {
    try {
      const data = await api.getRuntimeStatus();
      setRuntimes(data);
    } catch {
      // Non-fatal in offline dev
    }
  };

  const fetchModels = async () => {
    setLoading(true);
    try {
      const type = filterType === 'all' ? undefined : filterType;
      const [data] = await Promise.all([api.getCatalogModels(type, installedOnly), fetchRuntimes()]);
      setModels(data);
    } catch {
      // Offline fallback curated catalog
      setModels([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchModels();
  }, [filterType, installedOnly]);

  const handleInstallRuntime = async (pkg: 'kokoro' | 'piper' | 'whisper') => {
    setRuntimeBusy(`install-${pkg}`);
    setMessage(null);
    try {
      const res = await api.installRuntimePackage(pkg);
      setMessage({ text: res.message, type: 'success' });
      await fetchRuntimes();
    } catch (err: any) {
      setMessage({ text: err?.message || `Failed to install '${pkg}' runtime`, type: 'error' });
    } finally {
      setRuntimeBusy(null);
    }
  };

  const handleStartSidecar = async (backend: 'auto' | 'omnivoice' | 'kokoro', port: number) => {
    setRuntimeBusy(`start-${backend}`);
    setMessage(null);
    try {
      const res = await api.startSidecar(backend, port);
      setMessage({ text: res.message, type: 'success' });
      setTimeout(fetchRuntimes, 1200);
    } catch (err: any) {
      setMessage({ text: err?.message || `Failed to start ${backend} sidecar`, type: 'error' });
    } finally {
      setRuntimeBusy(null);
    }
  };

  const handleInstall = async (id: string) => {
    setInstallingId(id);
    setMessage(null);
    try {
      const res = await api.installModel(id);
      setModels((prev) =>
        prev.map((m) =>
          m.id === id
            ? {
                ...m,
                status: 'installed',
                local_path: res.local_path || `models/${id}.onnx`,
              }
            : m
        )
      );
      setMessage({ text: `Model '${id}' installed successfully and ready!`, type: 'success' });
      await fetchModels();
    } catch (err: any) {
      setMessage({ text: err.message || `Failed to install '${id}'`, type: 'error' });
    } finally {
      setInstallingId(null);
    }
  };

  const handleUninstall = async (id: string) => {
    if (!confirm(`Remove local neural weights for '${id}'?`)) return;
    try {
      await api.uninstallModel(id);
      setModels((prev) =>
        prev.map((m) => (m.id === id ? { ...m, status: 'available', local_path: undefined } : m))
      );
      setMessage({ text: `Model '${id}' uninstalled successfully.`, type: 'success' });
      await fetchModels();
    } catch (err: any) {
      setMessage({ text: err.message || `Failed to uninstall '${id}'`, type: 'error' });
    }
  };

  return (
    <div className="flex-1 flex flex-col h-full bg-[#0B0E14] overflow-y-auto">
      <div className="max-w-7xl w-full mx-auto p-6 pb-36 space-y-6">
        {/* Header */}
        <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 border-b border-[#242E3D] pb-6">
          <div>
            <h1 className="text-xl font-bold tracking-tight text-white flex items-center space-x-2">
              <Layers className="w-5 h-5 text-amber-500" />
              <span>Model Catalogue & Neural Weights</span>
            </h1>
            <p className="text-xs text-[#94A3B8] mt-1">
              Browse open-weights neural speech engines, verify host hardware requirements, and manage local models.
            </p>
          </div>

          <div className="flex items-center space-x-3">
            <button
              onClick={fetchModels}
              disabled={loading}
              className="flex items-center space-x-1.5 px-3 py-1.5 rounded text-xs font-mono bg-[#1A222D] text-[#94A3B8] hover:text-white border border-[#242E3D] transition-colors"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
              <span>Refresh</span>
            </button>
          </div>
        </div>

        {/* Status Message */}
        {message && (
          <div
            className={`p-3 rounded text-xs flex items-center space-x-2 ${
              message.type === 'success'
                ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                : 'bg-red-500/10 text-red-400 border border-red-500/20'
            }`}
          >
            {message.type === 'success' ? (
              <CheckCircle2 className="w-4 h-4 flex-shrink-0" />
            ) : (
              <AlertCircle className="w-4 h-4 flex-shrink-0" />
            )}
            <span>{message.text}</span>
          </div>
        )}

        {/* Host Runtimes & Inference Sidecars */}
        {runtimes && (
          <div className="bg-[#121820] border border-[#242E3D] rounded-lg p-4 space-y-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex items-center space-x-2 text-xs font-mono font-bold text-white uppercase">
                <Terminal className="w-4 h-4 text-amber-400" />
                <span>Local Neural Runtimes & Sidecars ({runtimes.python_executable || 'Python not detected'})</span>
              </div>
              <span className="text-[11px] font-mono text-[#94A3B8]">
                {runtimes.installed_model_files.length} local weight file(s) in models/
              </span>
            </div>

            <div className="grid grid-cols-1 md:grid-cols-3 gap-3 text-xs">
              <div className="p-3 rounded-lg bg-[#0B0E14] border border-[#242E3D] flex flex-col justify-between space-y-2.5">
                <div>
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-white">OmniVoice + Whisper Sidecar</span>
                    <span className={`text-[10px] font-mono px-1.5 py-0.5 rounded border ${
                      runtimes.neural_sidecar.running
                        ? 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30'
                        : runtimes.python_packages.omnivoice
                        ? 'bg-amber-500/10 text-amber-300 border-amber-500/30'
                        : 'bg-slate-800 text-slate-400 border-slate-700'
                    }`}>
                      {runtimes.neural_sidecar.running ? 'RUNNING :9000' : runtimes.python_packages.omnivoice ? 'INSTALLED' : 'MISSING'}
                    </span>
                  </div>
                  <p className="text-[11px] text-[#94A3B8] mt-1">
                    Enables 646-language cloning, refinement steps, &amp; Auto-Transcribe Whisper ASR.
                  </p>
                </div>
                <div className="flex items-center space-x-2">
                  {!runtimes.neural_sidecar.running && runtimes.python_packages.omnivoice && (
                    <button
                      type="button"
                      disabled={runtimeBusy !== null}
                      onClick={() => handleStartSidecar('omnivoice', 9000)}
                      className="flex-1 inline-flex items-center justify-center space-x-1.5 px-2.5 py-1.5 rounded bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border border-emerald-500/40 font-mono text-[11px]"
                    >
                      {runtimeBusy === 'start-omnivoice' ? <Loader2 className="w-3 h-3 animate-spin" /> : <Play className="w-3 h-3" />}
                      <span>Start Sidecar (:9000)</span>
                    </button>
                  )}
                  {!runtimes.python_packages.whisper && runtimes.python_available && (
                    <button
                      type="button"
                      disabled={runtimeBusy !== null}
                      onClick={() => handleInstallRuntime('whisper')}
                      className="inline-flex items-center space-x-1 px-2.5 py-1.5 rounded bg-amber-500/15 hover:bg-amber-500/25 text-amber-300 border border-amber-500/40 font-mono text-[11px]"
                    >
                      {runtimeBusy === 'install-whisper' ? <Loader2 className="w-3 h-3 animate-spin" /> : <Download className="w-3 h-3" />}
                      <span>Install Whisper</span>
                    </button>
                  )}
                </div>
              </div>

              <div className="p-3 rounded-lg bg-[#0B0E14] border border-[#242E3D] flex flex-col justify-between space-y-2.5">
                <div>
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-white">Kokoro-82M Sidecar</span>
                    <span className={`text-[10px] font-mono px-1.5 py-0.5 rounded border ${
                      runtimes.kokoro_sidecar.running
                        ? 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30'
                        : runtimes.python_packages.kokoro
                        ? 'bg-amber-500/10 text-amber-300 border-amber-500/30'
                        : 'bg-red-500/10 text-red-300 border-red-500/30'
                    }`}>
                      {runtimes.kokoro_sidecar.running ? 'RUNNING :8880' : runtimes.python_packages.kokoro ? 'INSTALLED' : 'NOT INSTALLED'}
                    </span>
                  </div>
                  <p className="text-[11px] text-[#94A3B8] mt-1">
                    Required for Kokoro voices (Heart, Bella, Sarah, Adam).
                  </p>
                </div>
                <div className="flex items-center space-x-2">
                  {!runtimes.kokoro_sidecar.running && (
                    runtimes.python_packages.kokoro ? (
                      <button
                        type="button"
                        disabled={runtimeBusy !== null}
                        onClick={() => handleStartSidecar('kokoro', 8880)}
                        className="flex-1 inline-flex items-center justify-center space-x-1.5 px-2.5 py-1.5 rounded bg-emerald-500/15 hover:bg-emerald-500/25 text-emerald-300 border border-emerald-500/40 font-mono text-[11px]"
                      >
                        {runtimeBusy === 'start-kokoro' ? <Loader2 className="w-3 h-3 animate-spin" /> : <Play className="w-3 h-3" />}
                        <span>Start Kokoro (:8880)</span>
                      </button>
                    ) : runtimes.python_available && (
                      <button
                        type="button"
                        disabled={runtimeBusy !== null}
                        onClick={() => handleInstallRuntime('kokoro')}
                        className="flex-1 inline-flex items-center justify-center space-x-1.5 px-2.5 py-1.5 rounded bg-amber-500/15 hover:bg-amber-500/25 text-amber-300 border border-amber-500/40 font-mono text-[11px]"
                      >
                        {runtimeBusy === 'install-kokoro' ? <Loader2 className="w-3 h-3 animate-spin" /> : <Download className="w-3 h-3" />}
                        <span>Install Kokoro Package</span>
                      </button>
                    )
                  )}
                </div>
              </div>

              <div className="p-3 rounded-lg bg-[#0B0E14] border border-[#242E3D] flex flex-col justify-between space-y-2.5">
                <div>
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-white">Piper Local ONNX CLI</span>
                    <span className={`text-[10px] font-mono px-1.5 py-0.5 rounded border ${
                      runtimes.piper_cli_available
                        ? 'bg-emerald-500/10 text-emerald-300 border-emerald-500/30'
                        : 'bg-red-500/10 text-red-300 border-red-500/30'
                    }`}>
                      {runtimes.piper_cli_available ? 'CLI READY' : 'CLI MISSING'}
                    </span>
                  </div>
                  <p className="text-[11px] text-[#94A3B8] mt-1">
                    Required alongside downloaded `.onnx` voice weights for Piper voices.
                  </p>
                </div>
                {!runtimes.piper_cli_available && runtimes.python_available && (
                  <button
                    type="button"
                    disabled={runtimeBusy !== null}
                    onClick={() => handleInstallRuntime('piper')}
                    className="inline-flex items-center justify-center space-x-1.5 px-2.5 py-1.5 rounded bg-amber-500/15 hover:bg-amber-500/25 text-amber-300 border border-amber-500/40 font-mono text-[11px]"
                  >
                    {runtimeBusy === 'install-piper' ? <Loader2 className="w-3 h-3 animate-spin" /> : <Download className="w-3 h-3" />}
                    <span>Install Piper Runtime</span>
                  </button>
                )}
              </div>
            </div>
          </div>
        )}

        {/* Filter Bar */}
        <div className="flex flex-wrap items-center justify-between gap-4 bg-[#121820] p-3 rounded-lg border border-[#242E3D]">
          <div className="flex items-center space-x-2">
            <Filter className="w-3.5 h-3.5 text-amber-500" />
            <span className="text-xs font-mono text-[#94A3B8]">Domain:</span>
            {(['all', 'tts', 'asr', 'vad'] as const).map((type) => (
              <button
                key={type}
                onClick={() => setFilterType(type)}
                className={`px-2.5 py-1 rounded text-xs font-mono uppercase transition-colors ${
                  filterType === type
                    ? 'bg-amber-500/20 text-amber-400 border border-amber-500/40'
                    : 'text-[#94A3B8] hover:text-white hover:bg-[#1A222D]'
                }`}
              >
                {type}
              </button>
            ))}
          </div>

          <label className="flex items-center space-x-2 text-xs text-[#94A3B8] cursor-pointer select-none">
            <input
              type="checkbox"
              checked={installedOnly}
              onChange={(e) => setInstalledOnly(e.target.checked)}
              className="rounded bg-[#1A222D] border-[#242E3D] text-amber-500 focus:ring-0 focus:ring-offset-0"
            />
            <span>Show installed models only</span>
          </label>
        </div>

        {/* Models Grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {models.map((model) => {
            const isInstalled = model.status?.toLowerCase() === 'installed';
            const isInstalling = installingId === model.id;
            const sizeMb = Math.round(model.size_bytes / (1024 * 1024));

            return (
              <div
                key={model.id}
                className="bg-[#121820] rounded-lg border border-[#242E3D] p-5 flex flex-col justify-between hover:border-[#3B485C] transition-colors group"
              >
                <div>
                  <div className="flex items-start justify-between gap-2">
                    <div>
                      <span className="text-[10px] font-mono px-2 py-0.5 rounded uppercase font-bold bg-[#1A222D] text-amber-400 border border-[#242E3D]">
                        {model.model_type}
                      </span>
                      <span className="ml-2 text-[10px] font-mono uppercase text-[#64748B]">
                        {model.format}
                      </span>
                      <h3 className="font-bold text-sm text-white mt-2 group-hover:text-amber-400 transition-colors">
                        {model.name}
                      </h3>
                      <div className="text-[11px] font-mono text-[#64748B] mt-0.5">{model.id}</div>
                    </div>

                    {isInstalled ? (
                      <span className="flex items-center space-x-1 text-[11px] font-mono px-2 py-0.5 rounded bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
                        <CheckCircle2 className="w-3 h-3" />
                        <span>READY</span>
                      </span>
                    ) : (
                      <span className="text-[11px] font-mono px-2 py-0.5 rounded bg-[#1A222D] text-[#94A3B8] border border-[#242E3D]">
                        AVAILABLE
                      </span>
                    )}
                  </div>

                  <p className="text-xs text-[#94A3B8] mt-3 leading-relaxed">{model.description}</p>

                  <div className="mt-4 pt-3 border-t border-[#242E3D]/50 flex flex-wrap gap-3 text-xs text-[#64748B] font-mono">
                    <div className="flex items-center space-x-1">
                      <HardDrive className="w-3.5 h-3.5 text-amber-500" />
                      <span>{sizeMb} MB</span>
                    </div>
                    <div className="flex items-center space-x-1">
                      <Cpu className="w-3.5 h-3.5 text-amber-500" />
                      <span>Min {model.min_ram_mb} MB</span>
                    </div>
                  </div>

                  <div className="mt-3 flex flex-wrap gap-1">
                    {model.supported_languages.map((lang) => (
                      <span
                        key={lang}
                        className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-[#1A222D] text-[#94A3B8]"
                      >
                        {lang}
                      </span>
                    ))}
                  </div>
                </div>

                <div className="mt-5 pt-4 border-t border-[#242E3D]">
                  {isInstalled ? (
                    <div className="flex items-center justify-between">
                      <div className="text-[11px] font-mono text-emerald-400/90 flex items-center space-x-1.5 min-w-0">
                        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400 flex-shrink-0" />
                        <span className="truncate">{model.local_path || 'active weights'}</span>
                      </div>
                      <button
                        onClick={() => handleUninstall(model.id)}
                        className="p-1.5 rounded text-[#64748B] hover:text-rose-400 hover:bg-rose-500/10 transition-colors"
                        title="Uninstall local weights"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  ) : (
                    <button
                      onClick={() => handleInstall(model.id)}
                      disabled={isInstalling}
                      className="w-full flex items-center justify-center space-x-2 py-2 px-3 rounded bg-amber-500/10 hover:bg-amber-500/20 text-amber-400 border border-amber-500/30 font-mono text-xs font-semibold transition-colors disabled:opacity-50"
                    >
                      {isInstalling ? (
                        <>
                          <RefreshCw className="w-3.5 h-3.5 animate-spin" />
                          <span>Downloading weights...</span>
                        </>
                      ) : (
                        <>
                          <Download className="w-3.5 h-3.5" />
                          <span>Install Weights ({sizeMb} MB)</span>
                        </>
                      )}
                    </button>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
};
