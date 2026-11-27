import React, { useEffect, useState } from 'react';
import { X, Activity, ExternalLink, Play, Loader2, Search } from 'lucide-react';
import { getApiBaseUrl, getHeaders } from '../../services/api';
import { openExternalUrl } from '../../services/desktopBridge';

interface ApiDocsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

interface EndpointItem {
  path: string;
  method: string;
  summary: string;
  description: string;
  defaultBody?: string;
}

export const ApiDocsModal: React.FC<ApiDocsModalProps> = ({ isOpen, onClose }) => {
  const [endpoints, setEndpoints] = useState<EndpointItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [expandedKey, setExpandedKey] = useState<string | null>(null);
  const [bodies, setBodies] = useState<Record<string, string>>({});
  const [responses, setResponses] = useState<Record<string, { status: string; body: string }>>({});
  const [runningKey, setRunningKey] = useState<string | null>(null);

  const baseUrl = getApiBaseUrl() || (typeof window !== 'undefined' ? window.location.origin : 'http://127.0.0.1:8080');

  useEffect(() => {
    if (!isOpen) return;
    setLoading(true);
    setError(null);
    fetch(`${getApiBaseUrl()}/openapi.json`, { headers: getHeaders() })
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((spec) => {
        const items: EndpointItem[] = [];
        const initBodies: Record<string, string> = {};
        Object.entries(spec.paths || {}).forEach(([path, methods]: [string, any]) => {
          Object.entries(methods || {}).forEach(([method, op]: [string, any]) => {
            const key = `${method}:${path}`;
            const schema = op?.requestBody?.content?.['application/json']?.schema;
            if (method.toLowerCase() === 'post') {
              initBodies[key] = schema?.example ? JSON.stringify(schema.example, null, 2) : '{}';
            }
            items.push({
              path,
              method: method.toLowerCase(),
              summary: op?.summary || path,
              description: op?.description || '',
              defaultBody: initBodies[key],
            });
          });
        });
        setEndpoints(items);
        setBodies((prev) => ({ ...initBodies, ...prev }));
      })
      .catch((e) => setError(e?.message || 'Failed to load OpenAPI spec'))
      .finally(() => setLoading(false));
  }, [isOpen]);

  if (!isOpen) return null;

  const filtered = endpoints.filter(
    (ep) =>
      ep.path.toLowerCase().includes(query.toLowerCase()) ||
      ep.summary.toLowerCase().includes(query.toLowerCase()) ||
      ep.method.includes(query.toLowerCase())
  );

  const runTest = async (ep: EndpointItem) => {
    const key = `${ep.method}:${ep.path}`;
    setRunningKey(key);
    try {
      const init: RequestInit = {
        method: ep.method.toUpperCase(),
        headers: getHeaders(ep.method === 'post' ? { 'Content-Type': 'application/json' } : {}),
      };
      if (ep.method === 'post') {
        init.body = bodies[key] || '{}';
      }
      const res = await fetch(`${getApiBaseUrl()}${ep.path}`, init);
      const ct = res.headers.get('content-type') || '';
      let body = '';
      if (ct.includes('audio/') || ct.includes('octet-stream')) {
        const blob = await res.blob();
        body = `Binary payload: ${blob.size} bytes (${ct})`;
      } else {
        const txt = await res.text();
        try {
          body = JSON.stringify(JSON.parse(txt), null, 2);
        } catch {
          body = txt.slice(0, 3000);
        }
      }
      setResponses((prev) => ({
        ...prev,
        [key]: { status: `${res.status} ${res.statusText}`, body },
      }));
    } catch (e: any) {
      setResponses((prev) => ({
        ...prev,
        [key]: { status: 'Request Error', body: e?.message || 'Network failure' },
      }));
    } finally {
      setRunningKey(null);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 bg-black/80 backdrop-blur-sm flex items-center justify-center p-4"
      role="dialog"
      aria-modal="true"
    >
      <div className="bg-[#0D1219] border border-[#242E3D] rounded-xl shadow-2xl w-full max-w-4xl max-h-[86vh] flex flex-col overflow-hidden">
        <div className="px-5 py-3.5 border-b border-[#242E3D] bg-[#0B0E14] flex items-center justify-between gap-3 shrink-0">
          <div className="flex items-center space-x-2.5 min-w-0">
            <Activity className="w-4 h-4 text-amber-400 shrink-0" />
            <div className="min-w-0">
              <h2 className="text-sm font-bold text-white truncate">VoxForge API Reference</h2>
              <p className="text-[11px] font-mono text-[#94A3B8] truncate">{baseUrl}</p>
            </div>
          </div>

          <div className="flex items-center space-x-2 shrink-0">
            <button
              type="button"
              onClick={() => openExternalUrl(`${baseUrl}/docs`)}
              className="inline-flex items-center space-x-1.5 px-2.5 py-1.5 rounded bg-[#1A222D] hover:bg-[#242E3D] border border-[#3B485C] text-xs font-mono text-sky-300"
            >
              <ExternalLink className="w-3.5 h-3.5" />
              <span>Open in Browser</span>
            </button>
            <button
              type="button"
              onClick={onClose}
              className="p-1.5 rounded text-[#94A3B8] hover:text-white hover:bg-[#1A222D]"
              aria-label="Close API Docs"
            >
              <X className="w-4 h-4" />
            </button>
          </div>
        </div>

        <div className="px-5 py-3 border-b border-[#1E2633] bg-[#111722] shrink-0">
          <div className="relative">
            <Search className="w-3.5 h-3.5 text-[#64748B] absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter endpoints (e.g. /v1/voices, /v1/audio/speech, /mcp)..."
              className="w-full bg-[#0B0E14] border border-[#242E3D] rounded-lg pl-8 pr-3 py-1.5 text-xs font-mono text-white placeholder-[#64748B] focus:border-amber-500 focus:outline-none"
            />
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-5 space-y-2.5">
          {loading && (
            <div className="flex items-center justify-center py-10 text-xs font-mono text-[#94A3B8] space-x-2">
              <Loader2 className="w-4 h-4 animate-spin text-amber-400" />
              <span>Loading OpenAPI 3.1 schema...</span>
            </div>
          )}
          {error && (
            <div className="p-3 rounded-lg bg-red-500/10 border border-red-500/30 text-xs font-mono text-red-300">
              {error}
            </div>
          )}
          {!loading &&
            filtered.map((ep) => {
              const key = `${ep.method}:${ep.path}`;
              const open = expandedKey === key;
              const resp = responses[key];
              const badgeCls =
                ep.method === 'get'
                  ? 'bg-emerald-500/15 text-emerald-300 border-emerald-500/35'
                  : ep.method === 'post'
                    ? 'bg-amber-500/15 text-amber-300 border-amber-500/35'
                    : 'bg-rose-500/15 text-rose-300 border-rose-500/35';
              return (
                <div key={key} className="rounded-lg bg-[#121820] border border-[#242E3D] overflow-hidden">
                  <button
                    type="button"
                    onClick={() => setExpandedKey(open ? null : key)}
                    className="w-full px-4 py-2.5 flex items-center justify-between gap-3 text-left hover:bg-[#1A222D]/60 transition-colors"
                  >
                    <div className="flex items-center space-x-3 min-w-0">
                      <span className={`text-[10px] font-mono font-bold uppercase px-2 py-0.5 rounded border shrink-0 ${badgeCls}`}>
                        {ep.method}
                      </span>
                      <span className="text-xs font-mono font-semibold text-white truncate">{ep.path}</span>
                    </div>
                    <span className="text-xs text-[#94A3B8] truncate">{ep.summary}</span>
                  </button>

                  {open && (
                    <div className="px-4 py-3 border-t border-[#1E2633] bg-[#0B0E14] space-y-2.5 text-xs">
                      <p className="text-[#CBD5E1]">{ep.description}</p>
                      {ep.method === 'post' && (
                        <textarea
                          rows={4}
                          value={bodies[key] ?? '{}'}
                          onChange={(e) => setBodies((prev) => ({ ...prev, [key]: e.target.value }))}
                          className="w-full bg-[#080B10] border border-[#242E3D] rounded p-2 text-xs font-mono text-white focus:border-amber-500 focus:outline-none"
                        />
                      )}
                      <div className="flex items-center space-x-3">
                        <button
                          type="button"
                          onClick={() => runTest(ep)}
                          disabled={runningKey === key}
                          className="inline-flex items-center space-x-1.5 px-3 py-1 rounded bg-amber-500 hover:bg-amber-400 text-[#0B0E14] font-semibold text-xs disabled:opacity-50"
                        >
                          {runningKey === key ? <Loader2 className="w-3 h-3 animate-spin" /> : <Play className="w-3 h-3 fill-current" />}
                          <span>Try Request</span>
                        </button>
                        {resp && <span className="font-mono text-[11px] text-emerald-300">{resp.status}</span>}
                      </div>
                      {resp && (
                        <pre className="p-2.5 rounded bg-[#080B10] border border-[#242E3D] text-[11px] font-mono text-[#CBD5E1] max-h-48 overflow-auto">
                          {resp.body}
                        </pre>
                      )}
                    </div>
                  )}
                </div>
              );
            })}
        </div>
      </div>
    </div>
  );
};
