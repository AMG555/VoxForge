import React, { useMemo, useState } from 'react';
import { Languages, Search, Globe2 } from 'lucide-react';
import {
  ALL_646_LANGUAGES,
  DetectedLanguage,
  REGIONAL_DIALECTS,
  SUPPORTED_LANGUAGES,
  normalizeLanguageCode,
} from '../../services/languageDetector';

interface LanguageDialectBarProps {
  selectedLanguageCode: string;
  onSelectLanguage: (code: string) => void;
  detectedLang: DetectedLanguage;
  autoMatchVoice: boolean;
  onToggleAutoMatchVoice: (checked: boolean) => void;
  isClonedVoiceSelected: boolean;
  charCount: number;
  wordCount: number;
}

/**
 * Modular prompt language, 646-language search, and regional accent selector bar.
 * Keeps cloned speaker selection intact when switching target languages (cross-lingual mode).
 */
export const LanguageDialectBar: React.FC<LanguageDialectBarProps> = ({
  selectedLanguageCode,
  onSelectLanguage,
  detectedLang,
  autoMatchVoice,
  onToggleAutoMatchVoice,
  isClonedVoiceSelected,
  charCount,
  wordCount,
}) => {
  const [showAllPicker, setShowAllPicker] = useState(false);
  const [langFilter, setLangFilter] = useState('');

  const effectiveBase = useMemo(() => {
    const active = selectedLanguageCode === 'AUTO' ? detectedLang.code : selectedLanguageCode;
    return normalizeLanguageCode(active).split('-')[0].toLowerCase();
  }, [selectedLanguageCode, detectedLang.code]);

  const regionalOptions = REGIONAL_DIALECTS[effectiveBase] || [];

  const filteredLanguages = useMemo(() => {
    const q = langFilter.trim().toLowerCase();
    const pool = showAllPicker ? ALL_646_LANGUAGES : SUPPORTED_LANGUAGES;
    if (!q) return pool;
    return ALL_646_LANGUAGES.filter(
      (lang) =>
        lang.name.toLowerCase().includes(q) || lang.code.toLowerCase().includes(q),
    );
  }, [showAllPicker, langFilter]);

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <label
            htmlFor="vox-target-lang-select"
            className="text-xs font-semibold uppercase tracking-wider text-[#94A3B8] flex items-center space-x-1.5"
          >
            <Languages className="w-3.5 h-3.5 text-amber-400" />
            <span>Prompt &amp; Output Language</span>
          </label>

          <select
            id="vox-target-lang-select"
            aria-label="Prompt and output language"
            value={selectedLanguageCode}
            onChange={(e) => onSelectLanguage(e.target.value)}
            className="bg-[#0B0E14] border border-[#242E3D] rounded px-2.5 py-1 text-xs font-mono text-[#CBD5E1] hover:border-amber-500/50 focus:border-amber-500 focus:outline-none cursor-pointer max-w-[230px]"
            title="Choose output language manually or keep on Auto-Detect"
          >
            {filteredLanguages.map((lang) => (
              <option key={lang.code} value={lang.code} className="bg-[#121820] text-white">
                {lang.flag} {lang.name}
              </option>
            ))}
          </select>

          <button
            type="button"
            onClick={() => setShowAllPicker((prev) => !prev)}
            className={`flex items-center space-x-1 px-2 py-1 rounded text-[11px] font-mono border transition-colors ${
              showAllPicker
                ? 'bg-amber-500/15 border-amber-500/50 text-amber-300'
                : 'bg-[#0B0E14] border-[#242E3D] text-[#94A3B8] hover:text-white hover:border-[#3B485C]'
            }`}
            aria-expanded={showAllPicker}
            title="Browse or search all 646 supported neural cloning languages"
          >
            <Globe2 className="w-3 h-3" />
            <span>646 Languages</span>
          </button>

          <div className="flex items-center space-x-1.5 px-2.5 py-0.5 rounded-full bg-[#121820] border border-[#242E3D] text-[11px] font-mono">
            <span className="w-1.5 h-1.5 rounded-full bg-emerald-400" />
            <span>{detectedLang.flag}</span>
            <span className="text-white font-medium">{detectedLang.name}</span>
            {selectedLanguageCode === 'AUTO' && (
              <span className="text-[10px] text-amber-400/80">
                ({Math.round(detectedLang.confidence * 100)}%)
              </span>
            )}
          </div>

          {isClonedVoiceSelected ? (
            <div
              className="flex items-center space-x-1.5 px-2.5 py-0.5 rounded bg-amber-500/10 border border-amber-500/30 text-[11px] font-mono text-amber-300"
              title="Selected cloned voice stays active and speaks the chosen output language"
            >
              <span>Cross-Lingual Clone Active</span>
            </div>
          ) : (
            <label
              className="flex items-center space-x-1 text-[11px] font-mono text-[#CBD5E1] cursor-pointer px-2 py-0.5 rounded bg-[#0B0E14] border border-[#242E3D] hover:border-amber-500/40"
              title="Automatically switch preset voice to match detected language"
            >
              <input
                type="checkbox"
                checked={autoMatchVoice}
                onChange={(e) => onToggleAutoMatchVoice(e.target.checked)}
                className="accent-amber-500 w-3 h-3"
              />
              <span>Auto-Match Voice</span>
            </label>
          )}
        </div>

        <div className="text-[11px] font-mono text-[#64748B]">
          {charCount} chars • {wordCount} words
        </div>
      </div>

      {(showAllPicker || regionalOptions.length > 0) && (
        <div className="flex flex-wrap items-center justify-between gap-2 bg-[#0B0E14] border border-[#242E3D] rounded-lg px-2.5 py-1.5">
          {showAllPicker && (
            <div className="relative flex-1 min-w-[200px] max-w-xs">
              <Search className="w-3 h-3 absolute left-2.5 top-2 text-[#64748B]" />
              <input
                type="text"
                aria-label="Filter 646 languages"
                value={langFilter}
                onChange={(e) => setLangFilter(e.target.value)}
                placeholder="Filter 646 languages (e.g. Malayalam, Yoruba, arb)..."
                className="w-full bg-[#121820] border border-[#242E3D] rounded pl-7 pr-2.5 py-1 text-[11px] text-white placeholder-[#64748B] focus:border-amber-500 focus:outline-none font-mono"
              />
            </div>
          )}

          {regionalOptions.length > 0 && (
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="text-[10px] font-mono uppercase tracking-wider text-[#64748B]">
                Accent:
              </span>
              {regionalOptions.map((opt) => {
                const isSelected =
                  selectedLanguageCode.toLowerCase() === opt.code.toLowerCase();
                return (
                  <button
                    key={opt.code}
                    type="button"
                    onClick={() => onSelectLanguage(opt.code)}
                    className={`px-2 py-0.5 rounded text-[11px] font-mono border transition-colors ${
                      isSelected
                        ? 'bg-amber-500/20 border-amber-500 text-amber-300 font-semibold'
                        : 'bg-[#121820] border-[#242E3D] text-[#94A3B8] hover:text-white hover:border-[#3B485C]'
                    }`}
                  >
                    {opt.label}
                  </button>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
};
