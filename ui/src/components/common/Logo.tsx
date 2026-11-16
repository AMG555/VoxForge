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
            <linearGradient id="navGlyph" x1="0%" y1="0%" x2="100%" y2="100%">
              <stop offset="0%" stopColor="#FFFFFF" />
              <stop offset="100%" stopColor="#FEF3C7" />
            </linearGradient>
          </defs>
          <rect width="256" height="256" rx="56" fill="#0B0E14" />
          <circle cx="128" cy="128" r="106" stroke="#1E293B" strokeWidth="3" />
          <path d="M128 36 L196 66 C196 132 170 186 128 216 C86 186 60 132 60 66 Z" fill="url(#navShield)" />
          <line x1="104" y1="82" x2="104" y2="128" stroke="url(#navGlyph)" strokeWidth="14" strokeLinecap="round" />
          <line x1="128" y1="62" x2="128" y2="128" stroke="url(#navGlyph)" strokeWidth="14" strokeLinecap="round" />
          <line x1="152" y1="82" x2="152" y2="128" stroke="url(#navGlyph)" strokeWidth="14" strokeLinecap="round" />
          <path d="M72 142 L184 142" stroke="url(#navGlyph)" strokeWidth="14" strokeLinecap="round" />
          <path d="M102 148 C106 160 100 172 90 174 L166 174 C156 172 150 160 154 148" fill="url(#navGlyph)" />
          <line x1="86" y1="176" x2="170" y2="176" stroke="url(#navGlyph)" strokeWidth="12" strokeLinecap="round" />
          <circle cx="128" cy="46" r="6" fill="#FFFFFF" />
        </svg>
      </div>

      {showText && (
        <div className="flex flex-col">
          <div className="flex items-center space-x-1.5">
            <span className="font-extrabold tracking-wider text-sm font-mono text-white">
              VOX<span className="text-amber-400">FORGE</span>
            </span>
            <span className="text-[9px] uppercase font-mono px-1 py-0.2 rounded bg-[#1A222D] text-[#94A3B8] border border-[#242E3D]">
              v0.4.0
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
