import React from 'react';
import { Minus, Square, X, Cpu } from 'lucide-react';
import {
  closeDesktopWindow,
  minimizeDesktopWindow,
  toggleMaximizeDesktopWindow,
} from '../../services/desktopBridge';
import { useAppDispatch, useAppSelector } from '../../store';
import { useGetSetupStatusQuery } from '../../store/setupApi';
import { setWizardOpen } from '../../store/setupSlice';

export const TitleBar: React.FC = () => {
  const dispatch = useAppDispatch();
  const { isDesktop, desktopVersion } = useAppSelector((s) => s.setup);
  const { data: setupStatus } = useGetSetupStatusQuery(undefined, {
    skip: !isDesktop,
  });

  if (!isDesktop) return null;

  const activeProfile = setupStatus?.config?.runtime_profile || 'rust_core';
  const profileLabel =
    activeProfile === 'full_cloning'
      ? 'Rust + Full Sidecar'
      : activeProfile === 'core_neural' && setupStatus?.managed_venv_ready
        ? 'Rust + Neural Sidecar'
        : activeProfile === 'remote'
          ? 'Remote Server'
          : 'Native Rust Core';

  return (
    <div
      data-tauri-drag-region
      className="h-9 bg-[#080B10] border-b border-[#1E2633] flex items-center justify-between pl-3 pr-0 select-none shrink-0 z-50"
    >
      <div data-tauri-drag-region className="flex items-center space-x-2.5 min-w-0">
        <span className="inline-flex items-center justify-center w-5 h-5 rounded bg-gradient-to-br from-amber-400 to-orange-600 text-[#0B0E14] font-bold text-[10px]">
          VF
        </span>
        <span data-tauri-drag-region className="text-xs font-semibold text-white tracking-tight">
          VoxForge
        </span>
        <span className="text-[10px] font-mono text-[#64748B]">v{desktopVersion}</span>
        <button
          type="button"
          onClick={() => dispatch(setWizardOpen(true))}
          className="inline-flex items-center space-x-1.5 px-2 py-0.5 rounded bg-[#121820] hover:bg-[#1A222D] border border-[#242E3D] text-[10px] font-mono text-emerald-400 transition-colors"
          title="Runtime & Optional AI Extensions Setup"
        >
          <Cpu className="w-3 h-3" />
          <span>{profileLabel}</span>
        </button>
      </div>

      <div data-tauri-drag-region className="flex-1 h-full" />

      <div className="flex items-center h-full">
        <button
          type="button"
          onClick={minimizeDesktopWindow}
          aria-label="Minimize window"
          className="h-full px-3 inline-flex items-center justify-center text-[#94A3B8] hover:text-white hover:bg-[#1E2633] transition-colors"
        >
          <Minus className="w-3.5 h-3.5" />
        </button>
        <button
          type="button"
          onClick={toggleMaximizeDesktopWindow}
          aria-label="Maximize or restore window"
          className="h-full px-3 inline-flex items-center justify-center text-[#94A3B8] hover:text-white hover:bg-[#1E2633] transition-colors"
        >
          <Square className="w-3 h-3" />
        </button>
        <button
          type="button"
          onClick={closeDesktopWindow}
          aria-label="Close window"
          className="h-full px-3.5 inline-flex items-center justify-center text-[#94A3B8] hover:text-white hover:bg-red-600 transition-colors"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  );
};
