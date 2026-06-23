import React, { useState, useEffect, useCallback, useMemo } from 'react';
import { 
  Users, 
  RefreshCw, 
  Trash2, 
  Clock, 
  Shield, 
  Activity, 
  User, 
  Copy, 
  Check, 
  Terminal, 
  Info, 
  ExternalLink,
  BookOpen,
  Cpu,
  Link2,
  Plug
} from 'lucide-react';
import { useNexus } from '../hooks/useNexus';
import type { McpSession } from '../lib/nexus-bridge';
import { cn, relativeTime } from '../lib/utils';

interface CapabilitiesData {
  status: string;
  version: string;
  sovereign_contract: {
    tools: Array<{
      name: string;
      description: string;
      inputSchema?: any;
    }>;
  };
  guilds: Array<{
    name: string;
    running: boolean;
    always_on: boolean;
    tools_count: number;
    launcher_type: string;
  }>;
  all_guild_tools: Array<{
    name: string;
    description: string;
    inputSchema?: any;
  }>;
  mcp: {
    prompts: Array<{ name: string; description: string }>;
    resources: Array<{ uri: string; name: string; description: string; mimeType: string }>;
  };
  sessions: McpSession[];
}

export function ConnectorsTab({ notify }: { notify: (msg: string, type?: 'info' | 'error') => void }) {
  const { bridge, sessions: globalSessions, loading: globalLoading, refreshData } = useNexus();
  const [capabilities, setCapabilities] = useState<CapabilitiesData | null>(null);
  const [localLoading, setLocalLoading] = useState(false);
  const [activeSubTab, setActiveSubTab] = useState<'sessions' | 'catalog' | 'guides'>('sessions');
  const [guideTab, setGuideTab] = useState<'cursor' | 'vscode' | 'claude' | 'custom'>('cursor');
  const [revokingIds, setRevokingIds] = useState<Set<string>>(new Set());
  const [copiedText, setCopiedText] = useState<string | null>(null);

  const fetchCapabilities = useCallback(async () => {
    if (!bridge) return;
    setLocalLoading(true);
    try {
      const data = await bridge.getCapabilities();
      setCapabilities(data);
    } catch (e) {
      console.error('Failed to fetch capabilities:', e);
      notify('Failed to load server capabilities', 'error');
    } finally {
      setLocalLoading(false);
    }
  }, [bridge, notify]);

  useEffect(() => {
    fetchCapabilities();
  }, [fetchCapabilities]);

  const handleRefresh = async () => {
    await Promise.all([
      refreshData(),
      fetchCapabilities()
    ]);
    notify('Connectors status updated', 'info');
  };

  const handleRevoke = async (id: string) => {
    if (!bridge) return;
    setRevokingIds(prev => {
      const next = new Set(prev);
      next.add(id);
      return next;
    });

    try {
      await bridge.revokeSession(id);
      notify(`Session ${id.slice(0, 8)} revoked`, 'info');
      // Refresh local view immediately
      fetchCapabilities();
    } catch (e) {
      notify(`Failed to revoke session: ${e instanceof Error ? e.message : 'Unknown error'}`, 'error');
    } finally {
      setRevokingIds(prev => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  };

  const handleCopy = (text: string, label: string) => {
    navigator.clipboard.writeText(text);
    setCopiedText(label);
    notify(`${label} copied to clipboard`, 'info');
    setTimeout(() => setCopiedText(null), 2000);
  };

  const cursorConfig = `{
  "mcpServers": {
    "tylluan": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/client-cli"],
      "env": {
        "TYLLUAN_URL": "http://localhost:3030/api/v1/mcp",
        "TYLLUAN_TOKEN": "TU_TOKEN_AQUI"
      }
    }
  }
}`;

  const clineConfig = `{
  "mcpServers": {
    "tylluan": {
      "command": "node",
      "args": ["C:/Users/YOUR_USER/.gemini/tylluan/mcp/tylluan_mcp_client.js"],
      "disabled": false,
      "alwaysOn": true
    }
  }
}`;

  const claudeConfig = `{
  "mcpServers": {
    "tylluan": {
      "command": "npx",
      "args": [
        "-y",
        "@modelcontextprotocol/client-cli"
      ],
      "env": {
        "TYLLUAN_URL": "http://127.0.0.1:3030/mcp",
        "TYLLUAN_TOKEN": "TU_TOKEN_AQUI"
      }
    }
  }
}`;

  const restCurl = `curl -X POST http://localhost:3030/api/v1/do \\
  -H "Authorization: Bearer TU_TOKEN_AQUI" \\
  -H "Content-Type: application/json" \\
  -d '{
    "intent": "Read the contents of README.md and summarize it",
    "agent_id": "curl-client"
  }'`;

  // Merge local capabilities sessions list with global hook sessions list
  const activeSessions = useMemo(() => {
    if (capabilities?.sessions) {
      return capabilities.sessions;
    }
    return globalSessions || [];
  }, [capabilities, globalSessions]);

  return (
    <div className="space-y-6 p-6">
      {/* Header Banner */}
      <div className="bg-gradient-to-r from-emerald-900/30 via-slate-900/80 to-slate-950/80 p-6 rounded-3xl border border-slate-800 flex flex-col md:flex-row md:items-center justify-between gap-6 relative overflow-hidden shadow-xl">
        <div className="absolute top-0 right-0 w-64 h-64 bg-emerald-500/5 rounded-full blur-3xl pointer-events-none" />
        <div className="space-y-2">
          <div className="flex items-center gap-3">
            <div className="p-2.5 bg-gradient-to-br from-emerald-400 to-emerald-600 rounded-2xl shadow-lg shadow-emerald-500/10 flex items-center justify-center">
              <Link2 className="w-5 h-5 text-slate-950" />
            </div>
            <div>
              <h1 className="text-2xl font-black text-white tracking-tight">Connectors & Discovery</h1>
              <p className="text-xs text-emerald-400/80 font-mono tracking-wider uppercase">Milestone M24 Sovereign Integrations</p>
            </div>
          </div>
          <p className="text-sm text-slate-400 max-w-xl">
            Tylluan acts as a Sovereign MCP Hub. External client LLMs connect via standard MCP or REST APIs.
            The client LLM discovers capabilities dynamically via Prompts & Resources, keeping contexts light.
          </p>
        </div>

        <div className="flex items-center gap-3 shrink-0 self-end md:self-center">
          <button 
            onClick={handleRefresh}
            disabled={globalLoading || localLoading}
            className="flex items-center gap-2 px-4 py-2.5 bg-slate-900 border border-slate-800 rounded-xl hover:bg-slate-800 hover:border-slate-700 text-xs font-bold text-slate-300 transition-all disabled:opacity-50"
          >
            <RefreshCw className={cn("w-4 h-4 text-emerald-400", (globalLoading || localLoading) && "animate-spin")} />
            <span>Reload Status</span>
          </button>
        </div>
      </div>

      {/* Tabs Selector */}
      <div className="flex border-b border-slate-800 gap-6">
        <button 
          onClick={() => setActiveSubTab('sessions')}
          className={cn(
            "pb-3 text-sm font-bold tracking-tight transition-all border-b-2 relative -mb-[2px]",
            activeSubTab === 'sessions' 
              ? "border-emerald-500 text-white" 
              : "border-transparent text-slate-500 hover:text-slate-300"
          )}
        >
          <div className="flex items-center gap-2">
            <Users className="w-4 h-4" />
            <span>Active Client Sessions</span>
            {activeSessions.length > 0 && (
              <span className="px-1.5 py-0.5 rounded-full bg-emerald-500/15 text-[10px] font-mono text-emerald-400 border border-emerald-500/20">
                {activeSessions.length}
              </span>
            )}
          </div>
        </button>

        <button 
          onClick={() => setActiveSubTab('catalog')}
          className={cn(
            "pb-3 text-sm font-bold tracking-tight transition-all border-b-2 relative -mb-[2px]",
            activeSubTab === 'catalog' 
              ? "border-emerald-500 text-white" 
              : "border-transparent text-slate-500 hover:text-slate-300"
          )}
        >
          <div className="flex items-center gap-2">
            <Cpu className="w-4 h-4" />
            <span>Capabilities Catalog</span>
            {capabilities && (
              <span className="px-1.5 py-0.5 rounded-full bg-blue-500/15 text-[10px] font-mono text-blue-400 border border-blue-500/20">
                {capabilities.guilds.length} Guilds
              </span>
            )}
          </div>
        </button>

        <button 
          onClick={() => setActiveSubTab('guides')}
          className={cn(
            "pb-3 text-sm font-bold tracking-tight transition-all border-b-2 relative -mb-[2px]",
            activeSubTab === 'guides' 
              ? "border-emerald-500 text-white" 
              : "border-transparent text-slate-500 hover:text-slate-300"
          )}
        >
          <div className="flex items-center gap-2">
            <BookOpen className="w-4 h-4" />
            <span>Client Setup Guides</span>
          </div>
        </button>
      </div>

      {/* Tab Panels */}
      <div className="space-y-6">
        {activeSubTab === 'sessions' && (
          <div className="space-y-4">
            <div className="bg-slate-900/40 rounded-2xl border border-slate-800/80 overflow-hidden shadow-lg backdrop-blur-sm">
              <table className="w-full text-left border-collapse">
                <thead>
                  <tr className="bg-slate-800/30 border-b border-slate-800 text-[10px] font-bold text-slate-500 uppercase tracking-widest">
                    <th className="px-6 py-4">Client / Process</th>
                    <th className="px-6 py-4">Agent Identification</th>
                    <th className="px-6 py-4 text-center">Tools Called</th>
                    <th className="px-6 py-4">Active Guild</th>
                    <th className="px-6 py-4">Last Activity</th>
                    <th className="px-6 py-4 text-right">Revoke Access</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-slate-800/60">
                  {activeSessions.map((session) => (
                    <tr key={session.id} className="group hover:bg-slate-850/20 transition-all duration-150">
                      <td className="px-6 py-4">
                        <div className="flex items-center gap-3">
                          <div className="w-9 h-9 rounded-xl bg-slate-800/50 flex items-center justify-center border border-slate-700/50 text-slate-400 group-hover:text-emerald-400 transition-colors">
                            <User className="w-4.5 h-4.5" />
                          </div>
                          <div>
                            <div className="text-sm font-bold text-slate-200">{session.client_name}</div>
                            <div className="text-[10px] font-mono text-slate-500">ID: {session.id.slice(0, 16)}...</div>
                          </div>
                        </div>
                      </td>
                      <td className="px-6 py-4">
                        {session.agent_id ? (
                          <div className="flex items-center gap-2">
                            <Shield className="w-3.5 h-3.5 text-emerald-400/80" />
                            <span className="text-xs font-mono text-emerald-400 bg-emerald-500/10 px-2 py-0.5 rounded border border-emerald-500/20 truncate max-w-[180px]">
                              {session.agent_id}
                            </span>
                          </div>
                        ) : (
                          <span className="text-xs text-slate-600 italic">Unidentified agent</span>
                        )}
                      </td>
                      <td className="px-6 py-4 text-center">
                        <span className="px-2 py-1 rounded-lg bg-slate-900 border border-slate-800 text-xs font-mono text-slate-300">
                          {session.tool_count}
                        </span>
                      </td>
                      <td className="px-6 py-4">
                        {session.last_guild ? (
                          <div className="flex items-center gap-2">
                            <Activity className="w-3.5 h-3.5 text-blue-400/80" />
                            <span className="text-xs font-mono text-blue-400 bg-blue-500/10 px-2.5 py-0.5 rounded border border-blue-500/20">
                              {session.last_guild}
                            </span>
                          </div>
                        ) : (
                          <span className="text-xs text-slate-700 font-mono">—</span>
                        )}
                      </td>
                      <td className="px-6 py-4">
                        <div className="flex items-center gap-2 text-xs text-slate-400 font-mono">
                          <Clock className="w-3.5 h-3.5 text-slate-600" />
                          {relativeTime(session.last_active_unix)}
                        </div>
                      </td>
                      <td className="px-6 py-4 text-right">
                        <button
                          onClick={() => handleRevoke(session.id)}
                          disabled={revokingIds.has(session.id)}
                          className={cn(
                            "p-2 rounded-xl transition-all border",
                            revokingIds.has(session.id) 
                              ? "bg-slate-800 border-slate-700 cursor-not-allowed text-slate-500" 
                              : "bg-red-500/10 hover:bg-red-500/25 text-red-400 border-red-500/20 hover:border-red-500/40"
                          )}
                          title="Revoke client token & disconnect"
                        >
                          {revokingIds.has(session.id) ? (
                            <RefreshCw className="w-4 h-4 animate-spin" />
                          ) : (
                            <Trash2 className="w-4 h-4" />
                          )}
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>

              {activeSessions.length === 0 && (
                <div className="py-20 flex flex-col items-center justify-center text-slate-600 bg-slate-950/20">
                  <Plug className="w-10 h-10 mb-3 opacity-25 text-emerald-400" />
                  <p className="text-sm font-semibold italic text-slate-400">No active client sessions</p>
                  <p className="text-xs mt-1 text-slate-500 max-w-xs text-center">
                    Connect an external client like Cursor or Roo Code to start using the Sovereign microkernel.
                  </p>
                </div>
              )}
            </div>
          </div>
        )}

        {activeSubTab === 'catalog' && (
          <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
            {/* Left Col: Sovereign Contract */}
            <div className="space-y-4">
              <div className="p-5 bg-gradient-to-br from-emerald-950/25 to-slate-900/50 border border-emerald-500/20 rounded-2xl shadow-md">
                <div className="flex items-center gap-2 mb-3">
                  <Shield className="w-4.5 h-4.5 text-emerald-400" />
                  <h3 className="text-sm font-black text-white tracking-wide uppercase">Sovereign Contract (5 Tools)</h3>
                </div>
                <p className="text-xs text-slate-400 leading-relaxed mb-4">
                  CONTRACT-01 guarantees that clients always see exactly these 5 tools. Any guild work is requested through them, preventing context overload.
                </p>

                <div className="space-y-2.5">
                  {capabilities?.sovereign_contract.tools.map((t) => (
                    <div key={t.name} className="p-3 bg-slate-950/60 border border-slate-850 rounded-xl space-y-1">
                      <div className="text-xs font-mono font-bold text-emerald-400">{t.name}</div>
                      <div className="text-[11px] text-slate-400 leading-normal">{t.description}</div>
                    </div>
                  )) ?? (
                    <div className="text-xs text-slate-500 italic">No contract tools loaded</div>
                  )}
                </div>
              </div>

              {/* MCP Handshake Catalog */}
              <div className="p-5 bg-slate-900/30 border border-slate-800 rounded-2xl space-y-4">
                <div className="flex items-center gap-2">
                  <Info className="w-4 h-4 text-emerald-400" />
                  <h3 className="text-xs font-black text-white uppercase tracking-wider">MCP Discovery Registry</h3>
                </div>
                <div className="space-y-3">
                  <div>
                    <div className="text-[11px] font-bold text-slate-400 uppercase tracking-wider mb-1">Prompts Catalog</div>
                    {capabilities?.mcp.prompts.map((p) => (
                      <div key={p.name} className="p-2.5 bg-slate-950/40 border border-slate-900 rounded-lg space-y-1">
                        <div className="text-xs font-mono text-blue-400">{p.name}</div>
                        <div className="text-[10px] text-slate-400">{p.description}</div>
                      </div>
                    ))}
                  </div>
                  <div>
                    <div className="text-[11px] font-bold text-slate-400 uppercase tracking-wider mb-1">Resources Catalog</div>
                    {capabilities?.mcp.resources.map((r) => (
                      <div key={r.uri} className="p-2.5 bg-slate-950/40 border border-slate-900 rounded-lg space-y-1">
                        <div className="text-xs font-mono text-purple-400 truncate">{r.uri}</div>
                        <div className="text-[10px] text-slate-300 font-medium">{r.name}</div>
                        <div className="text-[9px] text-slate-500">{r.description}</div>
                      </div>
                    ))}
                  </div>
                </div>
              </div>
            </div>

            {/* Right Col: Active Guilds and schemas */}
            <div className="lg:col-span-2 space-y-4">
              <div className="p-5 bg-slate-900/30 border border-slate-800 rounded-2xl">
                <div className="flex items-center justify-between mb-4">
                  <div className="flex items-center gap-2">
                    <Cpu className="w-4.5 h-4.5 text-blue-400" />
                    <h3 className="text-sm font-black text-white tracking-wide uppercase">Active Guild Modules</h3>
                  </div>
                  <span className="text-xs text-slate-500 font-mono">Total Underlying Tools: {capabilities?.all_guild_tools.length ?? 0}</span>
                </div>

                <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                  {capabilities?.guilds.map((g) => (
                    <div 
                      key={g.name} 
                      className={cn(
                        "p-4 rounded-xl border transition-all flex flex-col justify-between gap-3 bg-slate-950/40",
                        g.running 
                          ? "border-emerald-500/20 shadow-emerald-950/5" 
                          : "border-slate-800"
                      )}
                    >
                      <div className="flex justify-between items-start">
                        <div>
                          <div className="flex items-center gap-2">
                            <span className="font-bold text-sm text-slate-200">{g.name}</span>
                            <span className={cn(
                              "w-1.5 h-1.5 rounded-full",
                              g.running ? "bg-emerald-400 animate-pulse" : "bg-slate-600"
                            )} />
                          </div>
                          <span className="text-[10px] font-mono text-slate-500 uppercase">{g.launcher_type} launcher</span>
                        </div>
                        <span className={cn(
                          "px-2 py-0.5 rounded text-[10px] font-bold font-mono border",
                          g.running 
                            ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/20" 
                            : "bg-slate-800 text-slate-500 border-slate-700"
                        )}>
                          {g.running ? 'Running' : 'On-Demand'}
                        </span>
                      </div>

                      <div className="flex justify-between items-center text-xs text-slate-400">
                        <span>Tools exposed:</span>
                        <span className="font-mono font-bold text-slate-200 bg-slate-850 px-2 py-0.5 rounded border border-slate-850">
                          {g.tools_count}
                        </span>
                      </div>
                    </div>
                  )) ?? (
                    <div className="col-span-2 py-10 text-center text-xs text-slate-500 italic">No guild modules loaded</div>
                  )}
                </div>

                {/* Guild tools index */}
                <div className="mt-6 space-y-3">
                  <div className="text-xs font-bold text-slate-400 uppercase tracking-widest border-b border-slate-850 pb-2">Exposed Skill Schemas</div>
                  <div className="max-h-[350px] overflow-y-auto space-y-2.5 pr-2 custom-scrollbar">
                    {capabilities?.all_guild_tools.map((t) => (
                      <div key={t.name} className="p-3 bg-slate-950/60 border border-slate-850 rounded-xl hover:border-slate-800 transition-colors">
                        <div className="flex justify-between items-start mb-1">
                          <span className="text-xs font-mono font-bold text-slate-200">{t.name}</span>
                        </div>
                        <p className="text-[11px] text-slate-400 leading-normal">{t.description}</p>
                      </div>
                    ))}
                  </div>
                </div>

              </div>
            </div>
          </div>
        )}

        {activeSubTab === 'guides' && (
          <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
            {/* Guide Tabs */}
            <div className="space-y-2">
              <button 
                onClick={() => setGuideTab('cursor')}
                className={cn(
                  "w-full text-left p-4 rounded-xl border transition-all font-bold text-xs uppercase tracking-wide flex items-center justify-between",
                  guideTab === 'cursor' 
                    ? "bg-gradient-to-r from-slate-900 to-emerald-950/20 border-emerald-500/30 text-white shadow-md" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>Cursor Setup</span>
                <ExternalLink className="w-3.5 h-3.5 opacity-50" />
              </button>

              <button 
                onClick={() => setGuideTab('vscode')}
                className={cn(
                  "w-full text-left p-4 rounded-xl border transition-all font-bold text-xs uppercase tracking-wide flex items-center justify-between",
                  guideTab === 'vscode' 
                    ? "bg-gradient-to-r from-slate-900 to-emerald-950/20 border-emerald-500/30 text-white shadow-md" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>VS Code (Cline / Roo)</span>
                <ExternalLink className="w-3.5 h-3.5 opacity-50" />
              </button>

              <button 
                onClick={() => setGuideTab('claude')}
                className={cn(
                  "w-full text-left p-4 rounded-xl border transition-all font-bold text-xs uppercase tracking-wide flex items-center justify-between",
                  guideTab === 'claude' 
                    ? "bg-gradient-to-r from-slate-900 to-emerald-950/20 border-emerald-500/30 text-white shadow-md" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>Claude Desktop</span>
                <ExternalLink className="w-3.5 h-3.5 opacity-50" />
              </button>

              <button 
                onClick={() => setGuideTab('custom')}
                className={cn(
                  "w-full text-left p-4 rounded-xl border transition-all font-bold text-xs uppercase tracking-wide flex items-center justify-between",
                  guideTab === 'custom' 
                    ? "bg-gradient-to-r from-slate-900 to-emerald-950/20 border-emerald-500/30 text-white shadow-md" 
                    : "bg-slate-900/30 border-slate-800/80 text-slate-400 hover:text-slate-200 hover:bg-slate-850/30"
                )}
              >
                <span>Custom API Client</span>
                <Terminal className="w-3.5 h-3.5 opacity-50" />
              </button>
            </div>

            {/* Guide Content */}
            <div className="lg:col-span-3 bg-slate-900/30 border border-slate-800 rounded-2xl p-6 space-y-6">
              {guideTab === 'cursor' && (
                <div className="space-y-4">
                  <div className="space-y-1">
                    <h3 className="text-base font-bold text-white tracking-tight">Cursor MCP Integration</h3>
                    <p className="text-xs text-slate-400">Configure Cursor to connect to Tylluan's sovereign hub using standard MCP Stdio Client.</p>
                  </div>

                  <div className="p-4 rounded-xl bg-amber-500/5 border border-amber-500/10 flex gap-3 text-xs text-slate-400 leading-normal">
                    <Info className="w-4 h-4 text-amber-500/80 shrink-0 mt-0.5" />
                    <div>
                      <span className="font-bold text-amber-400">Security Key:</span> Bearer authentication is managed by setting the <code>TYLLUAN_TOKEN</code> environment variable. The default token is <code>TU_TOKEN_AQUI</code>.
                    </div>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Add this entry inside <code>project.json</code> or global Cursor config:</span>
                      <button 
                        onClick={() => handleCopy(cursorConfig, 'Cursor Config')}
                        className="flex items-center gap-1 hover:text-white transition-colors"
                      >
                        {copiedText === 'Cursor Config' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copy Code</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded-xl bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {cursorConfig}
                    </pre>
                  </div>

                  <div className="text-xs text-slate-500 leading-relaxed">
                    <span className="font-bold text-slate-400">How Auto-Discovery works:</span> Upon initialization, the Cursor agent automatically fetches the Prompts/Resources catalog from Tylluan. The agent will read <code>tylluan://metadata/guilds</code> and load it directly into its prompt context, allowing it to know exactly which commands to call inside <code>tylluan_do</code>.
                  </div>
                </div>
              )}

              {guideTab === 'vscode' && (
                <div className="space-y-4">
                  <div className="space-y-1">
                    <h3 className="text-base font-bold text-white tracking-tight">VS Code (Cline / Roo Code) Configuration</h3>
                    <p className="text-xs text-slate-400">Add Tylluan as a custom MCP server inside Cline or Roo Code extension settings.</p>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Add this configuration to your <code>cline_mcp_settings.json</code>:</span>
                      <button 
                        onClick={() => handleCopy(clineConfig, 'VS Code Config')}
                        className="flex items-center gap-1 hover:text-white transition-colors"
                      >
                        {copiedText === 'VS Code Config' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copy Code</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded-xl bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {clineConfig}
                    </pre>
                  </div>

                  <div className="text-xs text-slate-500 leading-relaxed space-y-2">
                    <p>
                      The client bridge file is located at <code>C:/Users/YOUR_USER/.gemini/tylluan/mcp/tylluan_mcp_client.js</code> and orchestrates stdio transport mapping to Tylluan's HTTP REST endpoints.
                    </p>
                    <p>
                      <span className="font-bold text-slate-400">HANDSHAKE CONTRACT:</span> The Cline agent will see exactly 5 sovereign tools. It will call <code>tylluan_recall</code> or <code>tylluan_graph</code> for memory indexing, and route all filesystem, git, and bash intents dynamically via <code>tylluan_do</code>.
                    </p>
                  </div>
                </div>
              )}

              {guideTab === 'claude' && (
                <div className="space-y-4">
                  <div className="space-y-1">
                    <h3 className="text-base font-bold text-white tracking-tight">Claude Desktop Integration</h3>
                    <p className="text-xs text-slate-400">Integrate Tylluan into the official Anthropic Claude Desktop client.</p>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Add this server entry inside your <code>claude_desktop_config.json</code>:</span>
                      <button 
                        onClick={() => handleCopy(claudeConfig, 'Claude Desktop Config')}
                        className="flex items-center gap-1 hover:text-white transition-colors"
                      >
                        {copiedText === 'Claude Desktop Config' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copy Code</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded-xl bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {claudeConfig}
                    </pre>
                  </div>
                </div>
              )}

              {guideTab === 'custom' && (
                <div className="space-y-4">
                  <div className="space-y-1">
                    <h3 className="text-base font-bold text-white tracking-tight">Custom REST Client / Script Integration</h3>
                    <p className="text-xs text-slate-400">Call Tylluan directly via HTTP REST API from custom shell scripts, Python programs, or fetch calls.</p>
                  </div>

                  <div className="space-y-2">
                    <div className="flex justify-between items-center text-xs text-slate-400">
                      <span>Example cURL request to execute an intent:</span>
                      <button 
                        onClick={() => handleCopy(restCurl, 'cURL command')}
                        className="flex items-center gap-1 hover:text-white transition-colors"
                      >
                        {copiedText === 'cURL command' ? <Check className="w-3.5 h-3.5 text-emerald-400" /> : <Copy className="w-3.5 h-3.5" />}
                        <span>Copy Code</span>
                      </button>
                    </div>
                    <pre className="p-4 rounded-xl bg-slate-950 border border-slate-850 text-xs font-mono text-slate-300 overflow-x-auto">
                      {restCurl}
                    </pre>
                  </div>

                  <div className="p-4 rounded-xl bg-slate-950 border border-slate-850 space-y-2.5">
                    <div className="text-xs font-bold text-slate-400 uppercase tracking-widest">HTTP Headers Required:</div>
                    <div className="grid grid-cols-3 gap-2 text-xs font-mono">
                      <div className="text-slate-500 font-bold">Authorization</div>
                      <div className="col-span-2 text-emerald-400">Bearer TU_TOKEN_AQUI</div>

                      <div className="text-slate-500 font-bold">Content-Type</div>
                      <div className="col-span-2 text-slate-300">application/json</div>
                    </div>
                  </div>
                </div>
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
