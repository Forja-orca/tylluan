import React, { useState, useEffect } from 'react';
import { 
  Plug, 
  Plus, 
  Trash2, 
  RefreshCw, 
  Globe, 
  Terminal, 
  AlertTriangle, 
  CheckCircle,
  XCircle,
  Server,
  ChevronDown
} from 'lucide-react';
import { NexusBridge } from '../lib/nexus-bridge';
import { cn } from '../lib/utils';

interface McpServerInfo {
  name: string;
  url?: string;
  command?: string;
  args?: string[];
  active?: boolean;
  running?: boolean;
  tools?: { name: string; description: string }[];
}

interface McpRegistryPanelProps {
  bridge: NexusBridge | null;
  notify: (msg: string, type?: 'info' | 'error') => void;
}

function parseArgs(input: string): string[] {
  const result: string[] = [];
  let current = '';
  let inQuotes = false;
  let quoteChar = '';

  for (let i = 0; i < input.length; i++) {
    const char = input[i];
    if (inQuotes) {
      if (char === quoteChar) {
        inQuotes = false;
      } else {
        current += char;
      }
    } else {
      if (char === '"' || char === "'") {
        inQuotes = true;
        quoteChar = char;
      } else if (char === ',') {
        result.push(current.trim());
        current = '';
      } else {
        current += char;
      }
    }
  }
  result.push(current.trim());
  return result.filter(Boolean);
}

export function McpRegistryPanel({ bridge, notify }: McpRegistryPanelProps) {
  const [servers, setServers] = useState<McpServerInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  // Modal states
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [newName, setNewName] = useState('');
  const [serverType, setServerType] = useState<'url' | 'command'>('url');
  const [newUrl, setNewUrl] = useState('');
  const [newCommand, setNewCommand] = useState('');
  const [newArgs, setNewArgs] = useState('');
  const [submitting, setSubmitting] = useState(false);
  
  // Delete confirm state
  const [confirmDeleteName, setConfirmDeleteName] = useState<string | null>(null);

  // Active toggle & expand states
  const [togglingName, setTogglingName] = useState<string | null>(null);
  const [expandedServers, setExpandedServers] = useState<Record<string, boolean>>({});

  const handleToggleActive = async (name: string, active: boolean) => {
    if (!bridge) return;
    setTogglingName(name);
    try {
      await bridge.toggleMcpExternal(name, active);
      notify(`Successfully ${active ? 'activated' : 'deactivated'} MCP server: ${name}`, 'info');
      fetchServers(true);
    } catch (err: any) {
      notify(err.message || `Failed to update MCP server status.`, 'error');
    } finally {
      setTogglingName(null);
    }
  };

  const toggleExpand = (name: string) => {
    setExpandedServers(prev => ({ ...prev, [name]: !prev[name] }));
  };

  const fetchServers = async (silent = false) => {
    if (!bridge) return;
    if (!silent) setLoading(true);
    try {
      const data = await bridge.listMcpExternal();
      setServers(Array.isArray(data) ? data : []);
      setError(null);
    } catch (err: any) {
      setError(err.message || 'Failed to retrieve MCP registry.');
    } finally {
      if (!silent) setLoading(false);
    }
  };

  useEffect(() => {
    fetchServers();
    const interval = setInterval(() => fetchServers(true), 30000);
    return () => clearInterval(interval);
  }, [bridge]);

  const handleAddServer = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!bridge) return;
    if (!newName.trim()) {
      notify('Server name is required.', 'error');
      return;
    }

    setSubmitting(true);
    try {
      const payload: any = { name: newName.trim() };
      if (serverType === 'url') {
        if (!newUrl.trim()) {
          notify('Server URL is required.', 'error');
          setSubmitting(false);
          return;
        }
        payload.url = newUrl.trim();
      } else {
        if (!newCommand.trim()) {
          notify('Command is required.', 'error');
          setSubmitting(false);
          return;
        }
        payload.command = newCommand.trim();
        if (newArgs.trim()) {
          payload.args = parseArgs(newArgs);
        }
      }

      await bridge.addMcpExternal(payload);
      notify(`Successfully added MCP server: ${newName}`, 'info');
      
      // Reset form and close
      setNewName('');
      setNewUrl('');
      setNewCommand('');
      setNewArgs('');
      setIsModalOpen(false);
      
      // Refresh list
      fetchServers();
    } catch (err: any) {
      notify(err.message || 'Failed to add MCP server.', 'error');
    } finally {
      setSubmitting(false);
    }
  };

  const handleDeleteServer = async (name: string) => {
    if (!bridge) return;
    try {
      await bridge.removeMcpExternal(name);
      notify(`Removed MCP server: ${name}`, 'info');
      setConfirmDeleteName(null);
      fetchServers();
    } catch (err: any) {
      notify(err.message || 'Failed to remove MCP server.', 'error');
    }
  };

  return (
    <div className="space-y-6">
      {/* Top Banner / Actions */}
      <div className="flex items-center justify-between gap-4 flex-wrap">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 bg-emerald-500/10 border border-emerald-500/20 rounded-xl flex items-center justify-center">
            <Plug className="w-5 h-5 text-emerald-400" />
          </div>
          <div>
            <h2 className="text-lg font-bold text-white tracking-tight uppercase">MCP External Registry</h2>
            <p className="text-xs text-slate-500 font-mono">Configure external Model Context Protocol server links</p>
          </div>
        </div>

        <div className="flex items-center gap-3">
          <button
            onClick={() => fetchServers()}
            disabled={loading}
            className="p-2.5 rounded-xl border border-slate-800 hover:border-slate-700 bg-slate-900/50 hover:bg-slate-900 text-slate-400 hover:text-slate-200 transition-colors disabled:opacity-50"
            title="Refresh list"
          >
            <RefreshCw className={cn("w-4 h-4", loading && "animate-spin")} />
          </button>

          <button
            onClick={() => setIsModalOpen(true)}
            className="flex items-center gap-2 px-4 py-2 bg-emerald-500 hover:bg-emerald-600 text-slate-950 rounded-xl text-xs font-bold transition-all shadow-lg shadow-emerald-500/10 hover:shadow-emerald-500/20 active:scale-95"
          >
            <Plus className="w-4 h-4" /> Add Server
          </button>
        </div>
      </div>

      {/* Main Content Grid */}
      {error && (
        <div className="p-4 bg-red-500/10 border border-red-500/20 rounded-2xl flex items-start gap-3">
          <AlertTriangle className="w-5 h-5 text-red-500 shrink-0 mt-0.5" />
          <div>
            <h4 className="text-sm font-bold text-red-400">Registry Connection Error</h4>
            <p className="text-xs text-red-500/80 mt-1">{error}</p>
          </div>
        </div>
      )}

      {loading && servers.length === 0 ? (
        <div className="flex flex-col items-center justify-center py-20 text-slate-500">
          <RefreshCw className="w-8 h-8 animate-spin text-emerald-500/50 mb-3" />
          <p className="text-xs font-mono">Scanning external MCP registry...</p>
        </div>
      ) : servers.length === 0 ? (
        <div className="border border-dashed border-slate-800 rounded-2xl flex flex-col items-center justify-center py-16 px-4 bg-slate-900/10 text-center">
          <div className="w-12 h-12 bg-slate-900 border border-slate-800 rounded-xl flex items-center justify-center text-slate-500 mb-3">
            <Server className="w-6 h-6" />
          </div>
          <h3 className="text-sm font-bold text-slate-300 uppercase">No External MCP Servers</h3>
          <p className="text-xs text-slate-500 max-w-sm mt-1">
            Connect public HTTP endpoints or local command-line tools to extend Tylluan capabilities.
          </p>
          <button
            onClick={() => setIsModalOpen(true)}
            className="mt-4 flex items-center gap-2 px-4 py-2 bg-slate-900 hover:bg-slate-800 text-slate-300 rounded-xl text-xs border border-slate-800 transition-colors"
          >
            <Plus className="w-3.5 h-3.5" /> Register first server
          </button>
        </div>
      ) : (
        <div className="border border-slate-800/80 bg-slate-900/30 rounded-2xl overflow-hidden backdrop-blur-md">
          <div className="overflow-x-auto">
            <table className="w-full text-left border-collapse">
              <thead>
                <tr className="border-b border-slate-800/80 text-[10px] uppercase tracking-wider text-slate-500 font-mono font-bold bg-slate-950/40">
                  <th className="py-3 px-5">Name</th>
                  <th className="py-3 px-5">Type / Location</th>
                  <th className="py-3 px-5">Status</th>
                  <th className="py-3 px-5">Active</th>
                  <th className="py-3 px-5 text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-800/40 text-sm">
                {servers.map((server) => {
                  const isUrl = !!server.url;
                  const hasTools = server.tools && server.tools.length > 0;
                  const isExpanded = !!expandedServers[server.name];
                  
                  return (
                    <React.Fragment key={server.name}>
                      <tr className="hover:bg-slate-900/25 transition-colors">
                        <td className="py-4 px-5 font-mono font-bold text-slate-200">
                          <div className="flex items-center gap-2">
                            {hasTools && (
                              <button
                                type="button"
                                onClick={() => toggleExpand(server.name)}
                                className="p-1 rounded hover:bg-slate-800 text-slate-500 hover:text-slate-300 transition-colors"
                              >
                                <ChevronDown className={cn("w-3.5 h-3.5 transition-transform duration-200", isExpanded && "rotate-180")} />
                              </button>
                            )}
                            <span>{server.name}</span>
                          </div>
                        </td>
                        <td className="py-4 px-5">
                          {isUrl ? (
                            <div className="flex items-center gap-2">
                              <Globe className="w-3.5 h-3.5 text-blue-400 flex-shrink-0" />
                              <span className="text-xs text-slate-400 font-mono truncate max-w-[280px]" title={server.url}>
                                {server.url}
                              </span>
                            </div>
                          ) : (
                            <div className="space-y-1">
                              <div className="flex items-center gap-2">
                                <Terminal className="w-3.5 h-3.5 text-violet-400 flex-shrink-0" />
                                <span className="text-xs text-slate-300 font-mono truncate max-w-[200px]" title={server.command}>
                                  {server.command}
                                </span>
                              </div>
                              {server.args && server.args.length > 0 && (
                                <div className="flex items-center gap-1 flex-wrap pl-5">
                                  {server.args.map((arg, idx) => (
                                    <span key={idx} className="px-1.5 py-0.5 rounded bg-slate-800 text-[9px] text-slate-500 font-mono border border-slate-700/50">
                                      {arg}
                                    </span>
                                  ))}
                                </div>
                              )}
                            </div>
                          )}
                        </td>
                        <td className="py-4 px-5">
                          {server.active === false ? (
                            <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[10px] font-black border tracking-tight uppercase bg-slate-800 text-slate-500 border-slate-700">
                              <span className="w-1.5 h-1.5 rounded-full bg-slate-600" />
                              Inactive
                            </span>
                          ) : server.running ? (
                            <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[10px] font-black border tracking-tight uppercase bg-emerald-500/10 text-emerald-400 border-emerald-500/20 shadow-[0_0_8px_rgba(16,185,129,0.1)]">
                              <span className="w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                              Running
                            </span>
                          ) : (
                            <span className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-[10px] font-black border tracking-tight uppercase bg-amber-500/10 text-amber-400 border-amber-500/20">
                              <span className="w-1.5 h-1.5 rounded-full bg-amber-400 animate-pulse" />
                              Offline
                            </span>
                          )}
                        </td>
                        <td className="py-4 px-5">
                          <button
                            type="button"
                            onClick={() => handleToggleActive(server.name, server.active === false)}
                            disabled={togglingName === server.name}
                            className={cn(
                              "relative inline-flex h-5 w-9 shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none disabled:opacity-50",
                              server.active !== false ? "bg-emerald-500" : "bg-slate-800"
                            )}
                          >
                            <span
                              className={cn(
                                "pointer-events-none inline-block h-3.5 w-3.5 transform rounded-full bg-slate-950 shadow ring-0 transition duration-200 ease-in-out",
                                server.active !== false ? "translate-x-4" : "translate-x-0"
                              )}
                            />
                          </button>
                        </td>
                        <td className="py-4 px-5 text-right">
                          {confirmDeleteName === server.name ? (
                            <div className="flex items-center justify-end gap-1.5">
                              <button
                                type="button"
                                onClick={() => handleDeleteServer(server.name)}
                                className="px-2 py-1 bg-red-500 hover:bg-red-600 text-slate-950 font-bold rounded text-[10px] uppercase transition-colors"
                              >
                                Confirm
                              </button>
                              <button
                                type="button"
                                onClick={() => setConfirmDeleteName(null)}
                                className="px-2 py-1 bg-slate-800 hover:bg-slate-700 text-slate-400 rounded text-[10px] uppercase border border-slate-700 transition-colors"
                              >
                                Cancel
                              </button>
                            </div>
                          ) : (
                            <button
                              type="button"
                              onClick={() => setConfirmDeleteName(server.name)}
                              className="p-2 bg-slate-850 hover:bg-red-500/15 text-slate-500 hover:text-red-400 border border-slate-800/80 rounded-xl transition-all active:scale-95"
                              title="Delete server link"
                            >
                              <Trash2 className="w-3.5 h-3.5" />
                            </button>
                          )}
                        </td>
                      </tr>
                      {isExpanded && hasTools && (
                        <tr className="bg-slate-950/20 border-b border-slate-800/40">
                          <td colSpan={5} className="py-3 px-8">
                            <div className="space-y-2">
                              <h4 className="text-[10px] font-bold uppercase tracking-wider text-slate-500 font-mono">Exposed Tools ({server.tools!.length})</h4>
                              <div className="grid grid-cols-1 md:grid-cols-2 gap-2">
                                {server.tools!.map((tool) => (
                                  <div key={tool.name} className="p-2.5 rounded-xl bg-slate-950/40 border border-slate-850/80 font-mono">
                                    <div className="text-xs font-bold text-emerald-400">{tool.name}</div>
                                    {tool.description && (
                                      <div className="text-[10px] text-slate-400 mt-1 leading-relaxed">{tool.description}</div>
                                    )}
                                  </div>
                                ))}
                              </div>
                            </div>
                          </td>
                        </tr>
                      )}
                    </React.Fragment>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* Add Server Modal Dialog */}
      {isModalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm animate-in fade-in duration-200">
          <div 
            className="w-full max-w-lg bg-slate-900 border border-slate-850 rounded-2xl shadow-2xl p-6 relative flex flex-col space-y-4 animate-in zoom-in-95 duration-200"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Modal Header */}
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Plug className="w-5 h-5 text-emerald-400" />
                <h3 className="text-md font-bold text-white uppercase tracking-tight">Register MCP Server</h3>
              </div>
              <button
                onClick={() => setIsModalOpen(false)}
                className="text-slate-500 hover:text-slate-300 font-mono text-sm px-2 py-1 rounded hover:bg-slate-800"
              >
                ✕
              </button>
            </div>

            {/* Modal Form */}
            <form onSubmit={handleAddServer} className="space-y-4">
              {/* Server Name */}
              <div className="space-y-1.5">
                <label className="block text-[10px] font-bold text-slate-500 uppercase tracking-wider font-mono">Server Alias</label>
                <input
                  type="text"
                  required
                  placeholder="e.g., memory-service"
                  value={newName}
                  onChange={(e) => setNewName(e.target.value)}
                  className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:outline-none text-sm text-slate-200 font-mono"
                />
              </div>

              {/* Server Type selector */}
              <div className="space-y-1.5">
                <label className="block text-[10px] font-bold text-slate-500 uppercase tracking-wider font-mono">Connection Protocol</label>
                <div className="grid grid-cols-2 gap-3">
                  <button
                    type="button"
                    onClick={() => setServerType('url')}
                    className={cn(
                      "flex items-center justify-center gap-2 p-3 rounded-xl border font-bold text-xs uppercase transition-all",
                      serverType === 'url'
                        ? "bg-emerald-500/10 text-emerald-400 border-emerald-500/30"
                        : "bg-slate-950 text-slate-500 border-slate-850 hover:border-slate-800"
                    )}
                  >
                    <Globe className="w-4 h-4" /> HTTP/SSE URL
                  </button>
                  <button
                    type="button"
                    onClick={() => setServerType('command')}
                    className={cn(
                      "flex items-center justify-center gap-2 p-3 rounded-xl border font-bold text-xs uppercase transition-all",
                      serverType === 'command'
                        ? "bg-violet-500/10 text-violet-400 border-violet-500/30"
                        : "bg-slate-950 text-slate-500 border-slate-850 hover:border-slate-800"
                    )}
                  >
                    <Terminal className="w-4 h-4" /> Stdio / Command
                  </button>
                </div>
              </div>

              {/* Conditional Inputs */}
              {serverType === 'url' ? (
                <div className="space-y-1.5 animate-in slide-in-from-top-2 duration-150">
                  <label className="block text-[10px] font-bold text-slate-500 uppercase tracking-wider font-mono">SSE Server Endpoint URL</label>
                  <input
                    type="url"
                    required={serverType === 'url'}
                    placeholder="http://localhost:5678/mcp-server/http"
                    value={newUrl}
                    onChange={(e) => setNewUrl(e.target.value)}
                    className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:outline-none text-sm text-slate-200 font-mono"
                  />
                </div>
              ) : (
                <div className="space-y-3 animate-in slide-in-from-top-2 duration-150">
                  <div className="space-y-1.5">
                    <label className="block text-[10px] font-bold text-slate-500 uppercase tracking-wider font-mono">Shell Executable Command</label>
                    <input
                      type="text"
                      required={serverType === 'command'}
                      placeholder="e.g., node"
                      value={newCommand}
                      onChange={(e) => setNewCommand(e.target.value)}
                      className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:outline-none text-sm text-slate-200 font-mono"
                    />
                  </div>
                  <div className="space-y-1.5">
                    <div className="flex justify-between items-center">
                      <label className="block text-[10px] font-bold text-slate-500 uppercase tracking-wider font-mono">Arguments (comma-separated)</label>
                      <span className="text-[9px] text-slate-600 font-mono">Optional</span>
                    </div>
                    <input
                      type="text"
                      placeholder="e.g., /path/to/server.js, --db, tylluan"
                      value={newArgs}
                      onChange={(e) => setNewArgs(e.target.value)}
                      className="w-full px-3.5 py-2 rounded-xl bg-slate-950 border border-slate-800 focus:border-emerald-500 focus:outline-none text-sm text-slate-200 font-mono"
                    />
                  </div>
                </div>
              )}

              {/* Action Buttons */}
              <div className="flex gap-3 justify-end pt-4 border-t border-slate-850">
                <button
                  type="button"
                  onClick={() => setIsModalOpen(false)}
                  className="px-4 py-2 border border-slate-800 hover:border-slate-700 bg-slate-950 hover:bg-slate-900 text-slate-400 hover:text-slate-200 rounded-xl text-xs font-bold transition-colors"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={submitting}
                  className="px-5 py-2 bg-emerald-500 hover:bg-emerald-600 text-slate-950 rounded-xl text-xs font-bold transition-all disabled:opacity-50 flex items-center gap-2"
                >
                  {submitting && <RefreshCw className="w-3 h-3 animate-spin" />}
                  Register Server
                </button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  );
}
