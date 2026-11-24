import React from 'react';
import { Activity, Cpu, GitFork, Layers, Mic, Scale, LayoutGrid, Music2 } from 'lucide-react';
import { HardwareInfo } from '../types';
import { Logo } from './common/Logo';
import { AgentBeacon } from './common/AgentBeacon';

interface NavigationProps {
  activeTab: 'workflows' | 'canvas' | 'timeline' | 'voices' | 'catalog' | 'hardware' | 'qa';
  setActiveTab: (tab: 'workflows' | 'canvas' | 'timeline' | 'voices' | 'catalog' | 'hardware' | 'qa') => void;
  hardware: HardwareInfo | null;
  onToggleDictation?: () => void;
  isDictationOpen?: boolean;
}

export const Navigation: React.FC<NavigationProps> = ({
  activeTab,
  setActiveTab,
  hardware,
  onToggleDictation,
  isDictationOpen,
}) => {
  return (
    <header className="h-14 border-b border-[#242E3D] bg-[#121820] flex items-center justify-between px-4 select-none shrink-0">
      <div className="flex items-center space-x-6">
        <Logo size={32} />

        <nav className="flex space-x-1">
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
                className={`relative flex items-center space-x-2 px-3 py-1.5 rounded text-xs font-medium transition-all ${
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

      <div className="flex items-center space-x-3">
        {onToggleDictation && (
          <button
            onClick={onToggleDictation}
            className={`flex items-center space-x-1.5 px-2.5 py-1 rounded text-xs font-mono transition-colors border ${
              isDictationOpen
                ? 'bg-red-500/20 text-red-300 border-red-500/40 animate-pulse'
                : 'bg-[#1A222D] hover:bg-[#242E3D] text-[#94A3B8] hover:text-white border-[#242E3D]'
            }`}
            title="VoxForge Push-to-Talk Dictation (Ctrl+Shift+D)"
          >
            <Mic className="w-3.5 h-3.5 text-amber-400" />
            <span>Dictate</span>
          </button>
        )}

        <AgentBeacon />

        <a
          href="/docs"
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-center space-x-1.5 px-2.5 py-1 rounded text-xs font-mono text-amber-400 hover:text-amber-300 bg-amber-500/10 hover:bg-amber-500/20 border border-amber-500/30 transition-colors"
          title="Interactive Scalar OpenAPI Reference"
        >
          <Activity className="w-3.5 h-3.5" />
          <span>API Docs</span>
        </a>

        {hardware && (
          <div className="flex items-center space-x-2 text-xs font-mono text-[#94A3B8] bg-[#0B0E14] px-3 py-1 rounded border border-[#242E3D]">
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
