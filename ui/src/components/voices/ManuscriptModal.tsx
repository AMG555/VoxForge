import React, { useState } from 'react';
import { X, BookOpen, FileText, Upload, Sparkles, Clock, Hash, CheckCircle2, ArrowRight, Copy } from 'lucide-react';
import { api } from '../../services/api';

interface ManuscriptModalProps {
  isOpen: boolean;
  onClose: () => void;
  onLoadChapter: (text: string, title: string) => void;
}

interface ParsedChapter {
  index: number;
  title: string;
  text: string;
  voice_id?: string;
  speed?: number;
  content_hash?: string;
  sha256_hash?: string;
  word_count: number;
  estimated_duration_minutes: number;
}

interface ManuscriptResult {
  title: string;
  total_chapters: number;
  total_words: number;
  estimated_duration_minutes: number;
  chapters: ParsedChapter[];
}

export const ManuscriptModal: React.FC<ManuscriptModalProps> = ({ isOpen, onClose, onLoadChapter }) => {
  const [text, setText] = useState<string>('');
  const [maxWords, setMaxWords] = useState<number>(1000);
  const [parsing, setParsing] = useState<boolean>(false);
  const [result, setResult] = useState<ManuscriptResult | null>(null);
  const [error, setError] = useState<string>('');
  const [copiedHash, setCopiedHash] = useState<string>('');

  if (!isOpen) return null;

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (event) => {
      const content = event.target?.result as string;
      if (content) {
        setText(content);
      }
    };
    reader.readAsText(file);
  };

  const handleParse = async () => {
    if (!text.trim()) {
      setError('Please provide manuscript text or upload a markdown file.');
      return;
    }
    setError('');
    setParsing(true);
    try {
      const res = await api.parseManuscript({
        manuscript_text: text,
        max_words_per_chapter: maxWords > 0 ? maxWords : 1000,
      });
      const chapters: ParsedChapter[] = (res.chapters || []).map((ch: any) => {
        const words = ch.word_count ?? (ch.text ? ch.text.split(/\s+/).filter(Boolean).length : 0);
        const duration = ch.estimated_duration_minutes ?? (words / 150.0);
        return {
          ...ch,
          word_count: words,
          estimated_duration_minutes: duration,
          content_hash: ch.content_hash || ch.sha256_hash || '',
        };
      });
      setResult({
        ...res,
        total_words: res.total_words ?? chapters.reduce((acc, c) => acc + (c.word_count || 0), 0),
        total_chapters: res.total_chapters ?? chapters.length,
        estimated_duration_minutes: res.estimated_duration_minutes ?? chapters.reduce((acc, c) => acc + (c.estimated_duration_minutes || 0), 0),
        chapters,
      });
    } catch (err: any) {
      setError(err.message || 'Failed to parse manuscript');
    } finally {
      setParsing(false);
    }
  };

  const handleLoadChapter = (chapter: ParsedChapter) => {
    onLoadChapter(chapter.text, chapter.title);
    onClose();
  };

  const handleCopyJson = () => {
    if (!result) return;
    navigator.clipboard.writeText(JSON.stringify(result, null, 2));
    setCopiedHash('manifest');
    setTimeout(() => setCopiedHash(''), 2000);
  };

  const sampleMarkdown = `# The Chrono Engine
## Chapter 1: The First Spark
The laboratory smelled of ozone and ancient copper. Doctor Aris adjusted the calibrated frequency dial, listening intently as the harmonic oscillators began their rhythmic drone. "Once the carrier wave locks onto the temporal resonance," he whispered, "the vocal pattern will transcend chronological barriers."

## Chapter 2: The Echo in the Wire
Across the glass observation pane, the audio visualizer oscillated with eerie symmetry. The speaker's voice emerged through the monitors—not as a synthesized imitation, but as an absolute acoustic mirror of the original speaker, resonant and full of natural human cadence.`;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/80 backdrop-blur-md p-4 sm:p-6 overflow-y-auto">
      <div className="relative w-full max-w-4xl max-h-[90vh] bg-[#0E131A] border border-[#242E3D] rounded-xl shadow-2xl flex flex-col overflow-hidden text-[#E2E8F0]">
        {/* Header */}
        <div className="p-4 sm:p-5 border-b border-[#242E3D] flex items-center justify-between bg-[#161D26]">
          <div className="flex items-center space-x-3">
            <div className="p-2 rounded-lg bg-emerald-500/10 border border-emerald-500/20 text-emerald-400">
              <BookOpen className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base sm:text-lg font-semibold text-white tracking-wide flex items-center gap-2">
                Audiobook Manuscript Ingestion
                <span className="text-[10px] uppercase font-mono px-2 py-0.5 rounded bg-emerald-500/20 text-emerald-300 border border-emerald-500/30">
                  Chapter Parser
                </span>
              </h2>
              <p className="text-xs text-[#94A3B8]">
                Parse long-form markdown books into chapters with word counts, duration telemetry, and studio batching.
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1.5 rounded-lg text-[#64748B] hover:text-white hover:bg-[#1E2633] transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Body */}
        <div className="p-5 overflow-y-auto space-y-5 flex-1">
          {error && (
            <div className="p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-xs font-mono">
              {error}
            </div>
          )}

          {/* Text Area & Input Controls */}
          <div className="space-y-2">
            <div className="flex items-center justify-between">
              <label className="text-xs font-medium text-[#94A3B8] flex items-center gap-1.5">
                <FileText className="w-3.5 h-3.5 text-emerald-400" />
                Manuscript Markdown or Prose:
              </label>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  onClick={() => setText(sampleMarkdown)}
                  className="text-[11px] font-mono text-emerald-400 hover:text-emerald-300 hover:underline"
                >
                  Load Sample
                </button>
                <label className="cursor-pointer text-[11px] font-mono flex items-center gap-1 text-sky-400 hover:text-sky-300 bg-sky-500/10 border border-sky-500/30 px-2 py-0.5 rounded">
                  <Upload className="w-3 h-3" />
                  <span>Upload .md/.txt</span>
                  <input
                    type="file"
                    accept=".md,.txt,.markdown"
                    className="hidden"
                    onChange={handleFileUpload}
                  />
                </label>
              </div>
            </div>

            <textarea
              rows={8}
              value={text}
              onChange={(e) => setText(e.target.value)}
              placeholder="# Book Title&#10;&#10;## Chapter 1: The Beginning&#10;Paste your full manuscript here. Headers (# or ##) define chapter boundaries. Paragraphs are automatically chunked if headers are absent."
              className="w-full bg-[#161D26] border border-[#242E3D] rounded-lg p-3 text-xs font-mono text-[#E2E8F0] placeholder-[#64748B] focus:outline-none focus:border-emerald-500/50 resize-y"
            />

            <div className="flex flex-wrap items-center justify-between gap-3 pt-1">
              <div className="flex items-center gap-2 text-xs text-[#94A3B8]">
                <span>Chunk threshold (words/chapter):</span>
                <input
                  type="number"
                  min={100}
                  max={5000}
                  step={100}
                  value={maxWords}
                  onChange={(e) => setMaxWords(parseInt(e.target.value) || 1000)}
                  className="w-20 bg-[#161D26] border border-[#242E3D] rounded px-2 py-1 text-xs font-mono text-white text-right focus:outline-none focus:border-emerald-500/50"
                />
              </div>

              <button
                type="button"
                onClick={handleParse}
                disabled={parsing || !text.trim()}
                className="flex items-center space-x-1.5 px-4 py-2 rounded-lg bg-emerald-600 hover:bg-emerald-500 disabled:opacity-50 text-white text-xs font-semibold shadow-md transition-all"
              >
                <Sparkles className="w-3.5 h-3.5" />
                <span>{parsing ? 'Parsing Manuscript...' : 'Parse & Analyze Chapters'}</span>
              </button>
            </div>
          </div>

          {/* Parsed Results Overview */}
          {result && (
            <div className="space-y-4 pt-2 border-t border-[#242E3D]">
              <div className="p-4 bg-[#161D26] rounded-xl border border-[#242E3D] space-y-3">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <span className="text-xs text-[#94A3B8]">Title:</span>
                    <span className="text-sm font-semibold text-white">{result.title}</span>
                  </div>
                  <button
                    onClick={handleCopyJson}
                    className="flex items-center gap-1 text-[11px] font-mono text-[#94A3B8] hover:text-white bg-[#0E131A] px-2 py-1 rounded border border-[#242E3D]"
                  >
                    {copiedHash === 'manifest' ? (
                      <>
                        <CheckCircle2 className="w-3 h-3 text-emerald-400" />
                        <span className="text-emerald-400">Copied JSON!</span>
                      </>
                    ) : (
                      <>
                        <Copy className="w-3 h-3" />
                        <span>Export Manifest JSON</span>
                      </>
                    )}
                  </button>
                </div>

                <div className="grid grid-cols-3 gap-3 text-center">
                  <div className="p-2.5 rounded-lg bg-[#0E131A] border border-[#242E3D]">
                    <div className="text-[11px] text-[#94A3B8] font-mono uppercase">Chapters</div>
                    <div className="text-lg font-bold text-emerald-400 font-mono">{result.total_chapters}</div>
                  </div>
                  <div className="p-2.5 rounded-lg bg-[#0E131A] border border-[#242E3D]">
                    <div className="text-[11px] text-[#94A3B8] font-mono uppercase">Total Words</div>
                    <div className="text-lg font-bold text-sky-400 font-mono">{result.total_words.toLocaleString()}</div>
                  </div>
                  <div className="p-2.5 rounded-lg bg-[#0E131A] border border-[#242E3D]">
                    <div className="text-[11px] text-[#94A3B8] font-mono uppercase">Est. Audio Time</div>
                    <div className="text-lg font-bold text-amber-400 font-mono">
                      {result.estimated_duration_minutes.toFixed(1)}m
                    </div>
                  </div>
                </div>
              </div>

              {/* Chapter Cards List */}
              <div className="space-y-2">
                <h3 className="text-xs font-semibold text-[#94A3B8] uppercase tracking-wider">
                  Detected Chapters ({result.chapters.length})
                </h3>

                <div className="space-y-2 max-h-64 overflow-y-auto pr-1">
                  {result.chapters.map((ch) => (
                    <div
                      key={ch.index}
                      className="p-3 bg-[#161D26] hover:bg-[#1E2633] border border-[#242E3D] rounded-lg transition-colors flex items-center justify-between gap-4"
                    >
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-2">
                          <span className="text-xs font-mono px-1.5 py-0.5 rounded bg-[#0E131A] text-emerald-400 border border-[#242E3D]">
                            #{ch.index}
                          </span>
                          <span className="text-xs font-semibold text-white truncate">{ch.title}</span>
                        </div>
                        <div className="flex items-center gap-3 text-[11px] text-[#94A3B8] font-mono mt-1">
                          <span className="flex items-center gap-1">
                            <Hash className="w-3 h-3 text-[#64748B]" />
                            {ch.word_count} words
                          </span>
                          <span className="flex items-center gap-1">
                            <Clock className="w-3 h-3 text-amber-400" />
                            ~{ch.estimated_duration_minutes.toFixed(1)} min
                          </span>
                          <span
                            className="text-[10px] text-[#64748B] truncate max-w-[120px]"
                            title={ch.content_hash || ch.sha256_hash || ''}
                          >
                            {(ch.content_hash || ch.sha256_hash || '').substring(0, 10)}...
                          </span>
                        </div>
                      </div>

                      <button
                        onClick={() => handleLoadChapter(ch)}
                        className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-emerald-500/10 hover:bg-emerald-500/20 text-emerald-400 border border-emerald-500/30 text-xs font-mono font-medium transition-all shrink-0"
                      >
                        <span>Load to Studio</span>
                        <ArrowRight className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="p-4 border-t border-[#242E3D] bg-[#0E131A] flex justify-end">
          <button
            onClick={onClose}
            className="px-4 py-1.5 rounded-lg bg-[#1E2633] hover:bg-[#2A3444] text-[#E2E8F0] text-xs font-medium transition-colors"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
