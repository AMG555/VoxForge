import React from 'react';

interface LogoProps {
  size?: number;
  className?: string;
  showText?: boolean;
}

export const Logo: React.FC<LogoProps> = ({ size = 28, className = '', showText = true }) => {
  return (
    <div className={`flex items-center space-x-2.5 select-none ${className}`}>
      <div
        className="relative flex items-center justify-center rounded-lg overflow-hidden group cursor-pointer"
        style={{ width: size, height: size }}
      >
        <svg
          viewBox="0 0 256 256"
          className="w-full h-full"
          fill="none"
          xmlns="http://www.w3.org/2000/svg"
        >
          <defs>
            <linearGradient id="navShield" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#F59E0B" />
              <stop offset="45%" stopColor="#EA580C" />
              <stop offset="100%" stopColor="#9A3412" />
            </linearGradient>
            <linearGradient id="navGlyph" x1="128" y1="56" x2="128" y2="180" gradientUnits="userSpaceOnUse">
              <stop offset="0%" stopColor="#FFFFFF" />
              <stop offset="100%" stopColor="#FEF3C7" />
            </linearGradient>
            <filter id="navDrop" x="-12%" y="-12%" width="124%" height="124%">
              <feDropShadow dx="0" dy="5" stdDeviation="7" floodColor="#000000" floodOpacity="0.35" />
            </filter>
          </defs>

          {/* Dark Obsidian Tile */}
          <rect width="256" height="256" rx="56" fill="#0B0E14" />
          <circle cx="128" cy="128" r="106" stroke="#1E293B" strokeWidth="2" />

          {/* Molten Forge Shield */}
          <g filter="url(#navDrop)">
            <path
              d="M128 36 L196 66 C196 132 170 186 128 216 C86 186 60 132 60 66 Z"
              fill="url(#navShield)"
            />
          </g>

          {/* 3 Harmonic Acoustic Wave Bars ("VOX") - 2D geometry with rx=6 */}
          <rect x="98" y="76" width="12" height="58" rx="6" fill="url(#navGlyph)" />
          <rect x="122" y="56" width="12" height="78" rx="6" fill="url(#navGlyph)" />
          <rect x="146" y="76" width="12" height="58" rx="6" fill="url(#navGlyph)" />

          {/* Radiating Sonic Arcs */}
          <path d="M84 92 C80 102 80 114 84 124" fill="none" stroke="url(#navGlyph)" strokeWidth="7" strokeLinecap="round" />
          <path d="M172 92 C176 102 176 114 172 124" fill="none" stroke="url(#navGlyph)" strokeWidth="7" strokeLinecap="round" />

          {/* Sleek Forged Anvil ("FORGE") */}
          <rect x="66" y="136" width="124" height="12" rx="6" fill="url(#navGlyph)" />
          <path
            d="M102 148 C106 160 100 172 90 174 L166 174 C156 172 150 160 154 148 Z"
            fill="url(#navGlyph)"
          />
          <rect x="81" y="171" width="94" height="10" rx="5" fill="url(#navGlyph)" />

          {/* Center Ember Core Point */}
          <circle cx="128" cy="160" r="4.5" fill="#EA580C" />

          {/* Neural Creation Spark */}
          <circle cx="128" cy="46" r="4.5" fill="#FFFFFF" />
        </svg>
      </div>

      {showText && (
        <div className="flex flex-col">
          <div className="flex items-center space-x-1.5">
            <span className="font-extrabold tracking-wider text-sm font-mono text-white">
              VOX<span className="text-amber-400">FORGE</span>
            </span>
            <span className="text-[9px] uppercase font-mono px-1 py-0.2 rounded bg-[#1A222D] text-[#94A3B8] border border-[#242E3D]">
              v0.4.1
            </span>
          </div>
          <span className="text-[9px] text-[#64748B] font-mono tracking-tight -mt-0.5">
            NEURAL SPEECH STUDIO
          </span>
        </div>
      )}
    </div>
  );
};
