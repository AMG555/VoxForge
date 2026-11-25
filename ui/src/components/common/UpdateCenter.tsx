import React, { useState } from 'react';
import {
  X,
  RefreshCw,
  Download,
  ShieldCheck,
  ExternalLink,
  CheckCircle2,
  AlertTriangle,
  Terminal,
  Copy,
  Check,
  RotateCcw,
} from 'lucide-react';
import { SystemUpdateApplyResult, SystemUpdateInfo } from '../../types';
import { api } from '../../services/api';

interface UpdateCenterProps {
  isOpen: boolean;
  onClose: () => void;
  updateInfo: SystemUpdateInfo | null;
  onRefresh: (force?: boolean) => Promise<void>;
  isChecking: boolean;
}

export const UpdateCenter: React.FC<UpdateCenterProps> = ({
  isOpen,
  onClose,
  updateInfo,
  onRefresh,
  isChecking,
}) => {
  const [isApplying, setIsApplying] = useState(false);
  const [applyResult, setApplyResult] = useState<SystemUpdateApplyResult | null>(null);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [copiedCmd, setCopiedCmd] = useState(false);

  if (!isOpen) return null;

  const isWindows = updateInfo?.platform === 'windows';
  const cliCommand = isWindows
    ? 'powershell -c "irm https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/install.ps1 | iex"'
    : 'curl -fsSL https://raw.githubusercontent.com/AMG555/VoxForge/main/scripts/install.sh | sh';

  const handleCopyCommand = () => {
    navigator.clipboard.writeText(cliCommand).catch(() => {});
    setCopiedCmd(true);
    setTimeout(() => setCopiedCmd(false), 1800);
  };

  const handleApplyUpdate = async () => {
    setErrorMsg(null);
    setIsApplying(true);
    try {
      const res = await api.applySystemUpdate(false);
      setApplyResult(res);
    } catch (err: any) {
      setErrorMsg(err?.message || 'Failed installing release update');
    } finally {
      setIsApplying(false);
    }
  };

  const handleRestartServer = async () => {
    setErrorMsg(null);
    try {
      await api.restartSystemServer();
      setTimeout(() => {
        window.location.reload();
      }, 2200);
    } catch (err: any) {
      setErrorMsg(err?.message || 'Automatic restart failed; please restart the server manually.');
    }
  };

  const installedVersion = updateInfo?.current_version || __APP_VERSION__;
  const latestVersion = updateInfo?.latest_version || installedVersion;
  const hasUpdate = Boolean(updateInfo?.update_available);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 backdrop-blur-sm p-4"
      role="dialog"
      aria-modal="true"
      aria-label="VoxForge Enterprise Release Manager"
    >
      <div className="w-full max-w-2xl bg-[#121820] border border-[#242E3D] rounded-xl shadow-2xl overflow-hidden text-white">
        {/* Header */}
        <div className="px-5 py-4 bg-[#0B0E14] border-b border-[#242E3D] flex items-center justify-between">
          <div className="flex items-center space-x-3">
            <div className="p-2 rounded-lg bg-amber-500/10 border border-amber-500/30 text-amber-400">
              <ShieldCheck className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-sm font-bold uppercase tracking-wider font-mono text-white">
                Release & Auto-Update Manager
              </h2>
              <p className="text-[11px] font-mono text-[#94A3B8]">
                GitHub Releases telemetry with SHA-256 checksum verification
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1.5 rounded-lg text-[#64748B] hover:text-white hover:bg-[#1A222D] transition-colors"
            aria-label="Close release manager"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Body */}
        <div className="p-5 space-y-4 max-h-[75vh] overflow-y-auto">
          {/* Version Summary Cards */}
          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <div className="bg-[#0B0E14] border border-[#242E3D] rounded-lg p-3">
              <div className="text-[10px] font-mono uppercase text-[#64748B]">Installed Build</div>
              <div className="text-base font-bold font-mono text-white mt-1">
                v{installedVersion}
              </div>
              <div className="text-[10px] font-mono text-[#94A3B8] mt-0.5">
                {updateInfo ? `${updateInfo.platform}/${updateInfo.arch}` : 'detecting...'}
              </div>
            </div>

            <div className="bg-[#0B0E14] border border-[#242E3D] rounded-lg p-3">
              <div className="text-[10px] font-mono uppercase text-[#64748B]">GitHub Latest</div>
              <div
                className={`text-base font-bold font-mono mt-1 ${
                  hasUpdate ? 'text-amber-400' : 'text-emerald-400'
                }`}
              >
                {updateInfo?.latest_version ? `v${latestVersion}` : 'Unavailable'}
              </div>
              <div className="text-[10px] font-mono text-[#94A3B8] mt-0.5">
                Status: {updateInfo?.check_status || 'pending'}
              </div>
            </div>

            <div className="bg-[#0B0E14] border border-[#242E3D] rounded-lg p-3">
              <div className="text-[10px] font-mono uppercase text-[#64748B]">Install Topology</div>
              <div className="text-xs font-bold font-mono text-sky-300 mt-1.5">
                {updateInfo?.install_mode || 'standalone_binary'}
              </div>
              <div className="text-[10px] font-mono text-[#64748B] truncate mt-0.5" title={updateInfo?.executable_path}>
                {updateInfo?.can_self_update ? 'In-app update enabled' : 'Managed / Dev runtime'}
              </div>
            </div>
          </div>

          {errorMsg && (
            <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-3 flex items-start space-x-2 text-xs font-mono text-red-300">
              <AlertTriangle className="w-4 h-4 text-red-400 shrink-0 mt-0.5" />
              <span>{errorMsg}</span>
            </div>
          )}

          {updateInfo?.error && (
            <div className="bg-amber-500/10 border border-amber-500/30 rounded-lg p-3 flex items-start space-x-2 text-xs font-mono text-amber-200">
              <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0 mt-0.5" />
              <span>{updateInfo.error}</span>
            </div>
          )}

          {applyResult && (
            <div className="bg-emerald-500/10 border border-emerald-500/30 rounded-lg p-3.5 space-y-2">
              <div className="flex items-center space-x-2 text-xs font-mono font-bold text-emerald-400">
                <CheckCircle2 className="w-4 h-4" />
                <span>{applyResult.message}</span>
              </div>
              {applyResult.sha256 && (
                <div className="text-[10px] font-mono text-[#94A3B8] break-all">
                  SHA-256 Verified: <span className="text-white">{applyResult.sha256}</span>
                </div>
              )}
              {applyResult.restart_required && (
                <button
                  type="button"
                  onClick={handleRestartServer}
                  className="inline-flex items-center space-x-1.5 px-3 py-1.5 rounded bg-emerald-500 hover:bg-emerald-400 text-black text-xs font-mono font-bold transition-colors"
                >
                  <RotateCcw className="w-3.5 h-3.5" />
                  <span>Restart Server Now</span>
                </button>
              )}
            </div>
          )}

          {/* Asset & SHA-256 Verification Details */}
          <div className="bg-[#0B0E14] border border-[#242E3D] rounded-lg p-3.5 space-y-2">
            <div className="flex items-center justify-between text-xs font-mono">
              <span className="text-[#94A3B8] uppercase text-[10px]">Target Release Artifact</span>
              {updateInfo?.asset_size_bytes ? (
                <span className="text-[#94A3B8] text-[10px]">
                  {(updateInfo.asset_size_bytes / (1024 * 1024)).toFixed(2)} MB
                </span>
              ) : null}
            </div>
            <div className="flex items-center justify-between font-mono text-xs">
              <span className="text-white font-semibold">{updateInfo?.asset_name || 'voxforge'}</span>
              <a
                href={updateInfo?.release_url || 'https://github.com/AMG555/VoxForge/releases/latest'}
                target="_blank"
                rel="noopener noreferrer"
                className="text-amber-400 hover:text-amber-300 flex items-center space-x-1 text-[11px]"
              >
                <span>Open GitHub Release</span>
                <ExternalLink className="w-3 h-3" />
              </a>
            </div>
            {updateInfo?.checksum_sha256 && (
              <div className="text-[10px] font-mono text-[#64748B] bg-[#121820] px-2.5 py-1.5 rounded border border-[#242E3D] break-all">
                SHA-256: <span className="text-emerald-400">{updateInfo.checksum_sha256}</span>
              </div>
            )}
          </div>

          {updateInfo?.release_notes && (
            <div className="bg-[#0B0E14] border border-[#242E3D] rounded-lg p-3.5 space-y-1.5">
              <div className="text-[10px] font-mono uppercase text-[#94A3B8]">Release Notes</div>
              <pre className="text-[11px] font-mono text-[#CBD5E1] whitespace-pre-wrap leading-relaxed max-h-32 overflow-y-auto">
                {updateInfo.release_notes}
              </pre>
            </div>
          )}

          {/* Enterprise CLI Update Command Fallback */}
          <div className="bg-[#0B0E14] border border-[#242E3D] rounded-lg p-3.5 space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-[10px] font-mono uppercase text-[#94A3B8] flex items-center space-x-1.5">
                <Terminal className="w-3.5 h-3.5 text-amber-400" />
                <span>One-Line Terminal Upgrade ({isWindows ? 'PowerShell' : 'POSIX Shell'})</span>
              </span>
              <button
                type="button"
                onClick={handleCopyCommand}
                className="inline-flex items-center space-x-1 text-[10px] font-mono text-amber-400 hover:text-amber-300"
              >
                {copiedCmd ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
                <span>{copiedCmd ? 'Copied' : 'Copy'}</span>
              </button>
            </div>
            <code className="block text-[11px] font-mono text-sky-300 bg-[#121820] px-2.5 py-2 rounded border border-[#242E3D] break-all select-all">
              {cliCommand}
            </code>
            <p className="text-[10px] font-mono text-[#64748B]">{updateInfo?.update_hint}</p>
          </div>
        </div>

        {/* Footer */}
        <div className="px-5 py-3.5 bg-[#0B0E14] border-t border-[#242E3D] flex items-center justify-between">
          <button
            type="button"
            onClick={() => onRefresh(true)}
            disabled={isChecking || isApplying}
            className="inline-flex items-center space-x-1.5 px-3 py-1.5 rounded bg-[#1A222D] hover:bg-[#242E3D] border border-[#3B485C] text-xs font-mono text-white disabled:opacity-50 transition-colors"
          >
            <RefreshCw className={`w-3.5 h-3.5 text-amber-400 ${isChecking ? 'animate-spin' : ''}`} />
            <span>{isChecking ? 'Checking...' : 'Check Now'}</span>
          </button>

          <div className="flex items-center space-x-2">
            {hasUpdate && updateInfo?.can_self_update && (
              <button
                type="button"
                onClick={handleApplyUpdate}
                disabled={isApplying}
                className="inline-flex items-center space-x-1.5 px-4 py-1.5 rounded bg-amber-500 hover:bg-amber-400 text-black text-xs font-mono font-bold disabled:opacity-50 transition-colors"
              >
                <Download className="w-3.5 h-3.5" />
                <span>
                  {isApplying ? 'Verifying & Installing...' : `Install Update v${latestVersion}`}
                </span>
              </button>
            )}
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-1.5 rounded bg-[#1A222D] hover:bg-[#242E3D] text-xs font-mono text-[#94A3B8] hover:text-white transition-colors"
            >
              Close
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
