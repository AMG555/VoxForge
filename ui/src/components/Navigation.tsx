import React from 'react';
import { Activity, Cpu, GitFork, Layers, Mic, Scale, LayoutGrid, Music2, ArrowUpCircle } from 'lucide-react';
import { HardwareInfo, SystemUpdateInfo } from '../types';
import { Logo } from './common/Logo';
import { AgentBeacon } from './common/AgentBeacon';

interface NavigationProps {
  activeTab: 'workflows' | 'canvas' | 'timeline' | 'voices' | 'catalog' | 'hardware' | 'qa';
  setActiveTab: (tab: 'workflows' | 'canvas' | 'timeline' | 'voices' | 'catalog' | 'hardware' | 'qa') => void;
  hardware: HardwareInfo | null;
  onToggleDictation?: () => void;
  isDictationOpen?: boolean;
  updateInfo?: SystemUpdateInfo | null;
  onOpenUpdateCenter?: () => void;
}

export const Navigation: React.FC<NavigationProps> = ({
  activeTab,
  setActiveTab,
  hardware,
  onToggleDictation,
  isDictationOpen,
  updateInfo,
  onOpenUpdateCenter,
}) => {
  return (
    <header className="h-14 border-b border-[#242E3D] bg-[#121820] flex items-center justify-between px-3 gap-2 select-none shrink-0 relative z-30">
      <div className="flex items-center space-x-3 min-w-0 flex-1">
        <Logo size={30} className="shrink-0" />

        <nav className="flex space-x-1 overflow-x-auto min-w-0">
          {[
            { id: 'workflows', label: 'Workflows', icon: LayoutGrid, title: 'Workflows & Projects Dashboard' },
            { id: 'canvas', label: 'Pipeline Canvas', icon: GitFork, title: 'Infinite Visual DSP DAG Pipeline Builder' },
            { id: 'timeline', label: 'Stories Timeline', icon: Music2, title: 'Stories Multi-Track Timeline (DAW)' },
            { id: 'voices', label: 'Voice Lab', icon: Mic, title: 'Neural Speech & Voice Cloning Lab' },
            { id: 'catalog', label: 'Model Catalog', icon: Layers, title: 'Model Catalog & Engine Packages' },
            { id: 'hardware', label: 'Hardware & Engines', icon: Cpu, title: 'Hardware Profiler & Engine Status' },
            { id: 'qa', label: 'A/B & QA Lab', icon: Scale, title: 'Double-Blind MOS Quality Benchmark' },
          ].map(({ id, label, icon: Icon, title }) => {
            const isActive = activeTab === id;
            return (
              <button
                key={id}
                onClick={() => setActiveTab(id as any)}
                className={`relative flex items-center space-x-1.5 px-2.5 py-1.5 rounded text-xs font-medium whitespace-nowrap transition-all ${
                  isActive
                    ? 'bg-[#1A222D] text-white border border-[#3B485C] shadow-sm font-semibold'
                    : 'text-[#94A3B8] hover:text-white hover:bg-[#1A222D]/50 border border-transparent'
                }`}
                title={title}
              >
                <Icon className={`w-3.5 h-3.5 ${isActive ? 'text-amber-400' : 'text-[#64748B]'}`} />
                <span>{label}</span>
                {isActive && (
                  <span className="absolute -bottom-[1px] left-2 right-2 h-[2px] bg-amber-400 rounded-full shadow-[0_0_6px_rgba(251,191,36,0.6)]" />
                )}
              </button>
            );
          })}
        </nav>
      </div>

      <div className="flex items-center space-x-2 shrink-0">
        {onToggleDictation && (
          <button
            onClick={onToggleDictation}
            className={`flex items-center space-x-1.5 px-2 py-1 rounded text-xs font-mono whitespace-nowrap transition-colors border ${
              isDictationOpen
                ? 'bg-red-500/20 text-red-300 border-red-500/40 animate-pulse'
                : 'bg-[#1A222D] hover:bg-[#242E3D] text-[#94A3B8] hover:text-white border-[#242E3D]'
            }`}
            title="VoxForge Push-to-Talk Dictation (Ctrl+Shift+D)"
          >
            <Mic className="w-3.5 h-3.5 text-amber-400" />
            <span className="hidden sm:inline">Dictate</span>
          </button>
        )}

        <AgentBeacon />

        {onOpenUpdateCenter && (
          <button
            type="button"
            onClick={onOpenUpdateCenter}
            className={`flex items-center space-x-1.5 px-2 py-1 rounded text-xs font-mono whitespace-nowrap transition-colors border ${
              updateInfo?.update_available
                ? 'bg-amber-500/20 hover:bg-amber-500/30 text-amber-300 border-amber-500/50 font-semibold'
                : 'bg-[#1A222D] hover:bg-[#242E3D] text-[#94A3B8] hover:text-white border-[#242E3D]'
            }`}
            title={
              updateInfo?.update_available
                ? `New VoxForge release v${updateInfo.latest_version} available on GitHub`
                : `Software Releases & Auto-Update (v${updateInfo?.current_version || __APP_VERSION__})`
            }
          >
            {updateInfo?.update_available && (
              <span className="w-2 h-2 rounded-full bg-amber-400 animate-pulse shrink-0" />
            )}
            <ArrowUpCircle className={`w-3.5 h-3.5 ${updateInfo?.update_available ? 'text-amber-400' : 'text-[#64748B]'}`} />
            <span>
              {updateInfo?.update_available && updateInfo.latest_version
                ? `Update v${updateInfo.latest_version}`
                : 'Updates'}
            </span>
          </button>
        )}

        <a
          href="/docs"
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center space-x-1.5 px-2 py-1 rounded text-xs font-mono whitespace-nowrap text-amber-400 hover:text-amber-300 bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 transition-colors"
          title="Interactive Scalar OpenAPI Reference"
        >
          <Activity className="w-3.5 h-3.5" />
          <span className="hidden lg:inline">API Docs</span>
        </a>

        {hardware && (
          <div
            className="hidden 2xl:flex items-center space-x-2 text-xs font-mono whitespace-nowrap text-[#94A3B8] bg-[#0B0E14] px-2.5 py-1 rounded border border-[#242E3D]"
            title={`${hardware.hardware_tier} | ${hardware.registered_engines.length} Registered Engines`}
          >
            <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
            <span>{hardware.hardware_tier}</span>
            <span className="text-[#3B485C]">|</span>
            <span>{hardware.registered_engines.length} Engines</span>
          </div>
        )}
      </div>
    </header>
  );
};
