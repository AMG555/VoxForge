import React, { useEffect, useRef, useState } from 'react';
import {
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Cpu,
  Download,
  FolderOpen,
  Globe,
  HardDrive,
  Loader2,
  Search,
  Sparkles,
  Terminal,
  X,
  XCircle,
  Zap,
} from 'lucide-react';
import { setApiBaseUrl } from '../../services/api';
import {
  isTauriRuntime,
  pickPythonExecutableDialog,
  pickRuntimeDirectoryDialog,
} from '../../services/desktopBridge';
import { useAppDispatch, useAppSelector } from '../../store';
import {
  useCancelSetupInstallMutation,
  useGetInstallProgressQuery,
  useGetSetupStatusQuery,
  useProbePythonMutation,
  useSaveSetupConfigMutation,
  useStartSetupInstallMutation,
  useTestRemoteServerMutation,
} from '../../store/setupApi';
import {
  setCustomPythonDraft,
  setDetailsExpanded,
  setOptionsExpanded,
  setRemoteExpanded,
  setRemoteUrlDraft,
  setRuntimeDirDraft,
  setSelectedProfile,
  setTelemetryOptIn,
  setWizardOpen,
} from '../../store/setupSlice';

function formatGb(bytes: number): string {
  if (!bytes || bytes <= 0) return 'Unknown';
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GiB`;
}

export const FirstRunWizard: React.FC = () => {
  const dispatch = useAppDispatch();
  const {
    isDesktop,
    isWizardOpen,
    selectedProfile,
    runtimeDirDraft,
    customPythonDraft,
    optionsExpanded,
    detailsExpanded,
    remoteExpanded,
    remoteUrlDraft,
    telemetryOptIn,
  } = useAppSelector((s) => s.setup);

  const { data: status, refetch: refetchStatus } = useGetSetupStatusQuery();
  const [saveConfig, { isLoading: isSavingConfig }] = useSaveSetupConfigMutation();
  const [startInstall, { isLoading: isStartingInstall }] = useStartSetupInstallMutation();
  const [cancelInstall, { isLoading: isCancelling }] = useCancelSetupInstallMutation();
  const [probePython, { isLoading: isProbingPython }] = useProbePythonMutation();
  const [testRemote, { isLoading: isTestingRemote }] = useTestRemoteServerMutation();

  const [isEditingPath, setIsEditingPath] = useState(false);
  const [pythonProbeMsg, setPythonProbeMsg] = useState<string | null>(null);
  const [remoteTestResult, setRemoteTestResult] = useState<{
    reachable: boolean;
    message: string;
  } | null>(null);
  const [installError, setInstallError] = useState<string | null>(null);
  const [hasTrackedJob, setHasTrackedJob] = useState(false);
  const autoOpenedOnceRef = useRef(false);

  const { data: progress } = useGetInstallProgressQuery(undefined, {
    skip: !isWizardOpen,
    pollingInterval: hasTrackedJob ? 800 : 2500,
  });

  useEffect(() => {
    if (!status) return;
    if (!runtimeDirDraft && status.config.runtime_dir) {
      dispatch(setRuntimeDirDraft(status.config.runtime_dir));
    }
    if (!customPythonDraft && status.config.custom_python) {
      dispatch(setCustomPythonDraft(status.config.custom_python));
    }
    if (!autoOpenedOnceRef.current) {
      autoOpenedOnceRef.current = true;
      if (!status.config.setup_completed && isDesktop) {
        dispatch(setWizardOpen(true));
      }
    }
  }, [status, runtimeDirDraft, customPythonDraft, isDesktop, dispatch]);

  useEffect(() => {
    if (progress?.status === 'running') {
      setHasTrackedJob(true);
    } else if (
      hasTrackedJob &&
      (progress?.status === 'completed' ||
        progress?.status === 'failed' ||
        progress?.status === 'cancelled')
    ) {
      refetchStatus();
    }
  }, [progress?.status, hasTrackedJob, refetchStatus]);

  if (!isWizardOpen) return null;

  const isRunning = progress?.status === 'running';
  const showProgressScreen =
    hasTrackedJob || isRunning || progress?.status === 'completed';

  const gpuDesc =
    status?.hardware.gpus?.[0]?.name ||
    (status?.hardware.cpu_brand
      ? `${status.hardware.cpu_brand} (${status.hardware.cpu_logical_threads} threads)`
      : 'Native Rust Hardware Engine');

  const handleChooseRuntimeFolder = async () => {
    if (isTauriRuntime()) {
      const picked = await pickRuntimeDirectoryDialog(runtimeDirDraft || undefined);
      if (picked) {
        dispatch(setRuntimeDirDraft(picked));
      }
      return;
    }
    setIsEditingPath((v) => !v);
  };

  const handleChoosePythonFile = async () => {
    const picked = await pickPythonExecutableDialog();
    if (picked) {
      dispatch(setCustomPythonDraft(picked));
      const res = await probePython({ path: picked }).unwrap().catch(() => null);
      if (res) {
        setPythonProbeMsg(
          res.valid
            ? `Verified ${res.version} at ${res.path}`
            : res.error || 'Invalid interpreter selected.'
        );
      }
    }
  };

  const handleSelectQuickPython = async (path: string) => {
    dispatch(setCustomPythonDraft(path));
    const res = await probePython({ path }).unwrap().catch(() => null);
    if (res) {
      setPythonProbeMsg(
        res.valid
          ? `Selected ${res.version} (${path})`
          : res.error || 'Interpreter probe failed.'
      );
    }
  };

  const handleLaunchRustCore = async () => {
    setInstallError(null);
    try {
      await saveConfig({
        setup_completed: true,
        runtime_profile: 'rust_core',
        runtime_dir: runtimeDirDraft || status?.default_runtime_dir,
        telemetry_opt_in: telemetryOptIn,
      }).unwrap();
      dispatch(setWizardOpen(false));
    } catch (err: any) {
      setInstallError(err?.data?.detail || err?.message || 'Failed saving configuration');
    }
  };

  const handleStartInstall = async () => {
    if (selectedProfile === 'rust_core') {
      await handleLaunchRustCore();
      return;
    }
    setInstallError(null);
    setHasTrackedJob(true);
    try {
      await startInstall({
        profile: selectedProfile,
        runtime_dir: runtimeDirDraft || status?.default_runtime_dir || '',
        custom_python: customPythonDraft || undefined,
        telemetry_opt_in: telemetryOptIn,
      }).unwrap();
    } catch (err: any) {
      setInstallError(err?.data?.detail || err?.message || 'Failed starting runtime installer');
    }
  };

  const handleTestRemote = async () => {
    if (!remoteUrlDraft.trim()) return;
    setRemoteTestResult(null);
    try {
      const res = await testRemote({ url: remoteUrlDraft.trim() }).unwrap();
      setRemoteTestResult({
        reachable: res.reachable,
        message: res.reachable
          ? `Connected to VoxForge v${res.version || '0.7.0'} at ${res.url}`
          : res.error || 'Remote host unreachable',
      });
    } catch (err: any) {
      setRemoteTestResult({
        reachable: false,
        message: err?.data?.detail || err?.message || 'Invalid remote URL',
      });
    }
  };

  const handleUseRemoteServer = async () => {
    const clean = remoteUrlDraft.trim().replace(/\/+$/, '');
    if (!clean) return;
    localStorage.setItem('voxforge_remote_url', clean);
    setApiBaseUrl(clean);
    await saveConfig({
      setup_completed: true,
      runtime_profile: 'remote',
      remote_server_url: clean,
    }).catch(() => {});
    dispatch(setWizardOpen(false));
    window.location.reload();
  };

  const handleFinishProgress = () => {
    setHasTrackedJob(false);
    dispatch(setWizardOpen(false));
  };

  return (
    <div
      className="fixed inset-0 z-50 bg-[#080B10]/95 backdrop-blur-md flex items-center justify-center p-4 overflow-y-auto"
      role="dialog"
      aria-modal="true"
      aria-labelledby="voxforge-setup-title"
    >
      <div className="w-full max-w-2xl bg-[#0D1219] border border-[#242E3D] rounded-xl shadow-2xl overflow-hidden my-auto">
        <div className="px-6 py-4 border-b border-[#1E2633] flex items-center justify-between bg-[#0B0E14]">
          <div className="flex items-center space-x-3">
            <div className="w-8 h-8 rounded-lg bg-amber-500/15 border border-amber-500/30 flex items-center justify-center text-amber-400">
              {showProgressScreen ? (
                <Loader2 className={`w-4 h-4 ${isRunning ? 'animate-spin' : ''}`} />
              ) : (
                <Download className="w-4 h-4" />
              )}
            </div>
            <div>
              <h2 id="voxforge-setup-title" className="text-base font-bold text-white">
                {showProgressScreen
                  ? isRunning
                    ? 'Setting up local neural runtime...'
                    : progress?.status === 'completed'
                      ? 'Local neural runtime ready'
                      : progress?.status === 'cancelled'
                        ? 'Runtime installation cancelled'
                        : 'Runtime installation interrupted'
                  : 'Set Up VoxForge Runtime'}
              </h2>
              <p className="text-xs text-[#94A3B8]">
                {showProgressScreen
                  ? 'First-run isolated Python 3.11 + AI library setup via Rust-speed uv.'
                  : 'Rust speech core & Cloud Relay work immediately. Pick optional local AI engines below.'}
              </p>
            </div>
          </div>

          {!isRunning && (
            <button
              type="button"
              onClick={() => {
                setHasTrackedJob(false);
                dispatch(setWizardOpen(false));
              }}
              className="p-1.5 rounded text-[#64748B] hover:text-white hover:bg-[#1A222D] transition-colors"
              aria-label="Close setup wizard"
            >
              <X className="w-4 h-4" />
            </button>
          )}
        </div>

        <div className="p-6 space-y-5 max-h-[80vh] overflow-y-auto">
          {!showProgressScreen ? (
            <>
              {/* 3 Runtime Tier Choices */}
              <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                {[
                  {
                    id: 'rust_core' as const,
                    title: 'Instant Rust Core',
                    badge: '0 GB · READY NOW',
                    desc: 'Edge-TTS (130+ langs), broadcast mastering & acoustic voice cloning with zero Python downloads.',
                    icon: Zap,
                    accent: 'text-emerald-400',
                  },
                  {
                    id: 'core_neural' as const,
                    title: 'Core Neural Studio',
                    badge: '~1.4 GB · RECOMMENDED',
                    desc: 'Isolated uv Python 3.11 + Kokoro-82M + Piper ONNX + Whisper ASR for offline synthesis.',
                    icon: Sparkles,
                    accent: 'text-amber-400',
                  },
                  {
                    id: 'full_cloning' as const,
                    title: 'Full Cloning Studio',
                    badge: '~4.5 GB · COMPLETE',
                    desc: 'Core Neural plus PyTorch & OmniVoice 646-language local neural cloning.',
                    icon: Cpu,
                    accent: 'text-sky-400',
                  },
                ].map((opt) => {
                  const Icon = opt.icon;
                  const active = selectedProfile === opt.id;
                  return (
                    <button
                      key={opt.id}
                      type="button"
                      onClick={() => dispatch(setSelectedProfile(opt.id))}
                      className={`text-left p-3.5 rounded-lg border transition-all flex flex-col justify-between space-y-2 ${
                        active
                          ? 'bg-[#151D29] border-amber-400 ring-1 ring-amber-400/40'
                          : 'bg-[#111722] border-[#242E3D] hover:border-[#3B485C]'
                      }`}
                    >
                      <div className="space-y-1.5">
                        <div className="flex items-center justify-between">
                          <Icon className={`w-4 h-4 ${opt.accent}`} />
                          <span className="text-[9px] font-mono uppercase px-1.5 py-0.5 rounded bg-[#0B0E14] text-[#CBD5E1] border border-[#242E3D]">
                            {opt.badge}
                          </span>
                        </div>
                        <div className="text-xs font-semibold text-white">{opt.title}</div>
                        <p className="text-[11px] text-[#94A3B8] leading-relaxed">{opt.desc}</p>
                      </div>
                    </button>
                  );
                })}
              </div>

              {/* App Environment & Hardware Summary Card */}
              <div className="bg-[#111722] border border-[#242E3D] rounded-lg p-4 space-y-2.5">
                <div className="flex items-start justify-between gap-3">
                  <div className="space-y-1 min-w-0 flex-1">
                    <div className="flex items-center space-x-2">
                      <FolderOpen className="w-3.5 h-3.5 text-amber-400 shrink-0" />
                      <span className="text-xs font-semibold text-white">App environment</span>
                      <span className="text-[11px] text-[#64748B]">
                        Isolated Python runtime & AI libraries
                      </span>
                    </div>
                    {isEditingPath ? (
                      <input
                        type="text"
                        value={runtimeDirDraft}
                        onChange={(e) => dispatch(setRuntimeDirDraft(e.target.value))}
                        className="w-full bg-[#0B0E14] border border-[#3B485C] rounded px-2.5 py-1.5 text-xs font-mono text-white"
                      />
                    ) : (
                      <div className="text-xs font-mono text-[#CBD5E1] break-all">
                        {runtimeDirDraft || status?.default_runtime_dir || 'Detecting...'}
                      </div>
                    )}
                    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-[#94A3B8] pt-0.5">
                      <span className="inline-flex items-center space-x-1">
                        <HardDrive className="w-3 h-3 text-emerald-400" />
                        <span>
                          {status ? `${formatGb(status.disk_available_bytes)} free on drive` : 'Checking disk...'}
                        </span>
                      </span>
                      <span className="inline-flex items-center space-x-1">
                        <Cpu className="w-3 h-3 text-sky-400" />
                        <span>{gpuDesc}</span>
                      </span>
                    </div>
                  </div>
                  <button
                    type="button"
                    onClick={handleChooseRuntimeFolder}
                    className="px-3 py-1.5 rounded bg-[#1A222D] hover:bg-[#242E3D] border border-[#3B485C] text-xs font-medium text-white shrink-0"
                  >
                    Change...
                  </button>
                </div>
              </div>

              {/* Primary CTA + Options button row */}
              <div className="flex items-center justify-between gap-3">
                <button
                  type="button"
                  onClick={handleStartInstall}
                  disabled={isSavingConfig || isStartingInstall}
                  className="px-5 py-2.5 rounded-lg bg-amber-500 hover:bg-amber-400 text-[#0B0E14] font-semibold text-xs inline-flex items-center space-x-2 transition-colors disabled:opacity-50"
                >
                  {(isSavingConfig || isStartingInstall) && (
                    <Loader2 className="w-3.5 h-3.5 animate-spin" />
                  )}
                  <span>
                    {selectedProfile === 'rust_core'
                      ? 'Continue with Instant Rust Core'
                      : 'Install local runtime'}
                  </span>
                </button>

                <div className="flex items-center space-x-2">
                  {selectedProfile !== 'rust_core' && (
                    <button
                      type="button"
                      onClick={handleLaunchRustCore}
                      className="px-3 py-2 rounded-lg bg-[#1A222D] hover:bg-[#242E3D] border border-[#242E3D] text-xs text-[#CBD5E1] transition-colors"
                    >
                      Skip for now (Rust Core)
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => dispatch(setOptionsExpanded(!optionsExpanded))}
                    className="px-3 py-2 rounded-lg bg-[#1A222D] hover:bg-[#242E3D] border border-[#3B485C] text-xs font-medium text-white inline-flex items-center space-x-1.5"
                  >
                    <span>Options</span>
                    {optionsExpanded ? (
                      <ChevronUp className="w-3.5 h-3.5" />
                    ) : (
                      <ChevronDown className="w-3.5 h-3.5" />
                    )}
                  </button>
                </div>
              </div>

              {installError && (
                <div className="p-3 rounded-lg bg-red-500/10 border border-red-500/30 text-xs text-red-300">
                  {installError}
                </div>
              )}

              {/* Collapsible Python Interpreter & Options drawer */}
              {optionsExpanded && (
                <div className="bg-[#111722] border border-[#242E3D] rounded-lg p-4 space-y-3">
                  <div className="flex items-center justify-between">
                    <div>
                      <div className="text-xs font-semibold text-white">Python Interpreter</div>
                      <p className="text-[11px] text-[#94A3B8]">
                        Leave blank to use VoxForge managed Python 3.11 via uv, or reuse an existing interpreter.
                      </p>
                    </div>
                  </div>

                  <div className="flex flex-wrap gap-2">
                    <button
                      type="button"
                      onClick={() => refetchStatus()}
                      className="px-2.5 py-1.5 rounded bg-[#1A222D] hover:bg-[#242E3D] border border-[#3B485C] text-xs text-white inline-flex items-center space-x-1.5"
                    >
                      <Search className="w-3.5 h-3.5 text-amber-400" />
                      <span>Quick search</span>
                    </button>
                    {isTauriRuntime() && (
                      <button
                        type="button"
                        onClick={handleChoosePythonFile}
                        disabled={isProbingPython}
                        className="px-2.5 py-1.5 rounded bg-[#1A222D] hover:bg-[#242E3D] border border-[#3B485C] text-xs text-white inline-flex items-center space-x-1.5"
                      >
                        <FolderOpen className="w-3.5 h-3.5 text-sky-400" />
                        <span>Choose interpreter</span>
                      </button>
                    )}
                  </div>

                  {status?.discovered_pythons && status.discovered_pythons.length > 0 && (
                    <div className="space-y-1.5">
                      {status.discovered_pythons.map((py) => (
                        <button
                          key={py.path}
                          type="button"
                          onClick={() => handleSelectQuickPython(py.path)}
                          className={`w-full text-left px-3 py-2 rounded border text-xs font-mono flex items-center justify-between ${
                            customPythonDraft === py.path
                              ? 'bg-amber-500/10 border-amber-500/40 text-white'
                              : 'bg-[#0B0E14] border-[#242E3D] text-[#94A3B8] hover:text-white'
                          }`}
                        >
                          <span className="truncate">{py.path}</span>
                          <span className="text-[10px] text-emerald-400 shrink-0 ml-2">
                            {py.version}
                          </span>
                        </button>
                      ))}
                    </div>
                  )}

                  {pythonProbeMsg && (
                    <div className="text-[11px] font-mono text-emerald-300 bg-emerald-500/10 border border-emerald-500/30 rounded px-2.5 py-1.5">
                      {pythonProbeMsg}
                    </div>
                  )}

                  <label className="flex items-center space-x-2 text-xs text-[#94A3B8] pt-1 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={telemetryOptIn}
                      onChange={(e) => dispatch(setTelemetryOptIn(e.target.checked))}
                      className="rounded border-[#3B485C] bg-[#0B0E14]"
                    />
                    <span>Save local hardware & setup failure diagnostics for troubleshooting</span>
                  </label>
                </div>
              )}

              {/* Remote server fallback toggle */}
              <div className="pt-1 border-t border-[#1E2633]">
                <button
                  type="button"
                  onClick={() => dispatch(setRemoteExpanded(!remoteExpanded))}
                  className="text-xs text-[#94A3B8] hover:text-white inline-flex items-center space-x-1.5"
                >
                  <Globe className="w-3.5 h-3.5 text-sky-400" />
                  <span>Use VoxForge on another computer instead</span>
                  {remoteExpanded ? (
                    <ChevronUp className="w-3.5 h-3.5" />
                  ) : (
                    <ChevronDown className="w-3.5 h-3.5" />
                  )}
                </button>

                {remoteExpanded && (
                  <div className="mt-3 p-3.5 rounded-lg bg-[#111722] border border-[#242E3D] space-y-2.5">
                    <div className="flex items-center space-x-2">
                      <input
                        type="url"
                        value={remoteUrlDraft}
                        onChange={(e) => dispatch(setRemoteUrlDraft(e.target.value))}
                        placeholder="http://192.168.1.50:8080"
                        className="flex-1 bg-[#0B0E14] border border-[#3B485C] rounded px-3 py-1.5 text-xs font-mono text-white"
                      />
                      <button
                        type="button"
                        onClick={handleTestRemote}
                        disabled={isTestingRemote || !remoteUrlDraft.trim()}
                        className="px-3 py-1.5 rounded bg-[#1A222D] hover:bg-[#242E3D] border border-[#3B485C] text-xs text-white"
                      >
                        {isTestingRemote ? 'Testing...' : 'Test'}
                      </button>
                      <button
                        type="button"
                        onClick={handleUseRemoteServer}
                        disabled={!remoteUrlDraft.trim()}
                        className="px-3 py-1.5 rounded bg-sky-600 hover:bg-sky-500 text-xs font-semibold text-white"
                      >
                        Connect
                      </button>
                    </div>
                    {remoteTestResult && (
                      <p
                        className={`text-[11px] font-mono ${
                          remoteTestResult.reachable ? 'text-emerald-400' : 'text-red-400'
                        }`}
                      >
                        {remoteTestResult.message}
                      </p>
                    )}
                  </div>
                )}
              </div>
            </>
          ) : (
            /* Live Installer Progress View (VoiceStudio-inspired steps + details + Cancel) */
            <div className="space-y-4">
              <div className="space-y-1.5">
                <div className="flex items-center justify-between text-xs font-mono">
                  <span className="text-[#CBD5E1]">{progress?.percent ?? 0}%</span>
                  <span className="text-[#64748B]">{progress?.elapsed_secs ?? 0}s elapsed</span>
                </div>
                <div className="w-full h-2 rounded-full bg-[#1A222D] overflow-hidden">
                  <div
                    className={`h-full transition-all duration-300 ${
                      progress?.status === 'failed'
                        ? 'bg-red-500'
                        : progress?.status === 'cancelled'
                          ? 'bg-amber-500'
                          : 'bg-sky-500'
                    }`}
                    style={{ width: `${Math.max(4, progress?.percent ?? 5)}%` }}
                  />
                </div>
              </div>

              <div className="bg-[#111722] border border-[#242E3D] rounded-lg divide-y divide-[#1E2633]">
                {(progress?.steps || []).map((step) => (
                  <div key={step.id} className="px-4 py-2.5 flex items-center justify-between">
                    <div className="flex items-center space-x-3">
                      {step.status === 'done' && (
                        <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                      )}
                      {step.status === 'running' && (
                        <Loader2 className="w-4 h-4 text-sky-400 animate-spin shrink-0" />
                      )}
                      {step.status === 'failed' && (
                        <XCircle className="w-4 h-4 text-red-400 shrink-0" />
                      )}
                      {step.status === 'cancelled' && (
                        <XCircle className="w-4 h-4 text-amber-400 shrink-0" />
                      )}
                      {step.status === 'pending' && (
                        <span className="w-4 h-4 rounded-full border border-[#3B485C] inline-block shrink-0" />
                      )}
                      <div>
                        <div className="text-xs font-medium text-white">{step.label}</div>
                        {step.detail && (
                          <div className="text-[10px] font-mono text-[#64748B] truncate max-w-md">
                            {step.detail}
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                ))}
              </div>

              <div className="flex items-center justify-between">
                <button
                  type="button"
                  onClick={() => dispatch(setDetailsExpanded(!detailsExpanded))}
                  className="text-xs font-mono text-[#94A3B8] hover:text-white inline-flex items-center space-x-1.5"
                >
                  <Terminal className="w-3.5 h-3.5" />
                  <span>Details</span>
                  {detailsExpanded ? (
                    <ChevronUp className="w-3.5 h-3.5" />
                  ) : (
                    <ChevronDown className="w-3.5 h-3.5" />
                  )}
                </button>

                <div className="flex items-center space-x-2">
                  {isRunning ? (
                    <button
                      type="button"
                      onClick={() => cancelInstall()}
                      disabled={isCancelling}
                      className="px-3.5 py-1.5 rounded bg-red-600/20 hover:bg-red-600/30 border border-red-500/40 text-xs font-semibold text-red-300"
                    >
                      {isCancelling ? 'Cancelling...' : 'Cancel'}
                    </button>
                  ) : (
                    <>
                      <button
                        type="button"
                        onClick={() => setHasTrackedJob(false)}
                        className="px-3.5 py-1.5 rounded bg-[#1A222D] hover:bg-[#242E3D] border border-[#3B485C] text-xs text-white"
                      >
                        Back
                      </button>
                      <button
                        type="button"
                        onClick={handleFinishProgress}
                        className="px-4 py-1.5 rounded bg-amber-500 hover:bg-amber-400 text-[#0B0E14] text-xs font-semibold"
                      >
                        Open VoxForge Studio
                      </button>
                    </>
                  )}
                </div>
              </div>

              {detailsExpanded && (
                <pre className="p-3 rounded-lg bg-[#080B10] border border-[#1E2633] text-[11px] font-mono text-[#94A3B8] max-h-48 overflow-y-auto whitespace-pre-wrap">
                  {(progress?.logs || []).join('\n') || 'Waiting for installer output...'}
                </pre>
              )}

              {progress?.error && (
                <div className="p-3 rounded-lg bg-red-500/10 border border-red-500/30 text-xs text-red-300">
                  {progress.error}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
