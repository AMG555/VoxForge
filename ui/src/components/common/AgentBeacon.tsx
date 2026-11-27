import React, { useState, useEffect } from 'react';
import { Bot, Radio, CheckCircle, ExternalLink, X, Copy, Check } from 'lucide-react';
import { getApiBaseUrl } from '../../services/api';

interface AgentBeaconProps {
  serverUrl?: string;
}

type McpSnippetMode = 'cli' | 'npx' | 'http';

export const AgentBeacon: React.FC<AgentBeaconProps> = ({ serverUrl = '' }) => {
  const [isSpeaking, setIsSpeaking] = useState<boolean>(false);
  const [activeClient, setActiveClient] = useState<string>('Claude Desktop');
  const [showFlyout, setShowFlyout] = useState<boolean>(false);
  const [mcpHealth, setMcpHealth] = useState<'connected' | 'idle' | 'offline'>('idle');
  const [snippetMode, setSnippetMode] = useState<McpSnippetMode>('npx');
  const [copiedConfig, setCopiedConfig] = useState<boolean>(false);

  const effectiveBase = serverUrl || getApiBaseUrl();
  const origin =
    effectiveBase ||
    (typeof window !== 'undefined' ? window.location.origin : 'http://127.0.0.1:8080');

  const configSnippets: Record<McpSnippetMode, string> = {
    npx: JSON.stringify(
      {
        mcpServers: {
          voxforge: {
            command: 'npx',
            args: ['-y', 'github:AMG555/VoxForge', 'mcp'],
          },
        },
      },
      null,
      2
    ),
    cli: JSON.stringify(
      {
        mcpServers: {
          voxforge: {
            command: 'voxforge',
            args: ['mcp'],
          },
        },
      },
      null,
      2
    ),
    http: JSON.stringify(
      {
        mcpServers: {
          voxforge: {
            url: `${origin}/mcp`,
          },
        },
      },
      null,
      2
    ),
  };

  const handleCopyConfig = () => {
    navigator.clipboard.writeText(configSnippets[snippetMode]).catch(() => {});
    setCopiedConfig(true);
    setTimeout(() => setCopiedConfig(false), 1800);
  };

  useEffect(() => {
    const handleAgentStart = (e: any) => {
      setIsSpeaking(true);
      if (e.detail?.client) setActiveClient(e.detail.client);
    };

    const handleAgentEnd = () => {
      setIsSpeaking(false);
    };

    window.addEventListener('voxforge:agent-start', handleAgentStart);
    window.addEventListener('voxforge:agent-end', handleAgentEnd);

    const endpoint = effectiveBase ? `${effectiveBase}/mcp` : '/mcp';
    const token = typeof window !== 'undefined' ? localStorage.getItem('voxforge_api_key') : null;
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    if (token) headers['Authorization'] = `Bearer ${token}`;

    fetch(endpoint, {
      method: 'POST',
      headers,
      body: JSON.stringify({ jsonrpc: '2.0', method: 'ping', id: 1 }),
    })
      .then((res) => {
        if (res.ok) setMcpHealth('connected');
        else setMcpHealth('idle');
      })
      .catch(() => setMcpHealth('idle'));

    return () => {
      window.removeEventListener('voxforge:agent-start', handleAgentStart);
      window.removeEventListener('voxforge:agent-end', handleAgentEnd);
    };
  }, [serverUrl]);

  return (
    <div className="relative">
      <button
        type="button"
        onClick={() => setShowFlyout(!showFlyout)}
        className={`flex items-center space-x-2 px-2.5 py-1 rounded-full text-xs font-mono transition-all duration-300 border ${
          isSpeaking
            ? 'bg-amber-500/20 border-amber-500/50 text-amber-300 shadow-[0_0_12px_rgba(245,158,11,0.3)] animate-pulse'
            : 'bg-[#1A222D]/60 hover:bg-[#1A222D] border-[#242E3D] text-[#94A3B8] hover:text-white'
        }`}
        title="Model Context Protocol (MCP) Streamable Agent Bridge (17 Tools)"
      >
        <span className="relative flex h-2 w-2">
          {isSpeaking && (
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-amber-400 opacity-75" />
          )}
          <span
            className={`relative inline-flex rounded-full h-2 w-2 ${
              isSpeaking
                ? 'bg-amber-400'
                : mcpHealth === 'connected'
                ? 'bg-emerald-400'
                : 'bg-[#64748B]'
            }`}
          />
        </span>

        <Bot className={`w-3.5 h-3.5 ${isSpeaking ? 'text-amber-400' : 'text-[#94A3B8]'}`} />

        <span className="text-[11px] font-medium">
          {isSpeaking ? (
            <span className="flex items-center space-x-1.5">
              <span>{activeClient}: Speaking</span>
              <span className="inline-flex space-x-0.5 items-end h-2.5">
                <span className="w-0.5 h-1.5 bg-amber-400 animate-pulse" />
                <span className="w-0.5 h-2.5 bg-amber-400 animate-bounce" />
                <span className="w-0.5 h-2 bg-amber-400 animate-pulse" />
              </span>
            </span>
          ) : (
            <span>MCP Bridge</span>
          )}
        </span>
      </button>

      {showFlyout && (
        <div className="absolute right-0 mt-2 w-96 bg-[#121820] border border-[#242E3D] rounded-xl shadow-2xl p-4 z-50 text-white">
          <div className="flex items-center justify-between pb-3 border-b border-[#242E3D]">
            <div className="flex items-center space-x-2">
              <Radio className="w-4 h-4 text-amber-400" />
              <div>
                <span className="text-xs font-bold tracking-wide uppercase block">MCP Agent Bridge</span>
                <span className="text-[10px] font-mono text-emerald-400">
                  17 Tools · 4 Resources · 2 Prompts
                </span>
              </div>
            </div>
            <button
              type="button"
              onClick={() => setShowFlyout(false)}
              className="text-[#94A3B8] hover:text-white p-1 rounded hover:bg-[#1A222D]"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>

          <div className="mt-3 space-y-3 text-xs">
            <div className="bg-[#0B0E14] p-2.5 rounded-lg border border-[#242E3D]">
              <div className="text-[10px] text-[#94A3B8] font-mono uppercase mb-1">Streamable HTTP / SSE Endpoint</div>
              <div className="font-mono text-amber-300 text-[11px] select-all flex items-center justify-between">
                <span className="truncate mr-2">{`${origin}/mcp`}</span>
                <span className="text-[9px] bg-emerald-500/20 text-emerald-400 px-1.5 py-0.5 rounded border border-emerald-500/30 shrink-0">
                  HTTP + SSE
                </span>
              </div>
            </div>

            <div className="bg-[#0B0E14] p-2.5 rounded-lg border border-[#242E3D] space-y-2">
              <div className="flex items-center justify-between">
                <span className="text-[10px] text-[#94A3B8] font-mono uppercase">One-Click Client Config</span>
                <button
                  type="button"
                  onClick={handleCopyConfig}
                  className="inline-flex items-center space-x-1 text-[10px] font-mono text-amber-400 hover:text-amber-300"
                >
                  {copiedConfig ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
                  <span>{copiedConfig ? 'Copied JSON' : 'Copy JSON'}</span>
                </button>
              </div>

              <div className="grid grid-cols-3 gap-1">
                {[
                  { id: 'npx', label: 'Zero-Install' },
                  { id: 'cli', label: 'Local CLI' },
                  { id: 'http', label: 'HTTP URL' },
                ].map((opt) => (
                  <button
                    key={opt.id}
                    type="button"
                    onClick={() => setSnippetMode(opt.id as McpSnippetMode)}
                    className={`py-1 rounded text-[10px] font-mono border transition-colors ${
                      snippetMode === opt.id
                        ? 'bg-amber-500/15 border-amber-500/50 text-amber-300 font-semibold'
                        : 'bg-[#121820] border-[#242E3D] text-[#94A3B8] hover:text-white'
                    }`}
                  >
                    {opt.label}
                  </button>
                ))}
              </div>

              <pre className="text-[10px] font-mono text-sky-300 bg-[#121820] p-2 rounded border border-[#242E3D] overflow-x-auto select-all">
                {configSnippets[snippetMode]}
              </pre>
            </div>

            <div className="space-y-1 font-mono text-[11px]">
              <div className="flex items-center justify-between text-[#94A3B8]">
                <span>Transport:</span>
                <span className="text-white flex items-center space-x-1">
                  <CheckCircle className="w-3 h-3 text-emerald-400 inline" />
                  <span>JSON-RPC 2.0 (stdio / HTTP)</span>
                </span>
              </div>
              <div className="flex items-center justify-between text-[#94A3B8]">
                <span>Clients:</span>
                <span className="text-[#E2E8F0]">Cursor, Claude, Antigravity, Windsurf</span>
              </div>
            </div>

            <div className="pt-2 border-t border-[#242E3D] flex items-center justify-between">
              <button
                type="button"
                onClick={() => {
                  setIsSpeaking(!isSpeaking);
                  if (!isSpeaking) setActiveClient('Cursor IDE');
                }}
                className="text-[11px] px-2.5 py-1 rounded bg-[#1A222D] hover:bg-[#242E3D] text-[#94A3B8] hover:text-white transition-colors"
              >
                {isSpeaking ? 'Stop Test' : 'Test Speech Beacon'}
              </button>

              <a
                href={serverUrl ? `${serverUrl}/docs` : '/docs'}
                target="_blank"
                rel="noreferrer"
                className="text-[11px] text-amber-400 hover:text-amber-300 flex items-center space-x-1"
              >
                <span>API Docs</span>
                <ExternalLink className="w-3 h-3" />
              </a>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
